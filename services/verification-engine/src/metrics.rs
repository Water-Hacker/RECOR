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

    /// COMP-2 — verification-outbox retention prune counter.
    /// `result` ∈ {success,error}; `success` increments BY rows-pruned.
    pub outbox_retention_pruned_total: IntCounterVec,

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

    // ─── R-VER-1 — BUNEC adapter ───────────────────────────────────────
    /// BUNEC lookups by outcome.
    /// `result` ∈ {found, not_found, circuit_open, retries_exhausted,
    /// non_retryable, mismatch}.
    pub bunec_calls_total: IntCounterVec,
    /// BUNEC call latency in seconds, by outcome.
    pub bunec_call_latency_seconds: HistogramVec,

    // ─── R-VER-2 — Sanctions (Stage 3) ─────────────────────────────────
    /// Sanctions screen outcomes. `result` ∈ {certain, near, none, error}.
    pub sanctions_screen_total: IntCounterVec,
    /// Sanctions screen latency in seconds.
    pub sanctions_screen_latency_seconds: HistogramVec,
    /// Number of rows in `sanctions_persons` (sampled at ingest).
    pub sanctions_index_rows: IntGauge,

    // ─── R-VER-3 — PEP (Stage 4) ───────────────────────────────────────
    /// PEP screen outcomes. `result` ∈ {confirmed, associate, none, error}.
    pub pep_screen_total: IntCounterVec,
    /// PEP screen latency in seconds.
    pub pep_screen_latency_seconds: HistogramVec,
    /// Number of rows in `peps` (sampled at ingest).
    pub pep_index_rows: IntGauge,

    // ─── R-VER-4 — Adverse media (Stage 5) ─────────────────────────────
    /// Adverse-media calls by outcome. `result` ∈ {match, none, error, fixture}.
    pub adverse_media_calls_total: IntCounterVec,
    /// Adverse-media latency in seconds.
    pub adverse_media_latency_seconds: HistogramVec,
    /// Anthropic Inference Gateway token usage. Labels: purpose, model.
    pub inference_tokens_used_total: IntCounterVec,

    // ─── R-VER-5 — Pattern detection (Stage 6) ─────────────────────────
    /// Pattern-detection signature firings. Label: `signature`.
    pub pattern_detection_total: IntCounterVec,
    /// Pattern-detection latency in seconds.
    pub pattern_detection_latency_seconds: HistogramVec,

    // ─── R-VER-6 — Triangulation (Stage 7) ─────────────────────────────
    /// Pairwise consistency counters across upstream stage BPAs.
    pub triangulation_pairs_consistent_total: IntCounterVec,
    pub triangulation_pairs_inconsistent_total: IntCounterVec,

    // ─── R-LOOP-2: Kafka consumer metrics ───────────────────────────
    /// Per-message outcome counter — label `result` ∈
    /// {"applied","skipped","dlq"}. Incremented once per polled Kafka
    /// message regardless of whether the offset committed (it always
    /// does, but the result captures the application outcome).
    pub kafka_consume_total: IntCounterVec,
    /// Wall-clock lag from broker-stamped timestamp to consume time,
    /// in seconds. Sampled per polled message so the gauge tracks
    /// near-realtime — but it's a *gauge* not a histogram because
    /// dashboards need an instantaneous value for alerting.
    pub kafka_consume_lag_seconds: prometheus::Gauge,

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

/// BUNEC + sanctions + PEP latency buckets — partner-call shaped.
const PARTNER_LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Inference-gateway latency buckets — model calls run longer.
const INFERENCE_LATENCY_BUCKETS: &[f64] = &[
    0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
];

