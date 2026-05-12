//! OBS-1 — Prometheus metrics for the Declaration service.
//!
//! One [`Metrics`] handle is constructed at service startup and shared
//! via `Arc<_>` through:
//!   * the Tower middleware on the REST router ([`http_metrics_layer`]),
//!   * the gRPC service shell (`api::grpc`),
//!   * the OIDC auth middleware (per-verify counter + JWKS-fetch histogram),
//!   * the domain use-case handlers (submit / amend / correct counters),
//!   * the DLQ admin + outbox-relay paths (DLQ size gauge + replay counter),
//!   * the relay loop (delivery-latency histogram).
//!
//! The exposition endpoint is mounted at `GET /metrics` in
//! `crate::api::rest`. It is intentionally NOT documented in the OpenAPI
//! spec — operational endpoints are not consumer contract surface. See
//! the `// OBS-1` comment in `crate::api::openapi`.
//!
//! ## Doctrine compliance
//!
//! - **D14 fail-closed**: `gather_text()` returns `Err` rather than
//!   panicking if the underlying Prometheus encoder fails. The handler
//!   converts that into an HTTP 500 with a short text body — the service
//!   keeps serving real traffic regardless.
//! - **D16 observability**: every new error path that warrants
//!   alerting MUST add a counter or histogram here in the same PR
//!   that introduces the path.
//! - **D17 zero trust**: `/metrics` carries no authentication. The
//!   deployment expectation (documented in
//!   `docs/runbooks/observability-dashboards.md`) is in-cluster network
//!   only. The endpoint is rate-limit-exempt.
//! - **D18 no secrets / label cardinality**: every label value MUST be a
//!   bounded enum or a numerically-scoped path template. Forbidden:
//!   `principal`, `declaration_id`, `case_id`, any UUID/email/free
//!   string. The compiled-in `&'static str` label tables below enforce
//!   this by construction. New labels MUST be enumerated; high-cardinality
//!   labels blow up Prometheus memory and are how an internal telemetry
//!   surface accidentally leaks PII.

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::MatchedPath, http::Request, middleware::Next, response::Response};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};

// ─── Shared registry handle ──────────────────────────────────────────

/// Owns the Prometheus [`Registry`] and every collector this service
/// exports. Clone via `Arc<Metrics>`; the inner registry is itself
/// cheaply cloneable but we wrap the whole bundle in an `Arc` so
/// callers don't have to repeat the wrapping at every state struct.
#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    /// HTTP request counter — labels: `method`, `path`, `status`.
    /// `path` is the axum *matched path* template (e.g.
    /// `/v1/declarations/{declaration_id}`), never the concrete URL.
    pub http_requests_total: IntCounterVec,
    /// HTTP request latency in seconds — labels: `method`, `path`.
    pub http_request_duration_seconds: HistogramVec,

    // Domain counters (submit / amend / correct).
    pub declarations_submitted_total: IntCounterVec,
    pub declarations_amended_total: IntCounterVec,
    pub declarations_corrected_total: IntCounterVec,

    // Outbox-relay metrics.
    /// Undispatched outbox rows. Gauge — updated by the relay loop
    /// itself, not derived from incrementing counters.
    pub outbox_undispatched: IntGauge,
    /// Dead-letter-queue current size. Gauge — sampled by the DLQ store.
    pub outbox_dlq_size: IntGauge,
    /// Counter of DLQ replays — label: `result` ∈ {"success","failure"}.
    pub outbox_dlq_replays_total: IntCounterVec,
    /// Histogram of relay delivery latency in seconds. The relay records
    /// the wall-clock time from outbox row creation to successful 2xx.
    pub relay_delivery_latency_seconds: HistogramVec,

    /// COMP-2 — outbox retention prune counter. Label
    /// `result` ∈ {"success","error"}: `success` increments BY the
    /// number of rows pruned in the cycle; `error` increments BY 1
    /// when a cycle fails before reaching the row count.
    pub outbox_retention_pruned_total: IntCounterVec,

    // OIDC verifier metrics (shared shape with V-engine).
    /// JWKS fetch latency histogram.
    pub oidc_jwks_fetch_latency_seconds: HistogramVec,
    /// OIDC verify outcomes — label: `result` ∈
    /// {"success","invalid","unavailable"}.
    pub oidc_verify_total: IntCounterVec,

    /// Health-check (readyz/healthz) latency histogram.
    pub health_check_duration_seconds: HistogramVec,
}

