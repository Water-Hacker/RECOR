//! REST route definitions. Axum router + handlers.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use blake3::Hasher;
use serde::Deserialize;
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
    DissolveEntityRequest, DissolveResponse, ErrorEnvelope, GetEntityResponse, HealthzResponse,
    ReadyzResponse, RegisterEntityRequest, RegisterEntityResponse, SearchEntitiesResponse,
    UpdateEntityRequest, UpdateResponse,
};
use crate::api::oidc::OidcVerifier;
use crate::application::{
    DissolveEntityUseCase, GetEntityUseCase, RegisterEntityUseCase, SearchCriteria,
    SearchEntitiesUseCase, UpdateEntityUseCase,
};
use crate::config::Config;
use crate::domain::EntityId;
use crate::error::ServiceError;
use crate::infrastructure::IdempotencyStore;
use crate::metrics::{metrics_handler, metrics_middleware, Metrics};

#[derive(Clone)]
pub struct AppState {
    pub register_usecase: Arc<RegisterEntityUseCase>,
    pub get_usecase: Arc<GetEntityUseCase>,
    pub search_usecase: Arc<SearchEntitiesUseCase>,
    pub update_usecase: Arc<UpdateEntityUseCase>,
    pub dissolve_usecase: Arc<DissolveEntityUseCase>,
    pub idempotency: Arc<IdempotencyStore>,
    pub base_url: String,
    pub is_dev: bool,
    pub idempotency_ttl_seconds: i64,
    pub oidc: Option<Arc<OidcVerifier>>,
    pub metrics: Arc<Metrics>,
    /// Admin principals authorised for the dissolve endpoint. D17 — the
    /// allowlist is canonical; empty list ⇒ /dissolve returns 503.
    pub admin_principals: Arc<HashSet<String>>,
}

/// Build the main router for the entity service.
///
/// `expose_metrics_on_main`:
///   - `true` (current default): `/metrics` is mounted on the main
///     listener. Backwards-compatible.
///   - `false` (FIND-007): `/metrics` omitted; `main.rs` is expected
///     to bind a separate listener via `metrics_only_router`.
pub fn router(state: AppState, cfg: &Config, expose_metrics_on_main: bool) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
        metrics: state.metrics.clone(),
    };

    let protected = Router::new()
        .route("/v1/entities", post(register_entity))
        .route("/v1/entities/search", get(search_entities))
        .route("/v1/entities/{entity_id}", get(get_entity))
        .route(
            "/v1/entities/{entity_id}/update",
            post(update_entity_handler),
        )
        .route(
            "/v1/entities/{entity_id}/dissolve",
            post(dissolve_entity_handler),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(state.clone());

    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state.clone());

    let openapi = crate::api::openapi::openapi_routes();

    let app_routes = protected.merge(public).merge(openapi);

    let metrics_state = state.metrics.clone();
    let app_routes = app_routes.layer(axum::middleware::from_fn_with_state(
        metrics_state,
        metrics_middleware,
    ));

    // FIND-007: /metrics is conditionally mounted. See router doc-comment.
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

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    operation_id = "healthz",
    responses(
        (status = 200, description = "Service process is alive", body = HealthzResponse),
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
        (status = 200, description = "Service is ready", body = ReadyzResponse),
        (status = 503, description = "Dependency unreachable", body = ReadyzResponse),
    ),
)]
#[tracing::instrument(level = "info", skip(state))]
pub(crate) async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
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
    path = "/v1/entities",
    tag = "entities",
    operation_id = "registerEntity",
    request_body = RegisterEntityRequest,
    params(
        ("Idempotency-Key" = Option<String>, Header,
            description = "Optional idempotency key. Replays with the same body return the original response."),
    ),
    responses(
        (status = 201, description = "Entity registered", body = RegisterEntityResponse),
        (status = 200, description = "Idempotent replay", body = RegisterEntityResponse),
        (status = 400, description = "Malformed request body", body = ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token", body = ErrorEnvelope),
        (status = 409, description = "Duplicate identity tuple or idempotency conflict", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
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
        jurisdiction = %req.jurisdiction,
        idempotency_key = idempotency_key_field(&headers),
    )
)]
pub(crate) async fn register_entity(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    headers: HeaderMap,
    Json(req): Json<RegisterEntityRequest>,
) -> Result<(StatusCode, Json<RegisterEntityResponse>), ServiceError> {
    let correlation_id = Uuid::now_v7();

    // Idempotency hash over the canonical request shape (JSON re-encode).
    let request_hash = {
        let canonical = serde_json::to_vec(&req)
            .map_err(|_| ServiceError::BadRequest("could not canonicalise request".into()))?;
        let mut h = Hasher::new();
        h.update(&canonical);
        h.update(principal.subject.as_bytes());
        hex::encode(h.finalize().as_bytes())
    };

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
                let stored: RegisterEntityResponse =
                    serde_json::from_value(existing.response_body)
                        .map_err(|_| ServiceError::Internal)?;
                let status = StatusCode::from_u16(
                    u16::try_from(existing.response_status).unwrap_or(200),
                )
                .unwrap_or(StatusCode::OK);
                return Ok((status, Json(stored)));
            }
            Ok(None) => {}
            Err(e) => {
                warn!(error = ?e, "idempotency lookup failed; proceeding without replay");
            }
        }
    }

    // TODO(R-VER-1): wire BUNEC as source-of-truth for jurisdiction == "CM"
    // BEFORE executing register_usecase. Until the BUNEC adapter lands,
    // the registration path is declarant-submitted in both directions.

    let cmd = req
        .into_command(principal.subject.clone(), correlation_id)
        .map_err(ServiceError::Domain)?;
    let jurisdiction_label = if cmd.jurisdiction.as_str() == "CM" {
        "cm"
    } else {
        "other"
    };

    let receipt = state.register_usecase.execute(cmd).await?;
    let response = RegisterEntityResponse::from_receipt(receipt, &state.base_url);

    if let Some(key) = idem_key {
        let body_value =
            serde_json::to_value(&response).map_err(|_| ServiceError::Internal)?;
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
            warn!(error = ?e, "idempotency record failed; registration succeeded but replay disabled");
        }
    }

    state
        .metrics
        .entities_registered_total
        .with_label_values(&[jurisdiction_label])
        .inc();

    info!(entity_id = %response.entity_id, "entity registered");
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/v1/entities/{entity_id}",
    tag = "entities",
    operation_id = "getEntity",
    params(
        ("entity_id" = String, Path, format = "uuid", description = "Entity UUID"),
    ),
    responses(
        (status = 200, description = "Current projection of the entity", body = GetEntityResponse),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 404, description = "Entity not found", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, entity_id = %entity_id))]
