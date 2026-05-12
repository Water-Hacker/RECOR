//! OBS-1 — Prometheus metrics for the Verification Engine.
//!
//! Mirrors `recor_declaration::metrics` in shape but exports a
//! verification-engine-specific metric set:
//!
//! * the shared HTTP/OIDC/health metrics every service emits, plus
//! * `recor_verification_cases_total{lane}` — per-lane counter,
//! * `recor_fusion_belief_true` / `recor_fusion_belief_false` —
//!   histograms over the fused authenticity belief mass so operators
//!   can see drift in the lane router's input distribution.
//!
//! D17 zero trust: `/metrics` is internal-only — see the deployment
//! runbook for the in-cluster network expectation. D18 no high-card
//! labels: lane is a 3-value enum, result/probe are bounded enums.

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::MatchedPath, http::Request, middleware::Next, response::Response};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    /// HTTP request counter — labels: method, path, status.
    pub http_requests_total: IntCounterVec,
    /// HTTP request latency in seconds — labels: method, path.
    pub http_request_duration_seconds: HistogramVec,

    /// Verification cases by lane — label: `lane` ∈ {green,yellow,red}.
    pub verification_cases_total: IntCounterVec,
    /// DLQ size gauge (mirrors the same metric in the Declaration service).
    pub outbox_dlq_size: IntGauge,
    /// Counter of DLQ replays — label: `result` ∈ {success,failure}.
    pub outbox_dlq_replays_total: IntCounterVec,

    /// Distribution of the fused authenticity belief mass over the
    /// "true" hypothesis. Used to detect drift in lane-router input.
    pub fusion_belief_true: HistogramVec,
    /// Distribution of the fused authenticity belief mass over the
    /// "false" hypothesis.
    pub fusion_belief_false: HistogramVec,

    /// OIDC JWKS fetch latency. Labelled by result.
    pub oidc_jwks_fetch_latency_seconds: HistogramVec,
    /// OIDC verify outcomes — label: `result` ∈
    /// {success,invalid,unavailable}.
    pub oidc_verify_total: IntCounterVec,

    /// Health-probe latency.
    pub health_check_duration_seconds: HistogramVec,
}

const HTTP_LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// Belief masses are unit-interval values; eleven buckets give a fine
/// view of where the lane router's input distribution sits.
const BELIEF_BUCKETS: &[f64] =
    &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];

const JWKS_FETCH_BUCKETS: &[f64] =
    &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];

const HEALTH_CHECK_BUCKETS: &[f64] =
    &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0];

impl Metrics {
    pub fn new() -> Result<Arc<Self>, prometheus::Error> {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new(
                "http_requests_total",
                "Total HTTP requests handled, labelled by method, matched path template, and status.",
            ),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request latency in seconds. Labelled by method + matched-path template (no IDs).",
            )
            .buckets(HTTP_LATENCY_BUCKETS.to_vec()),
            &["method", "path"],
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        let verification_cases_total = IntCounterVec::new(
            Opts::new(
                "recor_verification_cases_total",
                "Verification cases adjudicated, by routed lane. lane ∈ {green,yellow,red}.",
            ),
            &["lane"],
        )?;
        registry.register(Box::new(verification_cases_total.clone()))?;

        let outbox_dlq_size = IntGauge::with_opts(Opts::new(
            "recor_outbox_dlq_size",
            "Current size of the verification-outbox DLQ. Sampled by the DLQ admin store.",
        ))?;
        registry.register(Box::new(outbox_dlq_size.clone()))?;

        let outbox_dlq_replays_total = IntCounterVec::new(
            Opts::new(
                "recor_outbox_dlq_replays_total",
                "Verification-outbox DLQ replay outcomes. result ∈ {success,failure}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(outbox_dlq_replays_total.clone()))?;

        let fusion_belief_true = HistogramVec::new(
            HistogramOpts::new(
                "recor_fusion_belief_true",
                "Distribution of fused authenticity belief on the `true` hypothesis. One observation per adjudicated case.",
            )
            .buckets(BELIEF_BUCKETS.to_vec()),
            &["lane"],
        )?;
        registry.register(Box::new(fusion_belief_true.clone()))?;

        let fusion_belief_false = HistogramVec::new(
            HistogramOpts::new(
                "recor_fusion_belief_false",
                "Distribution of fused authenticity belief on the `false` hypothesis. One observation per adjudicated case.",
            )
            .buckets(BELIEF_BUCKETS.to_vec()),
            &["lane"],
        )?;
        registry.register(Box::new(fusion_belief_false.clone()))?;

        let oidc_jwks_fetch_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_oidc_jwks_fetch_latency_seconds",
                "JWKS fetch latency in seconds. Labelled by result.",
            )
            .buckets(JWKS_FETCH_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(oidc_jwks_fetch_latency_seconds.clone()))?;

        let oidc_verify_total = IntCounterVec::new(
            Opts::new(
                "recor_oidc_verify_total",
                "OIDC token-verification outcomes. result ∈ {success,invalid,unavailable}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(oidc_verify_total.clone()))?;

        let health_check_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_health_check_duration_seconds",
                "Health probe latency. probe ∈ {healthz,readyz}.",
            )
            .buckets(HEALTH_CHECK_BUCKETS.to_vec()),
            &["probe"],
        )?;
        registry.register(Box::new(health_check_duration_seconds.clone()))?;

        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            verification_cases_total,
            outbox_dlq_size,
            outbox_dlq_replays_total,
            fusion_belief_true,
            fusion_belief_false,
            oidc_jwks_fetch_latency_seconds,
            oidc_verify_total,
            health_check_duration_seconds,
        }))
    }

    pub fn gather_text(&self) -> Result<(Vec<u8>, &'static str), prometheus::Error> {
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut buf = Vec::with_capacity(8 * 1024);
        encoder.encode(&metric_families, &mut buf)?;
        Ok((buf, "text/plain; version=0.0.4; charset=utf-8"))
    }
}

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
        m.http_requests_total
            .with_label_values(&["GET", "/healthz", "200"])
            .inc();
        m.http_request_duration_seconds
            .with_label_values(&["GET", "/healthz"])
            .observe(0.01);
        m.verification_cases_total
            .with_label_values(&["green"])
            .inc();
        m.outbox_dlq_size.set(0);
        m.outbox_dlq_replays_total
            .with_label_values(&["success"])
            .inc();
        m.fusion_belief_true
            .with_label_values(&["green"])
            .observe(0.85);
        m.fusion_belief_false
            .with_label_values(&["green"])
            .observe(0.05);
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
            "recor_verification_cases_total",
            "recor_outbox_dlq_size",
            "recor_outbox_dlq_replays_total",
            "recor_fusion_belief_true",
            "recor_fusion_belief_false",
            "recor_oidc_jwks_fetch_latency_seconds",
            "recor_oidc_verify_total",
            "recor_health_check_duration_seconds",
        ] {
            assert!(text.contains(name), "missing metric `{name}`; got:\n{text}");
        }
    }

    #[test]
    fn content_type_is_prometheus_0_0_4() {
        let m = Metrics::new().expect("registry");
        let (_body, ct) = m.gather_text().expect("encodes");
        assert!(ct.starts_with("text/plain; version=0.0.4"), "{ct}");
    }
}