/// HTTP latency buckets in seconds. Tuned for a Postgres-backed REST
/// service: most happy-path calls finish in tens of ms; we keep the
/// long tail visible up to 30s (the default tower timeout). Lower &
/// upper bounds match Grafana panel expectations downstream.
const HTTP_LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// Relay delivery latency buckets — broader than HTTP because we expect
/// at-least-once delivery to span seconds → minutes when a subscriber
/// is slow or temporarily down. The 30s bucket lines up with the OBS-1
/// alert threshold (`RecorRelayLatencyHigh`).
const RELAY_LATENCY_BUCKETS: &[f64] =
    &[0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0, 900.0];

/// Tight JWKS-fetch buckets: a healthy fetch is sub-second; a sad fetch
/// shows up as the long tail.
const JWKS_FETCH_BUCKETS: &[f64] =
    &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];

/// Health-check buckets: should be near-instant (DB ping).
const HEALTH_CHECK_BUCKETS: &[f64] =
    &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0];

impl Metrics {
    /// Build a fresh registry + all collectors. Fails only if the
    /// underlying Prometheus crate refuses to register a collector
    /// (which would indicate a programming error here, not runtime).
    pub fn new() -> Result<Arc<Self>, prometheus::Error> {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new(
                "http_requests_total",
                "Total HTTP requests handled by the service, labelled by method, matched path template, and status code.",
            ),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request latency in seconds — wall-clock between handler entry and response. Labelled by method + matched-path template (no concrete IDs).",
            )
            .buckets(HTTP_LATENCY_BUCKETS.to_vec()),
            &["method", "path"],
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        let declarations_submitted_total = IntCounterVec::new(
            Opts::new(
                "recor_declarations_submitted_total",
                "Declarations successfully submitted (POST /v1/declarations). Increment is AFTER the use case persists; rejects/4xx do not increment this.",
            ),
            &["kind"],
        )?;
        registry.register(Box::new(declarations_submitted_total.clone()))?;