pub(crate) async fn get_entity(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(entity_id): Path<Uuid>,
) -> Result<Json<GetEntityResponse>, ServiceError> {
    let _ = principal; // any authenticated caller may read the public projection
    let projection = state.get_usecase.execute(EntityId(entity_id)).await?;
    Ok(Json(projection.into()))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub jurisdiction: Option<String>,
    #[serde(rename = "type")]
    pub entity_type: Option<String>,
    pub limit: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/v1/entities/search",
    tag = "entities",
    operation_id = "searchEntities",
    params(
        ("q" = Option<String>, Query, description = "Substring match on canonical_name (ILIKE)"),
        ("jurisdiction" = Option<String>, Query, description = "ISO-3166-1 alpha-2 jurisdiction filter"),
        ("type" = Option<String>, Query, description = "entity_type prefix filter (sa, sarl, partnership, trust, other)"),
        ("limit" = Option<u32>, Query, description = "Page size (default 50; max 200)"),
    ),
    responses(
        (status = 200, description = "Matched entities", body = SearchEntitiesResponse),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn search_entities(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchEntitiesResponse>, ServiceError> {
    let _ = principal;
    let criteria = SearchCriteria {
        q: q.q,
        jurisdiction: q.jurisdiction,
        entity_type: q.entity_type,
        limit: q.limit.unwrap_or(0),
    };
    let projections = state.search_usecase.execute(criteria).await?;
    let count = projections.len();
    let items = projections.into_iter().map(GetEntityResponse::from).collect();
    Ok(Json(SearchEntitiesResponse { items, count }))
}

#[utoipa::path(
    post,
    path = "/v1/entities/{entity_id}/update",
    tag = "entities",
    operation_id = "updateEntity",
    params(
        ("entity_id" = String, Path, format = "uuid"),
    ),
    request_body = UpdateEntityRequest,
    responses(
        (status = 200, description = "Entity updated", body = UpdateResponse),
        (status = 400, description = "Malformed request body", body = ErrorEnvelope),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 404, description = "Entity not found", body = ErrorEnvelope),
        (status = 409, description = "Entity is dissolved", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, entity_id = %entity_id))]
pub(crate) async fn update_entity_handler(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(entity_id): Path<Uuid>,
    Json(req): Json<UpdateEntityRequest>,
) -> Result<Json<UpdateResponse>, ServiceError> {
    let correlation_id = Uuid::now_v7();
    let cmd = req
        .into_command(EntityId(entity_id), principal.subject.clone(), correlation_id)
        .map_err(ServiceError::Domain)?;
    let receipt = state.update_usecase.execute(cmd).await?;
    state
        .metrics
        .entities_updated_total
        .with_label_values(&["success"])
        .inc();
    Ok(Json(UpdateResponse {
        entity_id: receipt.entity_id,
        updated_at: receipt.updated_at,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/entities/{entity_id}/dissolve",
    tag = "entities",
    operation_id = "dissolveEntity",
    params(
        ("entity_id" = String, Path, format = "uuid"),
    ),
    request_body = DissolveEntityRequest,
    responses(
        (status = 200, description = "Entity dissolved", body = DissolveResponse),
        (status = 400, description = "Malformed request body", body = ErrorEnvelope),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 403, description = "Caller is not on the admin allowlist", body = ErrorEnvelope),
        (status = 404, description = "Entity not found", body = ErrorEnvelope),
        (status = 409, description = "Already dissolved or invalid dissolution date", body = ErrorEnvelope),
        (status = 503, description = "Admin allowlist is empty; endpoint disabled", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, entity_id = %entity_id))]
pub(crate) async fn dissolve_entity_handler(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(entity_id): Path<Uuid>,
    Json(req): Json<DissolveEntityRequest>,
) -> Result<Json<DissolveResponse>, ServiceError> {
    if state.admin_principals.is_empty() {
        return Err(ServiceError::BadRequest(
            "dissolve endpoint disabled (admin allowlist empty)".to_string(),
        ));
    }
    if !state.admin_principals.contains(&principal.subject) {
        return Err(ServiceError::AuthorizationDenied(
            "caller is not on the admin allowlist for dissolve",
        ));
    }
    let correlation_id = Uuid::now_v7();
    let cmd = req.into_command(EntityId(entity_id), principal.subject.clone(), correlation_id);
    let receipt = state.dissolve_usecase.execute(cmd).await?;
    state
        .metrics
        .entities_dissolved_total
        .with_label_values(&["success"])
        .inc();
    Ok(Json(DissolveResponse {
        entity_id: receipt.entity_id,
        dissolved_at: receipt.dissolved_at,
        recorded_at: receipt.recorded_at,
    }))
}

fn idempotency_key_field(headers: &HeaderMap) -> String {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}
