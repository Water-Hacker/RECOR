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
    cors::{AllowMethods, AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::auth::{auth_middleware, AuthConfig, Principal};
use crate::api::dlq::DlqAdminState;
use crate::api::dto::{
    AmendDeclarationRequest, AmendDeclarationResponse, CorrectDeclarationRequest,
    CorrectDeclarationResponse, DeclarationsByPrincipalResponse, GetDeclarationResponse,
    SubmitDeclarationRequest, SubmitDeclarationResponse, SupersedeDeclarationResponse,
};
use crate::api::internal::{handle_verification_outcome, InternalAppState};
use crate::api::rate_limit::{build_governor_config, governor_layer};
use crate::api::OidcVerifier;
use crate::application::{
    AmendDeclarationUseCase, CorrectDeclarationUseCase, GetDeclarationUseCase,
    ListByPrincipalUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
    SupersedeDeclarationUseCase,
};
use crate::config::Config;
use crate::domain::DeclarationId;
use crate::error::ServiceError;
use crate::infrastructure::postgres::IdempotencyStore;
use crate::infrastructure::OutboxAdminStore;
// OBS-1: Prometheus metrics.
use crate::metrics::{metrics_handler, metrics_middleware, Metrics};

#[derive(Clone)]
pub struct AppState {
    pub submit_usecase: Arc<SubmitDeclarationUseCase>,
    pub get_usecase: Arc<GetDeclarationUseCase>,
    pub record_verification_usecase: Arc<RecordVerificationOutcomeUseCase>,
    pub supersede_usecase: Arc<SupersedeDeclarationUseCase>,
    pub amend_usecase: Arc<AmendDeclarationUseCase>,
    pub correct_usecase: Arc<CorrectDeclarationUseCase>,
    /// COMP-1: data-subject access. Returns every declaration where
    /// the authenticated principal is the declarant.
    pub list_by_principal_usecase: Arc<ListByPrincipalUseCase>,
    pub idempotency: Arc<IdempotencyStore>,
    pub outbox_admin: Arc<OutboxAdminStore>,
    pub base_url: String,
    pub is_dev: bool,
    pub idempotency_ttl_seconds: i64,
    /// OIDC verifier. `None` is only acceptable in dev environments
    /// (the config layer refuses to start otherwise). Bearer-token
    /// requests with `oidc = None` are rejected at the middleware.
    pub oidc: Option<Arc<OidcVerifier>>,
    /// OBS-1: shared Prometheus metrics handle. Hydrated at startup
    /// in `main.rs`; cloned cheaply via `Arc<_>` into every handler
    /// + middleware site.
    pub metrics: Arc<Metrics>,
}

/// Build the main router for the declaration service.
///
/// `expose_metrics_on_main`:
///   - `true` (current default): `/metrics` is mounted on the main
///     listener alongside the business routes. Backwards-compatible.
///   - `false` (FIND-007): `/metrics` is omitted; `main.rs` is expected
///     to bind a separate listener via `metrics_only_router`.
pub fn router(state: AppState, cfg: &Config, expose_metrics_on_main: bool) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
        metrics: state.metrics.clone(),
    };

    // Rate limiting (OPS-1). Built once at router construction and
    // applied ONLY to the two state-changing submit POSTs. GET
    // endpoints, /healthz, /readyz, and /v1/internal/* are deliberately
    // exempt — see api::rate_limit module docs. `None` here means
    // rate limiting is disabled (RATE_LIMIT_PER_MIN=0); the safe
    // default for tests and local dev.
    let governor_config = build_governor_config(cfg.rate_limit_per_min, cfg.rate_limit_burst);

    // Build the submit MethodRouters once; if rate limiting is enabled,
    // wrap each with the governor layer at the route level. Applying
    // `route_layer(governor)` here (vs at the Router level) ensures
    // the limiter is scoped to just these two POST methods — the GET
    // on /v1/declarations/{declaration_id} stays exempt so the portal
    // can poll verification status every ~3s without self-DoSing.
    let submit_route = if let Some(cfg) = governor_config.clone() {
        post(submit_declaration).route_layer(governor_layer(cfg))
    } else {
        post(submit_declaration)
    };
    let supersede_route = if let Some(cfg) = governor_config.clone() {
        post(supersede_declaration).route_layer(governor_layer(cfg))
    } else {
        post(supersede_declaration)
    };
    let amend_route = if let Some(cfg) = governor_config.clone() {
        post(amend_declaration).route_layer(governor_layer(cfg))
    } else {
        post(amend_declaration)
    };
    let correct_route = if let Some(cfg) = governor_config {
        post(correct_declaration).route_layer(governor_layer(cfg))
    } else {
        post(correct_declaration)
    };

    let protected = Router::new()
        .route("/v1/declarations", submit_route)
        // COMP-1: data-subject access. Static path is matched BEFORE
        // the parameterised `/{declaration_id}` route so axum doesn't
        // try to parse `by-principal` as a UUID. Read-only, principal
        // sourced from auth (D17), exempt from the submit-path rate
        // limit but subject to the standard auth gate.
        .route(
            "/v1/declarations/by-principal",
            get(list_declarations_by_principal),
        )
        .route("/v1/declarations/{declaration_id}", get(get_declaration))
        .route(
            "/v1/declarations/{declaration_id}/supersede",
            supersede_route,
        )
        .route(
            "/v1/declarations/{declaration_id}/amend",
            amend_route,
        )
        .route(
            "/v1/declarations/{declaration_id}/correct",
            correct_route,
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Admin endpoints (R-LOOP-DLQ-2). Same user-auth middleware as
    // the protected routes, but the handlers gate themselves on
    // `Config::admin_principals_list()` so only the listed subjects
    // can list/replay DLQ rows. Empty list ⇒ endpoints return 503.
    use std::collections::HashSet;
    let admin_principals: HashSet<String> =
        cfg.admin_principals_list().into_iter().collect();
    let dlq_admin_state = DlqAdminState {
        store: state.outbox_admin.clone(),
        admin_principals: Arc::new(admin_principals),
        metrics: state.metrics.clone(),
    };
    let admin = Router::new()
        .route("/v1/internal/outbox-dlq", get(crate::api::dlq::list_dlq))
        .route(
            "/v1/internal/outbox-dlq/{id}/replay",
            post(crate::api::dlq::replay_dlq),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(dlq_admin_state);

    // Internal HMAC-authenticated webhook for the Verification Engine's
    // writeback relay. Not behind the user-auth middleware — uses its
    // own signature verification at the handler.
    use secrecy::ExposeSecret;
    let internal_state = InternalAppState {
        record_verification_usecase: state.record_verification_usecase.clone(),
        hmac_secret: cfg.writeback_hmac_secret.expose_secret().to_string(),
        old_hmac_secret: cfg.writeback_hmac_secret_old.expose_secret().to_string(),
        // R-LOOP-3: HMAC stays required unless AUTH_TRANSPORT=mtls-only.
        // The peer-SPIFFE-ID allowlist is only consulted when the
        // service is actually running with mTLS termination (the
        // composition root in main.rs gates that with an outer
        // tower layer; this state field is consumed inside the
        // handler for logging + future per-route enforcement).
        hmac_required: cfg.hmac_required(),
        expected_peer_spiffe_id: if cfg.mtls_enabled() {
            cfg.spiffe_id_peer.clone()
        } else {
            String::new()
        },
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
        .with_state(state.clone());

    // DOC-1: the OpenAPI spec + Scalar UI. Public (no auth) — the spec
    // is a contract for consumers. Mounted as a sibling router so it
    // doesn't pick up the bearer-auth middleware. D17 still holds:
    // these routes do not change state; they describe the surface.
    let openapi = crate::api::openapi::openapi_routes();

    let app_routes = protected
        .merge(admin)
        .merge(internal)
        .merge(public)
        .merge(openapi);

    // Apply the request-timing middleware ONLY to the app routes — not
    // to /metrics. The middleware reads `State<Arc<Metrics>>`, so we use
    // `from_fn_with_state` to bind that state alongside the existing
    // typed AppState routes.
    let metrics_state = state.metrics.clone();
    let app_routes = app_routes.layer(axum::middleware::from_fn_with_state(
        metrics_state,
        metrics_middleware,
    ));

    // CORS for the declarant portal (different origin in dev/CI:
    // localhost:8082 → localhost:8080). Production deployments
    // typically front the API through the portal's nginx and skip
    // CORS, but the portal-e2e/live CI rig hits both origins
    // directly so the browser blocks XHR without these headers.
    // Origin allowlist is sourced from `CORS_ALLOWED_ORIGINS` (CSV);
    // empty disables CORS entirely (production default).
    let cors_layer = cors_layer_from_csv(&cfg.cors_allowed_origins);

    // FIND-007: /metrics is conditionally mounted. See router doc-comment.
    // Mounted as its own sub-router so the metrics handler reads
    // `State<Arc<Metrics>>` rather than the full AppState; the
    // request-timing middleware does NOT apply so scrape traffic
    // doesn't inflate the latency histogram.
    let with_metrics: Router = if expose_metrics_on_main {
        let metrics_router: Router = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(state.metrics.clone());
        app_routes.merge(metrics_router)
    } else {
        app_routes
    };

    with_metrics.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(
                http::HeaderName::from_static("x-request-id"),
                MakeRequestUuid,
            ))
            .layer(PropagateRequestIdLayer::new(
                http::HeaderName::from_static("x-request-id"),
            ))
            .layer(TraceLayer::new_for_http())
            .layer(cors_layer)
            .layer(TimeoutLayer::new(Duration::from_secs(cfg.http_timeout_seconds))),
    )
}

