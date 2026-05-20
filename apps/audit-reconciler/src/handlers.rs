//! Operational HTTP surface: `/healthz`, `/readyz`, `/metrics`.
//!
//! The reconciler does NOT expose business endpoints — it's a cron.
//! Probes + scrape are sufficient for k8s liveness/readiness and
//! Prometheus.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tracing::warn;

use crate::metrics::ReconcilerMetrics;

#[derive(Clone)]
pub struct AppState {
    pub metrics: Arc<ReconcilerMetrics>,
    /// Used by `/readyz` — the reconciler is "ready" when it can talk
    /// to Postgres. The cron loop itself runs out-of-band.
    pub pool: sqlx::PgPool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let probe: Result<i32, sqlx::Error> =
        sqlx::query_scalar("SELECT 1").fetch_one(&state.pool).await;
    match probe {
        Ok(_) => (StatusCode::OK, "ready").into_response(),
        Err(e) => {
            warn!(error = %e, "readiness probe failed");
            (StatusCode::SERVICE_UNAVAILABLE, "not_ready").into_response()
        }
    }
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics.encode_text() {
        Ok(body) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            body,
        )
            .into_response(),
        Err(e) => {
            warn!(error = %e, "metrics encode failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "metrics encode failed",
            )
                .into_response()
        }
    }
}
