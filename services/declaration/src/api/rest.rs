//! REST route definitions. Axum router + handlers.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use blake3::Hasher;
use serde_json::json;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::auth::{auth_middleware, AuthConfig, Principal};
use crate::api::dto::{
    GetDeclarationResponse, SubmitDeclarationRequest, SubmitDeclarationResponse,
    SupersedeDeclarationResponse,
};
use crate::api::internal::{handle_verification_outcome, InternalAppState};
use crate::api::OidcVerifier;
use crate::application::{
    GetDeclarationUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
    SupersedeDeclarationUseCase,
};
use crate::config::Config;
use crate::domain::DeclarationId;
use crate::error::ServiceError;
use crate::infrastructure::postgres::IdempotencyStore;

#[derive(Clone)]
pub struct AppState {
    pub submit_usecase: Arc<SubmitDeclarationUseCase>,
    pub get_usecase: Arc<GetDeclarationUseCase>,
    pub record_verification_usecase: Arc<RecordVerificationOutcomeUseCase>,
    pub supersede_usecase: Arc<SupersedeDeclarationUseCase>,
    pub idempotency: Arc<IdempotencyStore>,
    pub base_url: String,
    pub is_dev: bool,
    pub idempotency_ttl_seconds: i64,
    /// OIDC verifier. `None` is only acceptable in dev environments
    /// (the config layer refuses to start otherwise). Bearer-token
    /// requests with `oidc = None` are rejected at the middleware.
    pub oidc: Option<Arc<OidcVerifier>>,
}

pub fn router(state: AppState, cfg: &Config) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
    };

    let protected = Router::new()
        .route("/v1/declarations", post(submit_declaration))
        .route("/v1/declarations/{declaration_id}", get(get_declaration))
        .route(
            "/v1/declarations/{declaration_id}/supersede",
            post(supersede_declaration),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(state.clone());

    // Internal HMAC-authenticated webhook for the Verification Engine's
    // writeback relay. Not behind the user-auth middleware — uses its
    // own signature verification at the handler.
    use secrecy::ExposeSecret;
    let internal_state = InternalAppState {
        record_verification_usecase: state.record_verification_usecase.clone(),
        hmac_secret: cfg.writeback_hmac_secret.expose_secret().to_string(),
        old_hmac_secret: cfg.writeback_hmac_secret_old.expose_secret().to_string(),
    };
    let internal = Router::new()
        .route(
            "/v1/internal/verification-outcomes",
            post(handle_verification_outcome),
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

#[tracing::instrument(level = "info")]
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

#[tracing::instrument(level = "info", skip(state))]
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    // Cheap readiness: confirms the idempotency-store pool is alive,
    // which by transitivity means the database is reachable.
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(state.idempotency.pool());
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

#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        entity_id = %req.entity_id,
        idempotency_key = idempotency_key_field(&headers),
    )
)]
async fn submit_declaration(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    headers: HeaderMap,
    Json(req): Json<SubmitDeclarationRequest>,
) -> Result<(StatusCode, Json<SubmitDeclarationResponse>), ServiceError> {
    // 1. Verify the attestation signature against the canonical bytes.
    let canonical_bytes = canonical_payload_bytes(&req, &principal.subject)?;
    req.attestation
        .verify_against(&canonical_bytes)
        .map_err(|e| ServiceError::AttestationVerificationFailed(e.to_string()))?;

    let correlation_id = Uuid::now_v7();

    // 2. Compute idempotency hash over the canonical request shape.
    let request_hash = blake3_hex(&canonical_bytes);

    // 3. Build the command BEFORE consulting idempotency so we have a
    // stable declaration_id for the receipt body.
    let cmd = req.into_command(principal.subject.clone(), correlation_id);
    let declaration_id = cmd.declaration_id;

    // 4. Idempotency check.
    let idem_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if let Some(key) = idem_key.as_deref() {
        match state
            .idempotency
            .check_existing(key, &principal.subject)
            .await
        {
            Ok(Some(existing)) => {
                if existing.request_hash != request_hash {
                    return Err(ServiceError::IdempotencyConflict);
                }
                info!(idempotency_key = %key, "idempotency replay");
                let stored: SubmitDeclarationResponse =
                    serde_json::from_value(existing.response_body)
                        .map_err(|_| ServiceError::Internal)?;
                let status = StatusCode::from_u16(
                    u16::try_from(existing.response_status).unwrap_or(200),
                )
                .unwrap_or(StatusCode::OK);
                return Ok((status, Json(stored)));
            }
            Ok(None) => {} // first-seen; proceed to submit
            Err(e) => {
                warn!(error = ?e, "idempotency lookup failed; proceeding without replay");
            }
        }
    }

    // 5. Execute the use case.
    let receipt = state.submit_usecase.execute(cmd).await?;
    let response = SubmitDeclarationResponse::from_receipt(receipt, &state.base_url);

    // 6. Record idempotency for next time.
    if let Some(key) = idem_key {
        let body_value = serde_json::to_value(&response).map_err(|_| ServiceError::Internal)?;
        let recorded = state
            .idempotency
            .record(
                &key,
                &principal.subject,
                &request_hash,
                201,
                &body_value,
                state.idempotency_ttl_seconds,
            )
            .await;
        if let Err(e) = recorded {
            warn!(error = ?e, "idempotency record failed; submission succeeded but replay disabled");
        }
    }

    info!(
        declaration_id = %declaration_id,
        receipt_hash = %response.receipt_hash_hex,
        "declaration submitted"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        declaration_id = %declaration_id,
    )
)]
async fn get_declaration(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(declaration_id): Path<Uuid>,
) -> Result<Json<GetDeclarationResponse>, ServiceError> {
    let projection = state
        .get_usecase
        .execute(DeclarationId(declaration_id))
        .await?;

    // Authorisation: declarants see their own. Cross-principal visibility
    // is the job of the (future) Access service; for v1 we enforce
    // owner-only.
    if projection.declarant_principal != principal.subject {
        return Err(ServiceError::AuthorizationDenied(
            "declaration is owned by a different principal",
        ));
    }

    Ok(Json(projection.into()))
}