/// FIND-007: minimal router serving ONLY `/metrics`. Bound on a
/// separate listener by `main.rs` when `METRICS_BIND_ADDR` is set.
pub fn metrics_only_router(metrics: Arc<Metrics>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics)
}

fn cors_layer_from_csv(allowed: &str) -> CorsLayer {
    let origins: Vec<_> = allowed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<http::HeaderValue>().ok())
        .collect();
    if origins.is_empty() {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(AllowMethods::list([
            http::Method::GET,
            http::Method::POST,
            http::Method::OPTIONS,
        ]))
        .allow_headers([
            http::header::AUTHORIZATION,
            http::header::CONTENT_TYPE,
            http::HeaderName::from_static("idempotency-key"),
            http::HeaderName::from_static("x-recor-dev-principal"),
        ])
        .max_age(std::time::Duration::from_secs(300))
}

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    operation_id = "healthz",
    responses(
        (status = 200, description = "Service process is alive", body = crate::api::dto::HealthzResponse),
    ),
)]
#[tracing::instrument(level = "info", skip(state))]
pub(crate) async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let resp = (StatusCode::OK, Json(json!({"status": "ok"})));
    state
        .metrics
        .health_check_duration_seconds
        .with_label_values(&["healthz"])
        .observe(start.elapsed().as_secs_f64());
    resp
}