        let declarations_amended_total = IntCounterVec::new(
            Opts::new(
                "recor_declarations_amended_total",
                "Declarations amended in place (POST /v1/declarations/{id}/amend). Increment is AFTER the use case persists.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(declarations_amended_total.clone()))?;

        let declarations_corrected_total = IntCounterVec::new(
            Opts::new(
                "recor_declarations_corrected_total",
                "Declarations corrected (POST /v1/declarations/{id}/correct). Increment is AFTER the use case persists.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(declarations_corrected_total.clone()))?;

        let outbox_undispatched = IntGauge::with_opts(Opts::new(
            "recor_outbox_undispatched",
            "Current number of outbox rows awaiting dispatch (dispatched_at IS NULL). Sampled by the relay loop each poll.",
        ))?;
        registry.register(Box::new(outbox_undispatched.clone()))?;

        let outbox_dlq_size = IntGauge::with_opts(Opts::new(
            "recor_outbox_dlq_size",
            "Current number of rows in the outbox dead-letter queue. Sampled by the relay loop + DLQ admin store.",
        ))?;
        registry.register(Box::new(outbox_dlq_size.clone()))?;

        let outbox_dlq_replays_total = IntCounterVec::new(
            Opts::new(
                "recor_outbox_dlq_replays_total",
                "DLQ replay attempts. result=success ⇒ row moved back to outbox; result=failure ⇒ replay refused (not_found / backend error).",
            ),
            &["result"],
        )?;
        registry.register(Box::new(outbox_dlq_replays_total.clone()))?;

        let relay_delivery_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_relay_delivery_latency_seconds",
                "End-to-end outbox-relay delivery latency: outbox row creation → successful 2xx response from the subscriber. Labelled by subscriber name.",
            )
            .buckets(RELAY_LATENCY_BUCKETS.to_vec()),
            &["subscriber"],
        )?;
        registry.register(Box::new(relay_delivery_latency_seconds.clone()))?;

        let outbox_retention_pruned_total = IntCounterVec::new(
            Opts::new(
                "recor_outbox_retention_pruned_total",
                "COMP-2 outbox retention prune counter. result=success increments BY rows-pruned per cycle; result=error increments BY 1 per failed cycle.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(outbox_retention_pruned_total.clone()))?;

        let oidc_jwks_fetch_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_oidc_jwks_fetch_latency_seconds",
                "OIDC JWKS endpoint fetch latency in seconds. Labelled by result ∈ {success,failure}.",
            )
            .buckets(JWKS_FETCH_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(oidc_jwks_fetch_latency_seconds.clone()))?;

        let oidc_verify_total = IntCounterVec::new(
            Opts::new(
                "recor_oidc_verify_total",
                "OIDC token-verification outcomes. result=success: token verified; result=invalid: client-side fault (bad sig, expired, unknown kid); result=unavailable: discovery/JWKS endpoint unreachable.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(oidc_verify_total.clone()))?;

        let health_check_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_health_check_duration_seconds",
                "Health probe latency (healthz + readyz). Labelled by probe ∈ {healthz,readyz}.",
            )
            .buckets(HEALTH_CHECK_BUCKETS.to_vec()),
            &["probe"],
        )?;
        registry.register(Box::new(health_check_duration_seconds.clone()))?;

        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            declarations_submitted_total,
            declarations_amended_total,
            declarations_corrected_total,
            outbox_undispatched,
            outbox_dlq_size,
            outbox_dlq_replays_total,
            relay_delivery_latency_seconds,
            outbox_retention_pruned_total,
            oidc_jwks_fetch_latency_seconds,
            oidc_verify_total,
            health_check_duration_seconds,
        }))
    }

    /// Render the registry to Prometheus exposition format.
    /// Returns the body bytes + the canonical Content-Type header value.
    pub fn gather_text(&self) -> Result<(Vec<u8>, &'static str), prometheus::Error> {
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut buf = Vec::with_capacity(8 * 1024);
        encoder.encode(&metric_families, &mut buf)?;
        Ok((buf, "text/plain; version=0.0.4; charset=utf-8"))
    }
}

// ─── REST: Tower middleware that times every request ─────────────────

/// Axum middleware: increment the request counter + observe the latency
/// histogram for every handled request. Applied at the router root in
/// `crate::api::rest`.
///
/// Path label resolution: we read the *matched path* extension that
/// axum populates on routed requests (e.g.
/// `/v1/declarations/{declaration_id}`). When no template matches
/// (404, malformed URL) we use the literal `unmatched` label to keep
/// label cardinality bounded — D18.
pub async fn metrics_middleware<B>(
    axum::extract::State(metrics): axum::extract::State<Arc<Metrics>>,
    matched: Option<MatchedPath>,
    req: Request<B>,
    next: Next,
) -> Response
where
    B: Send + 'static,
    axum::body::Body: From<B>,
{
    let method = req.method().clone();
    let path = matched
        .as_ref()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());

    let start = Instant::now();
    let req: Request<axum::body::Body> = req.map(axum::body::Body::from);
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    let status = response.status().as_u16().to_string();
    let method_str = method.as_str();

    // Best-effort; never block the response on metric registration errors.
    metrics
        .http_request_duration_seconds
        .with_label_values(&[method_str, path.as_str()])
        .observe(elapsed);
    metrics
        .http_requests_total
        .with_label_values(&[method_str, path.as_str(), status.as_str()])
        .inc();

    response
}

// ─── /metrics handler ────────────────────────────────────────────────