fn idempotency_key_field(headers: &HeaderMap) -> String {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn canonical_payload_bytes(
    req: &SubmitDeclarationRequest,
    principal: &str,
) -> Result<Vec<u8>, ServiceError> {
    use serde::Serialize;
    // Field order is the canonical order; the iso_date attribute aligns
    // the date encoding with the wire format the declarant signs.
    // Field names and serialised representation MUST match what the
    // declarant signs — anything else is a signature mismatch.
    #[derive(Serialize)]
    struct Canonical<'a> {
        entity_id: &'a crate::domain::EntityId,
        declarant_principal: &'a str,
        declarant_role: &'static str,
        kind: &'static str,
        #[serde(with = "crate::domain::serde_helpers::iso_date")]
        effective_from: time::Date,
        beneficial_owners: &'a [crate::domain::BeneficialOwnerClaim],
        nonce_hex: &'a str,
    }
    let canonical = Canonical {
        entity_id: &req.entity_id,
        declarant_principal: principal,
        declarant_role: req.declarant_role.as_str(),
        kind: req.kind.as_str(),
        effective_from: req.effective_from,
        beneficial_owners: &req.beneficial_owners,
        nonce_hex: &req.attestation.nonce_hex,
    };
    serde_json::to_vec(&canonical)
        .map_err(|_| ServiceError::BadRequest("could not canonicalise request".into()))
}

fn blake3_hex(bytes: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(bytes);
    hex::encode(h.finalize().as_bytes())
}

#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        superseded_declaration_id = %superseded_declaration_id,
        new_entity_id = %req.entity_id,
    )
)]
async fn supersede_declaration(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(superseded_declaration_id): Path<Uuid>,
    Json(req): Json<SubmitDeclarationRequest>,
) -> Result<(StatusCode, Json<SupersedeDeclarationResponse>), ServiceError> {
    // Same canonicalisation + attestation verification as submit — the
    // NEW declaration is a fully-signed declaration in its own right.
    let canonical_bytes = canonical_payload_bytes(&req, &principal.subject)?;
    req.attestation
        .verify_against(&canonical_bytes)
        .map_err(|e| ServiceError::AttestationVerificationFailed(e.to_string()))?;

    let correlation_id = Uuid::now_v7();
    let new_command = req.into_command(principal.subject.clone(), correlation_id);

    let receipt = state
        .supersede_usecase
        .execute(DeclarationId(superseded_declaration_id), new_command)
        .await?;

    let response = SupersedeDeclarationResponse::from_receipt(receipt, &state.base_url);
    info!(
        new_declaration_id = %response.new_declaration_id,
        superseded_declaration_id = %response.superseded_declaration_id,
        "declaration superseded"
    );
    Ok((StatusCode::CREATED, Json(response)))
}