#[utoipa::path(
    get,
    path = "/readyz",
    tag = "system",
    operation_id = "readyz",
    responses(
        (status = 200, description = "Service is ready to serve traffic", body = crate::api::dto::ReadyzResponse),
        (status = 503, description = "Dependency unreachable (typically the database)", body = crate::api::dto::ReadyzResponse),
    ),
)]
#[tracing::instrument(level = "info", skip(state))]
pub(crate) async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    // Cheap readiness: confirms the idempotency-store pool is alive,
    // which by transitivity means the database is reachable.
    let probe = sqlx::query_scalar!(r#"SELECT 1 AS "probe!: i32""#).fetch_one(state.idempotency.pool());
    let resp = match probe.await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))),
        Err(e) => {
            warn!(error = %e, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "not_ready", "reason": "database_unreachable"})),
            )
        }
    };
    state
        .metrics
        .health_check_duration_seconds
        .with_label_values(&["readyz"])
        .observe(start.elapsed().as_secs_f64());
    resp
}

#[utoipa::path(
    post,
    path = "/v1/declarations",
    tag = "declarations",
    operation_id = "submitDeclaration",
    request_body = SubmitDeclarationRequest,
    params(
        ("Idempotency-Key" = Option<String>, Header,
            description = "Optional client-supplied idempotency key. Replays of the same key with the same body return the original response."),
    ),
    responses(
        (status = 201, description = "Declaration accepted and persisted", body = SubmitDeclarationResponse),
        (status = 200, description = "Idempotent replay; returns the recorded response", body = SubmitDeclarationResponse),
        (status = 400, description = "Malformed request body", body = crate::api::dto::ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token or bad attestation", body = crate::api::dto::ErrorEnvelope),
        (status = 403, description = "Attestation principal mismatch / authorisation denied", body = crate::api::dto::ErrorEnvelope),
        (status = 409, description = "Idempotency conflict OR optimistic concurrency conflict", body = crate::api::dto::ErrorEnvelope),
        (status = 429, description = "Rate-limited (OPS-1; token-bucket per principal)", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        entity_id = %req.entity_id,
        idempotency_key = idempotency_key_field(&headers),
    )
)]
pub(crate) async fn submit_declaration(
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
    // Capture the declaration kind for the OBS-1 counter. Bounded enum
    // — 5 possible values — so safe as a Prometheus label (D18).
    let kind_label = cmd.kind.as_str();

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

    // OBS-1: domain counter — only after successful persistence, so
    // 4xx rejects do NOT inflate this counter. `kind` is a bounded
    // 5-value enum (DeclarationKind::as_str), satisfying D18 label
    // cardinality.
    state
        .metrics
        .declarations_submitted_total
        .with_label_values(&[kind_label])
        .inc();

    info!(
        declaration_id = %declaration_id,
        receipt_hash = %response.receipt_hash_hex,
        "declaration submitted"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/v1/declarations/{declaration_id}",
    tag = "declarations",
    operation_id = "getDeclaration",
    params(
        ("declaration_id" = String, Path, format = "uuid", description = "Declaration UUID"),
    ),
    responses(
        (status = 200, description = "Current projection of the declaration", body = GetDeclarationResponse),
        (status = 401, description = "Authentication required", body = crate::api::dto::ErrorEnvelope),
        (status = 403, description = "Declaration is owned by a different principal", body = crate::api::dto::ErrorEnvelope),
        (status = 404, description = "Declaration not found", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        declaration_id = %declaration_id,
    )
)]
pub(crate) async fn get_declaration(
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

#[utoipa::path(
    post,
    path = "/v1/declarations/{declaration_id}/supersede",
    tag = "declarations",
    operation_id = "supersedeDeclaration",
    params(
        ("declaration_id" = String, Path, format = "uuid",
            description = "Identifier of the declaration to supersede"),
    ),
    request_body = SubmitDeclarationRequest,
    responses(
        (status = 201, description = "Successor declaration accepted; previous record marked superseded", body = SupersedeDeclarationResponse),
        (status = 400, description = "Malformed request body", body = crate::api::dto::ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token or bad attestation", body = crate::api::dto::ErrorEnvelope),
        (status = 403, description = "Caller is not the owner of the prior declaration", body = crate::api::dto::ErrorEnvelope),
        (status = 404, description = "Prior declaration not found", body = crate::api::dto::ErrorEnvelope),
        (status = 409, description = "Already superseded or optimistic-concurrency conflict", body = crate::api::dto::ErrorEnvelope),
        (status = 429, description = "Rate-limited (OPS-1)", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        superseded_declaration_id = %superseded_declaration_id,
        new_entity_id = %req.entity_id,
    )
)]
pub(crate) async fn supersede_declaration(
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

/// Canonical bytes for an amendment. Same JCS-style construction as
/// `canonical_payload_bytes` for submit, but parameterised on the
/// amendment-side fields and the resolved `entity_id`. The entity_id
/// is fixed at submit time (Amend cannot change it); the declarant's
/// canonical bytes therefore include the original `entity_id`.
fn canonical_amend_bytes(
    req: &AmendDeclarationRequest,
    declarant_principal: &str,
    entity_id: &crate::domain::EntityId,
) -> Result<Vec<u8>, ServiceError> {
    use serde::Serialize;
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
        entity_id,
        declarant_principal,
        declarant_role: req.declarant_role.as_str(),
        kind: "amendment",
        effective_from: req.effective_from,
        beneficial_owners: &req.beneficial_owners,
        nonce_hex: &req.attestation.nonce_hex,
    };
    serde_json::to_vec(&canonical)
        .map_err(|_| ServiceError::BadRequest("could not canonicalise amend request".into()))
}

/// Canonical bytes for a correction. The canonical declaration body
/// is unchanged by a correction, so the attestation covers the
/// correction metadata bytes — `metadata_notes` + nonce + principal.
/// This protects against a stolen attestation being reused against a
/// different correction.
fn canonical_correction_bytes(
    req: &CorrectDeclarationRequest,
    declarant_principal: &str,
    declaration_id: &DeclarationId,
) -> Result<Vec<u8>, ServiceError> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Canonical<'a> {
        declaration_id: &'a DeclarationId,
        declarant_principal: &'a str,
        kind: &'static str,
        metadata_notes: Option<&'a str>,
        nonce_hex: &'a str,
    }
    let canonical = Canonical {
        declaration_id,
        declarant_principal,
        kind: "correction",
        metadata_notes: req.metadata_notes.as_deref(),
        nonce_hex: &req.attestation.nonce_hex,
    };
    serde_json::to_vec(&canonical)
        .map_err(|_| ServiceError::BadRequest("could not canonicalise correction request".into()))
}

