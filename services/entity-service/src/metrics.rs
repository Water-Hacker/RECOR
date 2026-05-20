//! OBS-1 — Prometheus metrics for the Entity service.
//!
//! One [`Metrics`] handle is constructed at service startup and shared
//! via `Arc<_>` through the REST router, OIDC middleware, and use-case
//! handlers. Same shape as recor-declaration::metrics so dashboards
//! can re-use the panel definitions.
//!
//! D18: every label value is a bounded enum. UUIDs / principals /
//! free-form names MUST NOT appear as label values.

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::MatchedPath, http::Request, middleware::Next, response::Response};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,

    /// Successful entity registrations — label: `jurisdiction_class`
    /// ∈ {"cm","other"} (bounded; per-country cardinality is bounded by
    /// design, but we collapse non-CM to "other" here to keep the
    /// dashboard panel readable).
    pub entities_registered_total: IntCounterVec,
    pub entities_updated_total: IntCounterVec,
    pub entities_dissolved_total: IntCounterVec,

    pub oidc_jwks_fetch_latency_seconds: HistogramVec,
    pub oidc_verify_total: IntCounterVec,

    /// COMP-2 outbox retention prune counter. `result=success`
    /// increments BY rows-pruned per cycle; `result=error` increments
    /// BY 1 per failed cycle.
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

        let entities_registered_total = IntCounterVec::new(
            Opts::new(
                "recor_entities_registered_total",
                "Entities successfully registered (POST /v1/entities). Increment is AFTER persistence; rejects do not increment.",
            ),
            &["jurisdiction_class"],
        )?;
        registry.register(Box::new(entities_registered_total.clone()))?;

        let entities_updated_total = IntCounterVec::new(
            Opts::new(
                "recor_entities_updated_total",
                "Entities updated (POST /v1/entities/{id}/update). result ∈ {success,error}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(entities_updated_total.clone()))?;

        let entities_dissolved_total = IntCounterVec::new(
            Opts::new(
                "recor_entities_dissolved_total",
                "Entities dissolved (POST /v1/entities/{id}/dissolve). result ∈ {success,error}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(entities_dissolved_total.clone()))?;

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
                "OIDC token-verification outcomes. result=success|invalid|unavailable.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(oidc_verify_total.clone()))?;

        let outbox_retention_pruned_total = IntCounterVec::new(
            Opts::new(
                "recor_outbox_retention_pruned_total",
                "COMP-2 outbox retention prune counter. result=success increments BY rows-pruned per cycle; result=error increments BY 1 per failed cycle.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(outbox_retention_pruned_total.clone()))?;

        let health_check_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_health_check_duration_seconds",
                "Health probe latency (healthz + readyz). probe ∈ {healthz,readyz}.",
            )
            .buckets(HEALTH_CHECK_BUCKETS.to_vec()),
            &["probe"],
        )?;
        registry.register(Box::new(health_check_duration_seconds.clone()))?;

        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            entities_registered_total,
            entities_updated_total,
            entities_dissolved_total,
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
        m.http_requests_total
            .with_label_values(&["GET", "/healthz", "200"])
            .inc();
        m.entities_registered_total
            .with_label_values(&["cm"])
            .inc();
        m.entities_updated_total
            .with_label_values(&["success"])
            .inc();
        m.entities_dissolved_total
            .with_label_values(&["success"])
            .inc();
        m.oidc_verify_total
            .with_label_values(&["success"])
            .inc();
        m.oidc_jwks_fetch_latency_seconds
            .with_label_values(&["success"])
            .observe(0.05);
        m.outbox_retention_pruned_total
            .with_label_values(&["success"])
            .inc();
        m.health_check_duration_seconds
            .with_label_values(&["readyz"])
            .observe(0.002);

        let (body, _ct) = m.gather_text().expect("encodes");
        let text = String::from_utf8(body).expect("utf-8");
        for name in [
            "http_requests_total",
            "recor_entities_registered_total",
            "recor_entities_updated_total",
            "recor_entities_dissolved_total",
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