/// HTTP handler for `GET /metrics`. Returns Prometheus exposition text.
/// D14 fail-closed: if `gather_text` fails, returns 500 with a short
/// error body — the service keeps running and serving real traffic.
pub async fn metrics_handler(
    axum::extract::State(metrics): axum::extract::State<Arc<Metrics>>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    match metrics.gather_text() {
        Ok((body, content_type)) => {
            ([(header::CONTENT_TYPE, content_type)], body).into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, "metrics encode failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "metrics encoding failure",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_constructs_with_all_metrics() {
        let m = Metrics::new().expect("registry constructs");
        // Touch every counter/histogram once to ensure they're actually
        // registered (Prometheus only emits a metric once its first
        // labelled sample is observed).
        m.http_requests_total
            .with_label_values(&["GET", "/healthz", "200"])
            .inc();
        m.http_request_duration_seconds
            .with_label_values(&["GET", "/healthz"])
            .observe(0.01);
        m.declarations_submitted_total
            .with_label_values(&["incorporation"])
            .inc();
        m.declarations_amended_total
            .with_label_values(&["success"])
            .inc();
        m.declarations_corrected_total
            .with_label_values(&["success"])
            .inc();
        m.outbox_undispatched.set(3);
        m.outbox_dlq_size.set(2);
        m.outbox_dlq_replays_total
            .with_label_values(&["success"])
            .inc();
        m.relay_delivery_latency_seconds
            .with_label_values(&["verification-engine"])
            .observe(0.5);
        m.outbox_retention_pruned_total
            .with_label_values(&["success"])
            .inc_by(7);
        m.oidc_jwks_fetch_latency_seconds
            .with_label_values(&["success"])
            .observe(0.05);
        m.oidc_verify_total
            .with_label_values(&["success"])
            .inc();
        m.health_check_duration_seconds
            .with_label_values(&["readyz"])
            .observe(0.002);

        let (body, _ct) = m.gather_text().expect("encodes");
        let text = String::from_utf8(body).expect("utf-8");
        for name in [
            "http_requests_total",
            "http_request_duration_seconds",
            "recor_declarations_submitted_total",
            "recor_declarations_amended_total",
            "recor_declarations_corrected_total",
            "recor_outbox_undispatched",
            "recor_outbox_dlq_size",
            "recor_outbox_dlq_replays_total",
            "recor_relay_delivery_latency_seconds",
            "recor_outbox_retention_pruned_total",
            "recor_oidc_jwks_fetch_latency_seconds",
            "recor_oidc_verify_total",
            "recor_health_check_duration_seconds",
        ] {
            assert!(
                text.contains(name),
                "exposition missing metric `{name}`; got:\n{text}"
            );
        }
    }

    #[test]
    fn content_type_is_prometheus_0_0_4() {
        let m = Metrics::new().expect("registry constructs");
        let (_body, ct) = m.gather_text().expect("encodes");
        assert!(
            ct.starts_with("text/plain; version=0.0.4"),
            "wrong Content-Type: {ct}"
        );
    }

    #[test]
    fn registry_is_fresh_per_instance() {
        // Two Metrics instances must not share state — important for
        // tests that build a fresh router per case.
        let a = Metrics::new().unwrap();
        let b = Metrics::new().unwrap();
        a.declarations_submitted_total
            .with_label_values(&["incorporation"])
            .inc();
        let (a_body, _) = a.gather_text().unwrap();
        let (b_body, _) = b.gather_text().unwrap();
        let a_text = String::from_utf8(a_body).unwrap();
        let b_text = String::from_utf8(b_body).unwrap();
        // `a` should record the increment; `b` should not have seen it
        // (or should have the metric line with value 0 — assert by
        // checking for a non-zero sample).
        assert!(
            a_text.contains("recor_declarations_submitted_total{kind=\"incorporation\"} 1"),
            "a should have value 1; got:\n{a_text}"
        );
        assert!(
            !b_text.contains("recor_declarations_submitted_total{kind=\"incorporation\"} 1"),
            "b should NOT have inherited a's sample; got:\n{b_text}"
        );
    }
}