#[utoipa::path(
    post,
    path = "/v1/declarations/{declaration_id}/amend",
    tag = "declarations",
    operation_id = "amendDeclaration",
    params(
        ("declaration_id" = String, Path, format = "uuid",
            description = "Identifier of the declaration to amend in place"),
    ),
    request_body = AmendDeclarationRequest,
    responses(
        (status = 200, description = "Amendment applied; the declaration row was re-projected", body = AmendDeclarationResponse),
        (status = 400, description = "Malformed request body or invariant violation", body = crate::api::dto::ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token or bad attestation", body = crate::api::dto::ErrorEnvelope),
        (status = 403, description = "Caller is not the owner of the declaration", body = crate::api::dto::ErrorEnvelope),
        (status = 404, description = "Declaration not found", body = crate::api::dto::ErrorEnvelope),
        (status = 409, description = "State-machine refusal — declaration is Accepted/Rejected/Superseded (use Supersede or re-submit)", body = crate::api::dto::ErrorEnvelope),
        (status = 429, description = "Rate-limited (OPS-1)", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        declaration_id = %declaration_id,
    )
)]
pub(crate) async fn amend_declaration(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(declaration_id): Path<Uuid>,
    Json(req): Json<AmendDeclarationRequest>,
) -> Result<(StatusCode, Json<AmendDeclarationResponse>), ServiceError> {
    // 1. Resolve the aggregate's entity_id from the projection so the
    //    canonical-bytes computation matches what the declarant signed.
    //    Owner-check is enforced by the aggregate (`handle_amend`); we
    //    still need entity_id from the projection to canonicalise.
    let declaration_id = DeclarationId(declaration_id);
    let projection = state
        .get_usecase
        .execute(declaration_id)
        .await
        .map_err(ServiceError::from)?;

    // Belt-and-braces: surface a 403 on cross-principal amend at the
    // API layer too. The aggregate would refuse with AmendNotOwner;
    // the early 403 avoids leaking the projection metadata to a non-owner.
    if projection.declarant_principal != principal.subject {
        return Err(ServiceError::AuthorizationDenied(
            "declaration is owned by a different principal",
        ));
    }

    // 2. Verify the attestation over the AMENDED canonical bytes.
    let canonical_bytes =
        canonical_amend_bytes(&req, &principal.subject, &projection.entity_id)?;
    req.attestation
        .verify_against(&canonical_bytes)
        .map_err(|e| ServiceError::AttestationVerificationFailed(e.to_string()))?;

    // 3. Build the command and execute.
    let correlation_id = Uuid::now_v7();
    let cmd = req.into_command(declaration_id, principal.subject.clone(), correlation_id);
    let receipt = state.amend_usecase.execute(cmd).await?;
    let response = AmendDeclarationResponse::from_receipt(receipt, &state.base_url);
    // OBS-1: increment AFTER successful persistence. Label `result` is
    // a bounded enum — `success` here, the error paths above already
    // returned early. (Failure samples would inflate the counter
    // incorrectly so we deliberately only emit on success; the HTTP
    // counter still captures the non-2xx tally per-endpoint.)
    state
        .metrics
        .declarations_amended_total
        .with_label_values(&["success"])
        .inc();
    info!(
        declaration_id = %response.declaration_id,
        aggregate_version = response.aggregate_version,
        "declaration amended"
    );
    Ok((StatusCode::OK, Json(response)))
}

