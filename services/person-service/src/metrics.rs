//! OBS-1 — Prometheus metrics for the Person service.
//!
//! Mirrors `services/declaration/src/metrics.rs` in shape (one
//! [`Metrics`] handle constructed at startup, shared via `Arc<_>`).
//! Label cardinality is bounded by construction — every label value is
//! a `&'static` enum string, never a UUID / principal / email (D18).

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::MatchedPath, http::Request, middleware::Next, response::Response,
};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,

    /// Domain counter: successful person registrations.
    pub persons_registered_total: IntCounterVec,
    /// Domain counter: successful merges.
    pub persons_merged_total: IntCounterVec,
    /// Domain counter: search invocations — label `nationality_filter`
    /// ∈ {"yes","no"} only; the actual nationality is NEVER a label.
    pub persons_search_total: IntCounterVec,

    /// OIDC verifier metrics (shared shape with declaration).
    pub oidc_jwks_fetch_latency_seconds: HistogramVec,
    pub oidc_verify_total: IntCounterVec,

    /// COMP-2 outbox retention prune counter. `result=success`
    /// increments BY rows-pruned per cycle; `result=error` increments
    /// BY 1 per failed cycle. Mirrors the declaration-service surface
    /// so a single Prometheus alert rule covers every event-sourced
    /// service in the platform.
    pub outbox_retention_pruned_total: IntCounterVec,

    pub health_check_duration_seconds: HistogramVec,
}

const HTTP_LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];
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
                "Total HTTP requests handled, labelled by method, matched path template, and status code.",
            ),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request latency in seconds, labelled by method + matched path template.",
            )
            .buckets(HTTP_LATENCY_BUCKETS.to_vec()),
            &["method", "path"],
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        let persons_registered_total = IntCounterVec::new(
            Opts::new(
                "recor_persons_registered_total",
                "Persons successfully registered via POST /v1/persons. Increment is AFTER the use case persists.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(persons_registered_total.clone()))?;

        let persons_merged_total = IntCounterVec::new(
            Opts::new(
                "recor_persons_merged_total",
                "Persons merged via POST /v1/persons/{id}/merge-into/{target_id}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(persons_merged_total.clone()))?;

        let persons_search_total = IntCounterVec::new(
            Opts::new(
                "recor_persons_search_total",
                "Search invocations. Label `nationality_filter` is `yes` when a filter was supplied, `no` otherwise.",
            ),
            &["nationality_filter"],
        )?;
        registry.register(Box::new(persons_search_total.clone()))?;

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
                "OIDC JWKS endpoint fetch latency in seconds.",
            )
            .buckets(JWKS_FETCH_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(oidc_jwks_fetch_latency_seconds.clone()))?;

        let oidc_verify_total = IntCounterVec::new(
            Opts::new(
                "recor_oidc_verify_total",
                "OIDC token-verification outcomes.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(oidc_verify_total.clone()))?;

        let health_check_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_health_check_duration_seconds",
                "Health probe latency.",
            )
            .buckets(HEALTH_CHECK_BUCKETS.to_vec()),
            &["probe"],
        )?;
        registry.register(Box::new(health_check_duration_seconds.clone()))?;

        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            persons_registered_total,
            persons_merged_total,
            persons_search_total,
            oidc_jwks_fetch_latency_seconds,
            oidc_verify_total,
            outbox_retention_pruned_total,
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
        // Touch every metric family so it appears in the Prometheus
        // text exposition (families with no observed labels are
        // omitted from `gather()`).
        m.http_requests_total
            .with_label_values(&["GET", "/healthz", "200"])
            .inc();
        m.http_request_duration_seconds
            .with_label_values(&["GET", "/healthz"])
            .observe(0.001);
        m.persons_registered_total
            .with_label_values(&["success"])
            .inc();
        m.persons_merged_total.with_label_values(&["success"]).inc();
        m.persons_search_total.with_label_values(&["yes"]).inc();
        m.oidc_verify_total.with_label_values(&["success"]).inc();
        m.outbox_retention_pruned_total
            .with_label_values(&["success"])
            .inc();
        m.health_check_duration_seconds
            .with_label_values(&["readyz"])
            .observe(0.002);
        let (body, ct) = m.gather_text().expect("encodes");
        let text = String::from_utf8(body).expect("utf-8");
        assert!(ct.starts_with("text/plain; version=0.0.4"));
        for name in [
            "http_requests_total",
            "http_request_duration_seconds",
            "recor_persons_registered_total",
            "recor_persons_merged_total",
            "recor_persons_search_total",
            "recor_oidc_verify_total",
            "recor_outbox_retention_pruned_total",
            "recor_health_check_duration_seconds",
        ] {
            assert!(
                text.contains(name),
                "exposition missing metric `{name}`; got:\n{text}"
            );
        }
    }
}
