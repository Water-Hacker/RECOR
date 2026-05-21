//! REST route definitions. Axum router + handlers for the Person service.

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
use crate::api::dlq::DlqAdminState;
use crate::api::dto::{
    ErrorEnvelope, GetPersonResponse, HealthzResponse, MergePersonsResponse,
    ReadyzResponse, RegisterPersonRequest, RegisterPersonResponse, SearchPersonsResponse,
};
use crate::api::OidcVerifier;
use crate::application::{
    GetPersonUseCase, MergePersonsUseCase, RegisterPersonUseCase, SearchPersonsUseCase,
    SearchQuery,
};
use crate::config::Config;
use crate::domain::{MergePersons, PersonId};
use crate::error::ServiceError;
use crate::infrastructure::outbox_admin::OutboxAdminStore;
use crate::infrastructure::postgres::IdempotencyStore;
use crate::metrics::{metrics_handler, metrics_middleware, Metrics};

#[derive(Clone)]
pub struct AppState {
    pub register_usecase: Arc<RegisterPersonUseCase>,
    pub get_usecase: Arc<GetPersonUseCase>,
    pub search_usecase: Arc<SearchPersonsUseCase>,
    pub merge_usecase: Arc<MergePersonsUseCase>,
    pub idempotency: Arc<IdempotencyStore>,
    /// TODO-040 — DLQ admin store. Surfaces dead-lettered outbox rows
    /// the relay produces; reused by `/v1/internal/outbox-dlq*`.
    pub outbox_admin: Arc<OutboxAdminStore>,
    pub base_url: String,
    pub is_dev: bool,
    pub idempotency_ttl_seconds: i64,
    pub oidc: Option<Arc<OidcVerifier>>,
    pub metrics: Arc<Metrics>,
    pub admin_principals: Arc<HashSet<String>>,
}

/// Build the main router for the person service.
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

    let protected = Router::new()
        .route("/v1/persons", post(register_person))
        .route("/v1/persons/search", get(search_persons))
        .route("/v1/persons/{person_id}", get(get_person))
        .route(
            "/v1/persons/{person_id}/merge-into/{target_id}",
            post(merge_persons),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // TODO-040 — DLQ admin endpoints. Same user-auth middleware as the
    // protected routes; the handlers gate themselves on
    // `admin_principals`. Empty list ⇒ both endpoints return 503.
    let dlq_admin_state = DlqAdminState {
        store: state.outbox_admin.clone(),
        admin_principals: state.admin_principals.clone(),
        metrics: state.metrics.clone(),
    };
    let admin = Router::new()
        .route(
            "/v1/internal/outbox-dlq",
            get(crate::api::dlq::list_dlq),
        )
        .route(
            "/v1/internal/outbox-dlq/{id}/replay",
            post(crate::api::dlq::replay_dlq),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(dlq_admin_state);

    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state.clone());

    let openapi = crate::api::openapi::openapi_routes();

    let app_routes = protected.merge(admin).merge(public).merge(openapi);

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

/// FIND-007: minimal router that serves ONLY `/metrics`. Bound on a
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
        (status = 200, description = "Service ready", body = ReadyzResponse),
        (status = 503, description = "Database unreachable", body = ReadyzResponse),
    ),
)]
#[tracing::instrument(level = "info", skip(state))]
pub(crate) async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(state.idempotency.pool());
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
    path = "/v1/persons",
    tag = "persons",
    operation_id = "registerPerson",
    request_body = RegisterPersonRequest,
    params(
        ("Idempotency-Key" = Option<String>, Header,
            description = "Optional idempotency key; replays of the same key with the same body return the original response."),
    ),
    responses(
        (status = 201, description = "Person registered", body = RegisterPersonResponse),
        (status = 200, description = "Idempotent replay", body = RegisterPersonResponse),
        (status = 400, description = "Malformed request body / invariant violation", body = ErrorEnvelope),
        (status = 401, description = "Missing/invalid bearer token", body = ErrorEnvelope),
        (status = 403, description = "Caller is not in the admin allowlist (FIND-006)", body = ErrorEnvelope),
        (status = 409, description = "Idempotency conflict OR person already registered", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
        (status = 503, description = "Endpoint disabled — ADMIN_PRINCIPALS empty (FIND-006)", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        actor_principal = %principal.subject,
        person_id = ?req.person_id,
        idempotency_key = idempotency_key_field(&headers),
    )
)]
pub(crate) async fn register_person(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    headers: HeaderMap,
    Json(req): Json<RegisterPersonRequest>,
) -> Result<(StatusCode, Json<RegisterPersonResponse>), ServiceError> {
    // FIND-006 (audit Sprint 1 interim mitigation): person rows carry
    // Sensitive-PII (primary_id_document, biometric_reference_hash).
    // Until NDI integration lands (R-DECL-4 follow-up), the legitimate
    // creator of a Person row is an operator on the admin allowlist
    // (declarant-driven creation re-enables once an external authority
    // can validate the identity). Empty allowlist disables the endpoint
    // entirely (D14 fail-closed); non-admin principals get 403.
    refuse_unless_admin(&state.admin_principals, &principal)?;
    let correlation_id = Uuid::now_v7();

    // Stable canonical bytes for the idempotency hash. The actor_principal
    // is from auth (D17), so we include it explicitly in the hash key.
    let request_hash = canonical_request_hash(&req, &principal.subject)?;

    let cmd = req.into_command(principal.subject.clone(), correlation_id);
    let person_id = cmd.person_id;

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
                let stored: RegisterPersonResponse =
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

    let receipt = state.register_usecase.execute(cmd).await?;
    let response = RegisterPersonResponse::from_receipt(receipt, &state.base_url);

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
        .persons_registered_total
        .with_label_values(&["success"])
        .inc();

    info!(person_id = %person_id, "person registered");
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/v1/persons/{person_id}",
    tag = "persons",
    operation_id = "getPerson",
    params(("person_id" = String, Path, format = "uuid", description = "Person UUID")),
    responses(
        (status = 200, description = "Person projection", body = GetPersonResponse),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 404, description = "Not found", body = ErrorEnvelope),
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
        actor_principal = %principal.subject,
        person_id = %person_id,
    )
)]
pub(crate) async fn get_person(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(person_id): Path<Uuid>,
) -> Result<Json<GetPersonResponse>, ServiceError> {
    // FIND-005: per-row RBAC. Load the projection, then enforce
    // `principal == created_by_principal OR principal IN admin_allowlist`.
    // On denial, return 404 (not 403) so non-owners cannot enumerate
    // person_ids by inferring existence from the response code.
    let projection = state.get_usecase.execute(PersonId(person_id)).await?;
    if !is_admin(&state.admin_principals, &principal)
        && projection.created_by_principal != principal.subject
    {
        // Log loud enough for forensics but do not leak existence to
        // the caller — the response shape mirrors a true not-found.
        tracing::warn!(
            actor = %principal.subject,
            owner = %projection.created_by_principal,
            person_id = %person_id,
            "GET person projection refused — non-owner, non-admin"
        );
        return Err(ServiceError::NotFound(person_id.to_string()));
    }
    Ok(Json(projection.into()))
}