#[utoipa::path(
    post,
    path = "/v1/declarations/{declaration_id}/correct",
    tag = "declarations",
    operation_id = "correctDeclaration",
    params(
        ("declaration_id" = String, Path, format = "uuid",
            description = "Identifier of the declaration to correct (pre-verification only)"),
    ),
    request_body = CorrectDeclarationRequest,
    responses(
        (status = 200, description = "Correction applied; metadata updated", body = CorrectDeclarationResponse),
        (status = 400, description = "Malformed request body", body = crate::api::dto::ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token or bad attestation", body = crate::api::dto::ErrorEnvelope),
        (status = 403, description = "Caller is not the owner of the declaration", body = crate::api::dto::ErrorEnvelope),
        (status = 404, description = "Declaration not found", body = crate::api::dto::ErrorEnvelope),
        (status = 409, description = "State-machine refusal — corrections are admitted only in `submitted` (use amend or supersede)", body = crate::api::dto::ErrorEnvelope),
        (status = 429, description = "Rate-limited (OPS-1)", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        declaration_id = %declaration_id,
    )
)]
pub(crate) async fn correct_declaration(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(declaration_id): Path<Uuid>,
    Json(req): Json<CorrectDeclarationRequest>,
) -> Result<(StatusCode, Json<CorrectDeclarationResponse>), ServiceError> {
    let declaration_id = DeclarationId(declaration_id);
    let projection = state
        .get_usecase
        .execute(declaration_id)
        .await
        .map_err(ServiceError::from)?;
    if projection.declarant_principal != principal.subject {
        return Err(ServiceError::AuthorizationDenied(
            "declaration is owned by a different principal",
        ));
    }

    let canonical_bytes =
        canonical_correction_bytes(&req, &principal.subject, &declaration_id)?;
    req.attestation
        .verify_against(&canonical_bytes)
        .map_err(|e| ServiceError::AttestationVerificationFailed(e.to_string()))?;

    let correlation_id = Uuid::now_v7();
    let cmd = req.into_command(declaration_id, principal.subject.clone(), correlation_id);
    let receipt = state.correct_usecase.execute(cmd).await?;
    let response = CorrectDeclarationResponse::from_receipt(receipt, &state.base_url);
    // OBS-1: domain counter — only on success. See the amend handler
    // for the rationale on `result="success"` only.
    state
        .metrics
        .declarations_corrected_total
        .with_label_values(&["success"])
        .inc();
    info!(
        declaration_id = %response.declaration_id,
        aggregate_version = response.aggregate_version,
        "declaration corrected"
    );
    Ok((StatusCode::OK, Json(response)))
}

