//! REST API.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::warn;
use uuid::Uuid;

use crate::api::auth::{auth_middleware, AuthConfig};
use crate::api::oidc::OidcVerifier;
use crate::application::{GetVerificationUseCase, SubmitVerificationUseCase};
use crate::config::Config;
use crate::domain::{DeclarationSnapshot, VerificationCase, VerificationCaseId};
use crate::error::ServiceError;
use crate::infrastructure::PostgresVerificationRepository;

#[derive(Clone)]
pub struct AppState {
    pub submit_usecase: Arc<SubmitVerificationUseCase>,
    pub get_usecase: Arc<GetVerificationUseCase>,
    pub repository: Arc<PostgresVerificationRepository>,
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
}

pub fn router(state: AppState, cfg: &Config) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
    };

    let protected = Router::new()
        .route("/v1/verifications", post(submit_verification))
        .route("/v1/verifications/{case_id}", get(get_verification))
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(state.clone());

    // Internal HMAC-authenticated webhook for the Declaration service's
    // outbox relay. Not behind the user-auth middleware — uses its own
    // signature verification at the handler.
    use secrecy::ExposeSecret;
    let internal_state = crate::api::internal::InternalAppState {
        submit_usecase: state.submit_usecase.clone(),
        hmac_secret: cfg.inbound_hmac_secret.expose_secret().to_string(),
        old_hmac_secret: cfg.inbound_hmac_secret_old.expose_secret().to_string(),
    };
    let internal = Router::new()
        .route(
            "/v1/internal/declaration-events",
            post(crate::api::internal::handle_declaration_event),
        )
        .with_state(internal_state);

    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state);

    protected.merge(internal).merge(public).layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(
                http::HeaderName::from_static("x-request-id"),
                MakeRequestUuid,
            ))
            .layer(PropagateRequestIdLayer::new(
                http::HeaderName::from_static("x-request-id"),
            ))
            .layer(TraceLayer::new_for_http())
            .layer(TimeoutLayer::new(Duration::from_secs(cfg.http_timeout_seconds))),
    )
}

#[tracing::instrument]
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

#[tracing::instrument(skip(state))]
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(state.repository.pool());
    match probe.await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))),
        Err(e) => {
            warn!(error = %e, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "not_ready", "reason": "database_unreachable"})),
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitVerificationRequest {
    pub declaration: DeclarationSnapshot,
}

#[derive(Debug, Serialize)]
pub struct SubmitVerificationResponse {
    pub case_id: VerificationCaseId,
    pub lane: String,
    pub authenticity_belief: f64,
    pub authenticity_plausibility: f64,
    pub risk_belief: f64,
    pub total_duration_ms: u64,
    pub case_url: String,
}

impl SubmitVerificationResponse {
    fn from_case(case: &VerificationCase, base_url: &str) -> Self {
        Self {
            case_id: case.case_id,
            lane: case.lane.as_str().to_string(),
            authenticity_belief: case.fused_authenticity.belief_true(),
            authenticity_plausibility: case.fused_authenticity.plausibility_true(),
            risk_belief: case.fused_risk.belief_true(),
            total_duration_ms: case.total_duration_ms,
            case_url: format!("{base_url}/v1/verifications/{}", case.case_id),
        }
    }
}

#[tracing::instrument(skip_all)]
async fn submit_verification(
    State(state): State<AppState>,
    axum::Extension(_principal): axum::Extension<crate::api::auth::Principal>,
    Json(req): Json<SubmitVerificationRequest>,
) -> Result<(StatusCode, Json<SubmitVerificationResponse>), ServiceError> {
    let case = state.submit_usecase.execute(req.declaration).await?;
    let base_url = std::env::var("RECOR_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string());
    let resp = SubmitVerificationResponse::from_case(&case, &base_url);
    Ok((StatusCode::CREATED, Json(resp)))
}

#[tracing::instrument(skip(state))]
async fn get_verification(
    State(state): State<AppState>,
    axum::Extension(_principal): axum::Extension<crate::api::auth::Principal>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<VerificationCase>, ServiceError> {
    let case = state.get_usecase.execute(VerificationCaseId(case_id)).await?;
    Ok(Json(case))
}