#[utoipa::path(
    get,
    path = "/v1/persons/search",
    tag = "persons",
    operation_id = "searchPersons",
    params(
        ("q" = String, Query, description = "Free-form fragment to match against canonical_full_name"),
        ("nationality" = Option<String>, Query, description = "Optional ISO 3166-1 alpha-2 filter"),
        ("limit" = Option<i64>, Query, description = "Page size; clamped to [1, 50]"),
    ),
    responses(
        (status = 200, description = "Search results", body = SearchPersonsResponse),
        (status = 400, description = "Empty / overlong query", body = ErrorEnvelope),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
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
        actor_principal = %principal.subject,
    )
)]
pub(crate) async fn search_persons(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchPersonsResponse>, ServiceError> {
    // FIND-005 RBAC scope: admin callers see every row matching the
    // textual filters; non-admin callers see only rows they
    // themselves registered. The filter is computed once here and
    // propagated through the use case to the repository's WHERE
    // clause — never materialising the full result set then trimming
    // it in Rust (D14 fail-closed; no PII transit even briefly).
    let nationality_label = if query.nationality.is_some() { "yes" } else { "no" };
    state
        .metrics
        .persons_search_total
        .with_label_values(&[nationality_label])
        .inc();
    let created_by_filter = if is_admin(&state.admin_principals, &principal) {
        None
    } else {
        Some(principal.subject.as_str())
    };
    let rows = state
        .search_usecase
        .execute(query, created_by_filter)
        .await?;
    Ok(Json(SearchPersonsResponse::from_projections(rows)))
}

#[utoipa::path(
    post,
    path = "/v1/persons/{person_id}/merge-into/{target_id}",
    tag = "persons",
    operation_id = "mergePersons",
    params(
        ("person_id" = String, Path, format = "uuid", description = "Source (duplicate) person UUID"),
        ("target_id" = String, Path, format = "uuid", description = "Target (canonical) person UUID"),
    ),
    responses(
        (status = 200, description = "Merge applied", body = MergePersonsResponse),
        (status = 400, description = "Self-merge or invariant violation", body = ErrorEnvelope),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 403, description = "Caller is not in the admin allowlist", body = ErrorEnvelope),
        (status = 404, description = "Source or target person not found", body = ErrorEnvelope),
        (status = 409, description = "Source already merged or target is a merged-out shell", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
        (status = 503, description = "Admin endpoint disabled (no admin principals configured)", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(
    skip_all,
    fields(
        actor_principal = %principal.subject,
        person_id = %person_id,
        target_id = %target_id,
    )
)]
pub(crate) async fn merge_persons(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path((person_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<MergePersonsResponse>, ServiceError> {
    if state.admin_principals.is_empty() {
        return Err(ServiceError::AdminDisabled);
    }
    if !state.admin_principals.contains(&principal.subject) {
        return Err(ServiceError::AuthorizationDenied(
            "principal is not in the admin allowlist",
        ));
    }
    let cmd = MergePersons {
        from_person_id: PersonId(person_id),
        into_person_id: PersonId(target_id),
        actor_principal: principal.subject.clone(),
        merged_at: time::OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    };
    let receipt = state.merge_usecase.execute(cmd).await?;
    state
        .metrics
        .persons_merged_total
        .with_label_values(&["success"])
        .inc();
    info!(
        from = %person_id,
        into = %target_id,
        actor = %principal.subject,
        "persons merged"
    );
    Ok(Json(receipt.into()))
}

/// FIND-005 / FIND-006: shared admin-gate helper used by
/// `register_person`. Mirrors the inline check in `merge_persons` so
/// the two surfaces stay byte-identical when the admin posture is
/// updated. Empty allowlist ⇒ 503 (`AdminDisabled`); authenticated
/// non-admin ⇒ 403 (`AuthorizationDenied`).
fn refuse_unless_admin(
    admin_principals: &std::collections::HashSet<String>,
    principal: &Principal,
) -> Result<(), ServiceError> {
    if admin_principals.is_empty() {
        tracing::warn!(
            "person-service admin endpoint hit but ADMIN_PRINCIPALS is empty — \
             endpoint disabled (D14 fail-closed)"
        );
        return Err(ServiceError::AdminDisabled);
    }
    if !admin_principals.contains(&principal.subject) {
        tracing::warn!(
            principal = %principal.subject,
            "non-admin principal attempted person-service admin endpoint"
        );
        return Err(ServiceError::AuthorizationDenied(
            "principal is not in the admin allowlist",
        ));
    }
    Ok(())
}

/// FIND-005: cheap admin-membership probe used by `get_person` and
/// `search_persons` to decide whether to apply the per-row RBAC
/// predicate. Empty allowlist always returns `false`, mirroring the
/// fail-closed semantics of `refuse_unless_admin`.
fn is_admin(
    admin_principals: &std::collections::HashSet<String>,
    principal: &Principal,
) -> bool {
    !admin_principals.is_empty()
        && admin_principals.contains(&principal.subject)
}

fn idempotency_key_field(headers: &HeaderMap) -> String {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn canonical_request_hash(
    req: &RegisterPersonRequest,
    principal: &str,
) -> Result<String, ServiceError> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Canonical<'a> {
        person_id: &'a Option<PersonId>,
        actor_principal: &'a str,
        attributes: &'a crate::domain::value_object::PersonAttributes,
    }
    let canonical = Canonical {
        person_id: &req.person_id,
        actor_principal: principal,
        attributes: &req.attributes,
    };
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|_| ServiceError::BadRequest("could not canonicalise request".into()))?;
    let mut h = Hasher::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize().as_bytes()))
}

#[cfg(test)]
mod rbac_tests {
    //! Unit tests for the FIND-005 / FIND-006 RBAC helpers
    //! (`refuse_unless_admin` and `is_admin`). Handler-level
    //! integration is exercised via the application-layer
    //! `SearchPersonsUseCase` test in
    //! `crate::application::search_persons::tests::created_by_filter_propagates_to_repository`.

    use std::collections::HashSet;

    use crate::api::auth::{Principal, PrincipalSource};

    use super::*;

    fn principal(subject: &str) -> Principal {
        Principal {
            subject: subject.to_string(),
            source: PrincipalSource::DevHeader,
        }
    }

    fn allowlist(subjects: &[&str]) -> HashSet<String> {
        subjects.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn refuse_unless_admin_503_on_empty_allowlist() {
        let res = refuse_unless_admin(&HashSet::new(), &principal("anyone"));
        assert!(matches!(res, Err(ServiceError::AdminDisabled)));
    }

    #[test]
    fn refuse_unless_admin_403_on_non_admin() {
        let res = refuse_unless_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/declarant-7"),
        );
        assert!(matches!(res, Err(ServiceError::AuthorizationDenied(_))));
    }

    #[test]
    fn refuse_unless_admin_ok_for_listed_principal() {
        let res = refuse_unless_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/ops-1"),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn is_admin_false_on_empty_allowlist() {
        assert!(!is_admin(&HashSet::new(), &principal("anyone")));
    }

    #[test]
    fn is_admin_false_on_non_admin() {
        assert!(!is_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/declarant-7"),
        ));
    }

    #[test]
    fn is_admin_true_for_listed_principal() {
        assert!(is_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/ops-1"),
        ));
    }
}