#[utoipa::path(
    get,
    path = "/v1/declarations/by-principal",
    tag = "declarations",
    operation_id = "listDeclarationsByPrincipal",
    description = "Data-subject access (COMP-1). Returns every declaration RÉCOR holds where the authenticated principal is the declarant. The principal is sourced exclusively from the authenticated session (D17) — no path parameter, no body, no query string. Implements GDPR right-of-access and data-portability rights; see docs/compliance/gdpr-procedures.md. Each row carries its receipt_hash_hex so the declarant can re-verify the receipt offline (D15).",
    responses(
        (status = 200, description = "Every declaration the authenticated principal is the declarant on, most-recent first", body = DeclarationsByPrincipalResponse),
        (status = 401, description = "Missing/invalid bearer token", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        principal = %principal.subject,
        event_kind = "data_subject_access",
    )
)]
pub(crate) async fn list_declarations_by_principal(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
) -> Result<Json<DeclarationsByPrincipalResponse>, ServiceError> {
    // D17 (zero trust): `principal.subject` is the authenticated subject
    // resolved by `auth_middleware` from either the OIDC Bearer token or
    // the dev-only `X-Recor-Dev-Principal` header. There is no path
    // parameter or body — the only datum that selects rows is the
    // authenticated session.
    //
    // D15 (cryptographic provenance): each returned projection includes
    // the receipt_hash_hex; the declarant can re-verify each receipt
    // offline against the canonical bytes they signed.
    //
    // D14 (fail-closed): a repository error surfaces as 500; an empty
    // principal (which would indicate an auth-middleware bug) becomes
    // an internal error rather than dumping every declaration.
    //
    // Audit signal: the structured fields above stamp `event_kind =
    // "data_subject_access"` on this span. OPS-2's RedactingLayer
    // redacts the `principal` field; the event itself is the audit
    // record that someone exercised their access right at this time.
    let projections = state
        .list_by_principal_usecase
        .execute(&principal.subject)
        .await?;

    let declarations: Vec<GetDeclarationResponse> =
        projections.into_iter().map(GetDeclarationResponse::from).collect();
    let response = DeclarationsByPrincipalResponse {
        principal: principal.subject.clone(),
        count: declarations.len(),
        declarations,
    };

    // Emit the audit signal as an event on the current span. The
    // RedactingLayer (OPS-2) suppresses the principal value; the
    // record of "a data_subject_access event happened at this time
    // with N rows returned" remains in the chain.
    info!(
        event_kind = "data_subject_access",
        result_count = response.count,
        "data-subject access right exercised"
    );

    Ok(Json(response))
}