/// Pattern detection per-signature buckets.
const PATTERN_LATENCY_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
];

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

        let outbox_retention_pruned_total = IntCounterVec::new(
            Opts::new(
                "recor_outbox_retention_pruned_total",
                "COMP-2 verification-outbox retention prune counter. result=success increments BY rows-pruned per cycle; result=error increments BY 1 per failed cycle.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(outbox_retention_pruned_total.clone()))?;

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

        // R-VER-1
        let bunec_calls_total = IntCounterVec::new(
            Opts::new(
                "recor_bunec_calls_total",
                "BUNEC adapter call outcomes. result ∈ {found,not_found,circuit_open,retries_exhausted,non_retryable,mismatch}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(bunec_calls_total.clone()))?;
        let bunec_call_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_bunec_call_latency_seconds",
                "BUNEC adapter call latency in seconds, by outcome.",
            )
            .buckets(PARTNER_LATENCY_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(bunec_call_latency_seconds.clone()))?;

        // R-VER-2
        let sanctions_screen_total = IntCounterVec::new(
            Opts::new(
                "recor_sanctions_screen_total",
                "Sanctions-screening outcomes. result ∈ {certain,near,none,error}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(sanctions_screen_total.clone()))?;
        let sanctions_screen_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_sanctions_screen_latency_seconds",
                "Sanctions-screening latency in seconds.",
            )
            .buckets(PARTNER_LATENCY_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(sanctions_screen_latency_seconds.clone()))?;
        let sanctions_index_rows = IntGauge::with_opts(Opts::new(
            "recor_sanctions_index_rows",
            "Number of rows in the `sanctions_persons` table (sampled at ingest).",
        ))?;
        registry.register(Box::new(sanctions_index_rows.clone()))?;

        // R-VER-3
        let pep_screen_total = IntCounterVec::new(
            Opts::new(
                "recor_pep_screen_total",
                "PEP-screening outcomes. result ∈ {confirmed,associate,none,error}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(pep_screen_total.clone()))?;
        let pep_screen_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_pep_screen_latency_seconds",
                "PEP-screening latency in seconds.",
            )
            .buckets(PARTNER_LATENCY_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(pep_screen_latency_seconds.clone()))?;
        let pep_index_rows = IntGauge::with_opts(Opts::new(
            "recor_pep_index_rows",
            "Number of rows in the `peps` table (sampled at ingest).",
        ))?;
        registry.register(Box::new(pep_index_rows.clone()))?;

        // R-VER-4
        let adverse_media_calls_total = IntCounterVec::new(
            Opts::new(
                "recor_adverse_media_calls_total",
                "Adverse-media call outcomes. result ∈ {match,none,error,fixture}.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(adverse_media_calls_total.clone()))?;
        let adverse_media_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_adverse_media_latency_seconds",
                "Adverse-media end-to-end latency in seconds.",
            )
            .buckets(INFERENCE_LATENCY_BUCKETS.to_vec()),
            &["result"],
        )?;
        registry.register(Box::new(adverse_media_latency_seconds.clone()))?;
        let inference_tokens_used_total = IntCounterVec::new(
            Opts::new(
                "recor_inference_tokens_used_total",
                "Anthropic Inference Gateway tokens consumed. Labels: purpose, model.",
            ),
            &["purpose", "model"],
        )?;
        registry.register(Box::new(inference_tokens_used_total.clone()))?;

        // R-VER-5
        let pattern_detection_total = IntCounterVec::new(
            Opts::new(
                "recor_pattern_detection_total",
                "Pattern-detection signature firings. Label: signature.",
            ),
            &["signature", "outcome"],
        )?;
        registry.register(Box::new(pattern_detection_total.clone()))?;
        let pattern_detection_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "recor_pattern_detection_latency_seconds",
                "Pattern-detection per-signature latency in seconds.",
            )
            .buckets(PATTERN_LATENCY_BUCKETS.to_vec()),
            &["signature"],
        )?;
        registry.register(Box::new(pattern_detection_latency_seconds.clone()))?;

        // R-VER-6
        let triangulation_pairs_consistent_total = IntCounterVec::new(
            Opts::new(
                "recor_triangulation_pairs_consistent_total",
                "Cross-source pairwise consistency: agreements. Label: pair.",
            ),
            &["pair"],
        )?;
        registry.register(Box::new(triangulation_pairs_consistent_total.clone()))?;
        let triangulation_pairs_inconsistent_total = IntCounterVec::new(
            Opts::new(
                "recor_triangulation_pairs_inconsistent_total",
                "Cross-source pairwise consistency: disagreements. Label: pair.",
            ),
            &["pair"],
        )?;
        registry.register(Box::new(triangulation_pairs_inconsistent_total.clone()))?;

        // R-LOOP-2 Kafka consumer metrics.
        let kafka_consume_total = IntCounterVec::new(
            Opts::new(
                "recor_kafka_consume_total",
                "Kafka consume outcomes from the declaration-events topic. result=applied: use case succeeded; result=skipped: non-declaration event (no-op); result=dlq: message dead-lettered (parse or use-case error).",
            ),
            &["result"],
        )?;
        registry.register(Box::new(kafka_consume_total.clone()))?;

        let kafka_consume_lag_seconds = prometheus::Gauge::with_opts(Opts::new(
            "recor_kafka_consume_lag_seconds",
            "Wall-clock lag in seconds between Kafka broker-stamped timestamp and consume time. Sampled per polled message.",
        ))?;
        registry.register(Box::new(kafka_consume_lag_seconds.clone()))?;


        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            verification_cases_total,
            outbox_dlq_size,
            outbox_dlq_replays_total,
            outbox_retention_pruned_total,
            fusion_belief_true,
            fusion_belief_false,
            oidc_jwks_fetch_latency_seconds,
            oidc_verify_total,
            health_check_duration_seconds,
            bunec_calls_total,
            bunec_call_latency_seconds,
            sanctions_screen_total,
            sanctions_screen_latency_seconds,
            sanctions_index_rows,
            pep_screen_total,
            pep_screen_latency_seconds,
            pep_index_rows,
            adverse_media_calls_total,
            adverse_media_latency_seconds,
            inference_tokens_used_total,
            pattern_detection_total,
            pattern_detection_latency_seconds,
            triangulation_pairs_consistent_total,
            triangulation_pairs_inconsistent_total,

            kafka_consume_total,
            kafka_consume_lag_seconds,

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
        m.outbox_retention_pruned_total
            .with_label_values(&["success"])
            .inc_by(3);
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
        m.bunec_calls_total.with_label_values(&["found"]).inc();
        m.bunec_call_latency_seconds
            .with_label_values(&["found"])
            .observe(0.05);
        m.sanctions_screen_total.with_label_values(&["none"]).inc();
        m.sanctions_screen_latency_seconds
            .with_label_values(&["none"])
            .observe(0.005);
        m.sanctions_index_rows.set(0);
        m.pep_screen_total.with_label_values(&["none"]).inc();
        m.pep_screen_latency_seconds
            .with_label_values(&["none"])
            .observe(0.005);
        m.pep_index_rows.set(0);
        m.adverse_media_calls_total
            .with_label_values(&["fixture"])
            .inc();
        m.adverse_media_latency_seconds
            .with_label_values(&["fixture"])
            .observe(0.01);
        m.inference_tokens_used_total
            .with_label_values(&["adverse_media", "claude-haiku-4-5-20251001"])
            .inc_by(0);
        m.pattern_detection_total
            .with_label_values(&["circular_ownership", "fired"])
            .inc();
        m.pattern_detection_latency_seconds
            .with_label_values(&["circular_ownership"])
            .observe(0.001);
        m.triangulation_pairs_consistent_total
            .with_label_values(&["identity__sanctions"])
            .inc();
        m.triangulation_pairs_inconsistent_total
            .with_label_values(&["identity__sanctions"])
            .inc_by(0);

        // R-LOOP-2 metrics.
        m.kafka_consume_total
            .with_label_values(&["applied"])
            .inc();
        m.kafka_consume_lag_seconds.set(0.42);


        let (body, _ct) = m.gather_text().expect("encodes");
        let text = String::from_utf8(body).expect("utf-8");
        for name in [
            "http_requests_total",
            "http_request_duration_seconds",
            "recor_verification_cases_total",
            "recor_outbox_dlq_size",
            "recor_outbox_dlq_replays_total",
            "recor_outbox_retention_pruned_total",
            "recor_fusion_belief_true",
            "recor_fusion_belief_false",
            "recor_oidc_jwks_fetch_latency_seconds",
            "recor_oidc_verify_total",
            "recor_health_check_duration_seconds",
            "recor_bunec_calls_total",
            "recor_bunec_call_latency_seconds",
            "recor_sanctions_screen_total",
            "recor_sanctions_screen_latency_seconds",
            "recor_sanctions_index_rows",
            "recor_pep_screen_total",
            "recor_pep_screen_latency_seconds",
            "recor_pep_index_rows",
            "recor_adverse_media_calls_total",
            "recor_adverse_media_latency_seconds",
            "recor_inference_tokens_used_total",
            "recor_pattern_detection_total",
            "recor_pattern_detection_latency_seconds",
            "recor_triangulation_pairs_consistent_total",
            "recor_triangulation_pairs_inconsistent_total",

            "recor_kafka_consume_total",
            "recor_kafka_consume_lag_seconds",

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
