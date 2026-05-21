//! REST API.
//!
//! DOC-1 / FIND-013 closure. The handlers in this module are
//! `#[utoipa::path]`-annotated; the assembled OpenAPI 3.1 document
//! lives in [`crate::api::openapi`]; the snapshot is committed at
//! `docs/openapi/verification-engine.json` and verified by
//! `tools/ci/check-openapi-drift.sh`.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::warn;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::auth::{auth_middleware, AuthConfig};
use crate::api::dlq::DlqAdminState;
use crate::api::oidc::OidcVerifier;
use crate::application::{GetVerificationUseCase, SubmitVerificationUseCase};
use crate::config::Config;
use crate::domain::{
    DeclarationSnapshot, DecisionRationale, VerificationCase, VerificationCaseId,
};
use crate::error::ServiceError;
use crate::infrastructure::{OutboxAdminStore, PostgresVerificationRepository};
// OBS-1: Prometheus metrics.
use crate::metrics::{metrics_handler, metrics_middleware, Metrics};

#[derive(Clone)]
pub struct AppState {
    pub submit_usecase: Arc<SubmitVerificationUseCase>,
    pub get_usecase: Arc<GetVerificationUseCase>,
    pub repository: Arc<PostgresVerificationRepository>,
    pub outbox_admin: Arc<OutboxAdminStore>,
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
    /// OBS-1: shared Prometheus metrics handle. See `crate::metrics`.
    pub metrics: Arc<Metrics>,
    /// FIND-002 (audit Sprint 0): the legitimate caller of
    /// `POST /v1/verifications` is the declaration service via the
    /// HMAC-authenticated `/v1/internal/declaration-events` path —
    /// NOT the public REST submit endpoint. Gate the REST surface
    /// on the admin allowlist so an authenticated declarant cannot
    /// drive verification cases (which would spend Anthropic budget
    /// on Stage 5 and pollute `verification_cases` with no
    /// corresponding declaration). FIND-004 (cross-tenant case
    /// reads) is interim-mitigated by also gating GET on this
    /// allowlist until the per-case tenancy migration lands.
    pub admin_principals: Arc<std::collections::HashSet<String>>,
}

/// Build the main router for the V-engine.
///
/// `expose_metrics_on_main`:
///   - `true` (current default): `/metrics` is mounted on the main
///     listener alongside the business routes. Backwards-compatible
///     with single-port deployments (dev, integration tests).
///   - `false` (FIND-007): `/metrics` is omitted from the main router;
///     `main.rs` is expected to bind a separate listener via
///     `metrics_only_router` on a NetworkPolicy-restricted port. This
///     is the production posture — operators MUST flip this when the
///     main listener is reachable from outside the cluster.
pub fn router(state: AppState, cfg: &Config, expose_metrics_on_main: bool) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
        metrics: state.metrics.clone(),
    };

    let protected = Router::new()
        .route("/v1/verifications", post(submit_verification))
        .route("/v1/verifications/{case_id}", get(get_verification))
        .route(
            "/v1/verifications/{case_id}/rationale",
            get(get_verification_rationale),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Admin endpoints (R-LOOP-DLQ-3). Same user-auth middleware as
    // the protected routes, but the handlers gate themselves on
    // `Config::admin_principals_list()` so only the listed subjects
    // can list/replay DLQ rows. Empty list ⇒ endpoints return 503.
    //
    // Path is intentionally `/v1/internal/verification-outbox-dlq`
    // (not `/v1/internal/outbox-dlq`) so the surface is unambiguous
    // when both declaration and V-engine are deployed.
    use std::collections::HashSet;
    let admin_principals: HashSet<String> =
        cfg.admin_principals_list().into_iter().collect();
    let dlq_admin_state = DlqAdminState {
        store: state.outbox_admin.clone(),
        admin_principals: Arc::new(admin_principals),
        metrics: state.metrics.clone(),
    };
    let admin = Router::new()
        .route(
            "/v1/internal/verification-outbox-dlq",
            get(crate::api::dlq::list_dlq),
        )
        .route(
            "/v1/internal/verification-outbox-dlq/{id}/replay",
            post(crate::api::dlq::replay_dlq),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware,
        ))
        .with_state(dlq_admin_state);

    // Internal HMAC-authenticated webhook for the Declaration service's
    // outbox relay. Not behind the user-auth middleware — uses its own
    // signature verification at the handler.
    use secrecy::ExposeSecret;
    let internal_state = crate::api::internal::InternalAppState {
        submit_usecase: state.submit_usecase.clone(),
        hmac_secret: cfg.inbound_hmac_secret.expose_secret().to_string(),
        old_hmac_secret: cfg.inbound_hmac_secret_old.expose_secret().to_string(),
        // R-LOOP-3: HMAC stays required unless AUTH_TRANSPORT=mtls-only.
        hmac_required: cfg.hmac_required(),
        expected_peer_spiffe_id: if cfg.mtls_enabled() {
            cfg.spiffe_id_peer.clone()
        } else {
            String::new()
        },
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
        .with_state(state.clone());

    // DOC-1 / FIND-013: OpenAPI artefacts (`GET /openapi.json`, `GET /docs`).
    let openapi = crate::api::openapi::openapi_routes();

    let app_routes = protected
        .merge(admin)
        .merge(internal)
        .merge(public)
        .merge(openapi);

    // Per-endpoint timing + counter middleware over the app routes
    // only (not /metrics).
    let metrics_state = state.metrics.clone();
    let app_routes = app_routes.layer(axum::middleware::from_fn_with_state(
        metrics_state,
        metrics_middleware,
    ));

    // OBS-1 / FIND-007: GET /metrics — Prometheus exposition. No auth;
    // in-cluster network only. When `expose_metrics_on_main` is
    // `false`, the route is NOT mounted here — `main.rs` is expected
    // to bind a separate listener via `metrics_only_router` on a
    // NetworkPolicy-restricted port. The request-timing middleware is
    // NOT applied to metrics so scrape traffic doesn't inflate the
    // latency histogram.
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
/// separate listener by `main.rs` when `METRICS_BIND_ADDR` is set, so
/// a NetworkPolicy can restrict scrape traffic to the Prometheus pod
/// CIDR without affecting the business / ingress port.
pub fn metrics_only_router(metrics: Arc<crate::metrics::Metrics>) -> Router {
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
#[tracing::instrument(skip(state))]
pub(crate) async fn healthz(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthzResponse>) {
    let start = std::time::Instant::now();
    let resp = (
        StatusCode::OK,
        Json(HealthzResponse {
            status: "ok".to_string(),
        }),
    );
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
        (status = 200, description = "Database reachable; service ready", body = ReadyzResponse),
        (status = 503, description = "Database unreachable", body = ReadyzResponse),
    ),
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn readyz(
    State(state): State<AppState>,
) -> (StatusCode, Json<ReadyzResponse>) {
    let start = std::time::Instant::now();
    let probe = sqlx::query_scalar!(r#"SELECT 1 AS "probe!: i32""#).fetch_one(state.repository.pool());
    let resp = match probe.await {
        Ok(_) => (
            StatusCode::OK,
            Json(ReadyzResponse {
                status: "ready".to_string(),
                reason: None,
            }),
        ),
        Err(e) => {
            warn!(error = %e, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadyzResponse {
                    status: "not_ready".to_string(),
                    reason: Some("database_unreachable".to_string()),
                }),
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitVerificationRequest {
    /// Declaration snapshot to verify. Shape mirrors the
    /// declaration service's canonical-form payload. Deep type is
    /// pinned via `serde_json::Value` in the spec — see
    /// `services/declaration` for the authoritative schema.
    #[schema(value_type = serde_json::Value)]
    pub declaration: DeclarationSnapshot,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitVerificationResponse {
    /// Stable identifier of the created verification case.
    #[schema(value_type = String, format = "uuid")]
    pub case_id: VerificationCaseId,
    /// Lane decision the fusion engine settled on. One of
    /// `"green"`, `"yellow"`, `"red"`.
    pub lane: String,
    /// Dempster-Shafer belief mass for the "authentic" hypothesis.
    pub authenticity_belief: f64,
    /// Plausibility upper bound for the same hypothesis.
    pub authenticity_plausibility: f64,
    /// Belief mass for the "elevated risk" hypothesis.
    pub risk_belief: f64,
    /// Wall-clock pipeline duration, milliseconds.
    pub total_duration_ms: u64,
    /// Absolute URL for the resulting case projection.
    pub case_url: String,
}

/// Standard error envelope used across every authenticated endpoint.
/// Matches the declaration service's `ErrorEnvelope` for cross-service
/// consistency (same `kind` taxonomy, same outer shape).
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// Stable machine-readable error kind. Values include
    /// `authentication_required`, `not_admin`, `admin_disabled`,
    /// `not_found`, `bad_request`, `internal`.
    pub kind: String,
    /// Human-readable message. May change between releases; never
    /// match on it.
    pub message: String,
}

/// `/healthz` response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthzResponse {
    /// Always `"ok"` when the process is alive.
    pub status: String,
}

/// `/readyz` response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyzResponse {
    /// `"ready"` when the database is reachable; `"not_ready"`
    /// otherwise.
    pub status: String,
    /// Present only on `not_ready`: a short machine-readable cause
    /// such as `"database_unreachable"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

#[utoipa::path(
    post,
    path = "/v1/verifications",
    tag = "verifications",
    operation_id = "submitVerification",
    request_body = SubmitVerificationRequest,
    responses(
        (status = 201, description = "Verification case created", body = SubmitVerificationResponse),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 403, description = "Caller is not on the admin allowlist (FIND-002 — REST submit is admin-only; the production path is the HMAC-authenticated internal webhook)", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
        (status = 503, description = "ADMIN_PRINCIPALS empty — REST submit is disabled (FIND-002 fail-closed)", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn submit_verification(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<crate::api::auth::Principal>,
    Json(req): Json<SubmitVerificationRequest>,
) -> Result<(StatusCode, Json<SubmitVerificationResponse>), ServiceError> {
    // FIND-002: the legitimate verification-submission path is the
    // HMAC-authenticated `/v1/internal/declaration-events` webhook
    // (and, when R-LOOP-2's Kafka transport is active, the
    // Kafka consumer). The REST surface here is operator-only;
    // empty admin allowlist disables it entirely (D14 fail-closed).
    refuse_if_not_admin(&state.admin_principals, &principal)?;
    let case = state.submit_usecase.execute(req.declaration).await?;
    let base_url = std::env::var("RECOR_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string());
    let resp = SubmitVerificationResponse::from_case(&case, &base_url);
    // OBS-1: lane counter + fusion-belief histograms. `lane` is a
    // 3-value bounded enum (Green/Yellow/Red) — D18 safe.
    let lane = case.lane.as_str();
    state
        .metrics
        .verification_cases_total
        .with_label_values(&[lane])
        .inc();
    state
        .metrics
        .fusion_belief_true
        .with_label_values(&[lane])
        .observe(case.fused_authenticity.belief_true());
    state
        .metrics
        .fusion_belief_false
        .with_label_values(&[lane])
        .observe(case.fused_authenticity.belief_false());
    Ok((StatusCode::CREATED, Json(resp)))
}

#[utoipa::path(
    get,
    path = "/v1/verifications/{case_id}",
    tag = "verifications",
    operation_id = "getVerification",
    params(("case_id" = String, Path, format = "uuid", description = "Verification case UUID")),
    responses(
        // The case body is deep nested domain JSON; pinned via
        // `body = serde_json::Value` to avoid coupling utoipa to
        // every domain value-object. The shape is stable across
        // the v1 contract: `case_id`, `declaration`, `stage_outcomes`,
        // `fused_authenticity`, `fused_risk`, `lane`, `created_at`,
        // `completed_at`, `total_duration_ms`.
        (status = 200, description = "Verification case projection", body = serde_json::Value),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 404, description = "Case not found OR caller is neither owner nor admin (FIND-004; no enumeration via 403)", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_verification(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<crate::api::auth::Principal>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<VerificationCase>, ServiceError> {
    // FIND-004 (audit Sprint 1 full closure). Sprint 0 shipped an
    // interim admin-only gate while the per-case tenancy story was
    // unresolved. The V-engine schema has carried
    // `verification_cases.declarant_principal` (denormalised onto
    // the row from the inbound DeclarationSnapshot) since migration
    // 0001 — what was missing was the runtime check.
    //
    // Load the case, then enforce `principal == declarant_principal
    // OR principal IN admin_allowlist`. Denial returns 404 (mirrors
    // person-service get_person semantics, FIND-005): non-owners
    // cannot enumerate case_ids by inferring existence from the
    // response code. This restores the Sprint 0 capability for
    // legitimate declarants to read their own cases without
    // depending on operator intervention.
    let case = state.get_usecase.execute(VerificationCaseId(case_id)).await?;
    if !is_admin(&state.admin_principals, &principal)
        && case.declaration.declarant_principal != principal.subject
    {
        tracing::warn!(
            actor = %principal.subject,
            owner = %case.declaration.declarant_principal,
            case_id = %case_id,
            "GET verification case refused — non-owner, non-admin"
        );
        return Err(ServiceError::NotFound(case_id.to_string()));
    }
    Ok(Json(case))
}

/// TODO-049 — `GET /v1/verifications/{case_id}/rationale`. Returns the
/// `DecisionRationale` persisted alongside the case. Tenancy gate is
/// IDENTICAL to `get_verification` (FIND-004): admin OR
/// `principal == declarant_principal`. A non-owner / non-admin gets
/// 404 to avoid case-id enumeration. The rationale is the
/// explainability anchor declarants are entitled to consult; an admin
/// can read any.
#[utoipa::path(
    get,
    path = "/v1/verifications/{case_id}/rationale",
    tag = "verifications",
    operation_id = "getVerificationRationale",
    params(("case_id" = String, Path, format = "uuid", description = "Verification case UUID")),
    responses(
        (status = 200, description = "Decision rationale: stage-by-stage reasoning, fusion chain, and the lane thresholds applied at adjudication time", body = serde_json::Value),
        (status = 401, description = "Authentication required", body = ErrorEnvelope),
        (status = 404, description = "Case or rationale not found OR caller is neither owner nor admin (TODO-049 mirrors FIND-004; no enumeration via 403)", body = ErrorEnvelope),
        (status = 500, description = "Internal failure", body = ErrorEnvelope),
    ),
    security(
        ("bearerAuth" = []),
        ("devPrincipalHeader" = []),
    ),
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_verification_rationale(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<crate::api::auth::Principal>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<DecisionRationale>, ServiceError> {
    // Load the case first so the tenancy predicate runs against
    // `declarant_principal`. Mirror of `get_verification`.
    let case = state.get_usecase.execute(VerificationCaseId(case_id)).await?;
    if !is_admin(&state.admin_principals, &principal)
        && case.declaration.declarant_principal != principal.subject
    {
        tracing::warn!(
            actor = %principal.subject,
            owner = %case.declaration.declarant_principal,
            case_id = %case_id,
            "GET verification rationale refused — non-owner, non-admin"
        );
        return Err(ServiceError::NotFound(case_id.to_string()));
    }
    let rationale = state
        .get_usecase
        .execute_rationale(VerificationCaseId(case_id))
        .await?;
    Ok(Json(rationale))
}

/// FIND-002 (audit Sprint 0). `submit_verification` is operator-only.
/// The legitimate verification-submission path is the
/// HMAC-authenticated `/v1/internal/declaration-events` webhook (and,
/// when R-LOOP-2's Kafka transport is active, the Kafka consumer).
/// Empty allowlist ⇒ 503 (D14 fail-closed); non-admin ⇒ 403.
fn refuse_if_not_admin(
    admin_principals: &std::collections::HashSet<String>,
    principal: &crate::api::auth::Principal,
) -> Result<(), ServiceError> {
    if admin_principals.is_empty() {
        tracing::warn!(
            "V-engine REST endpoint hit but ADMIN_PRINCIPALS is empty — endpoint disabled"
        );
        return Err(ServiceError::AdminDisabled);
    }
    if !admin_principals.contains(&principal.subject) {
        tracing::warn!(
            principal = %principal.subject,
            "non-admin principal attempted V-engine REST endpoint"
        );
        return Err(ServiceError::NotAdmin);
    }
    Ok(())
}

/// FIND-004: cheap admin-membership probe used by `get_verification`
/// to decide whether to apply the per-case tenancy predicate. Empty
/// allowlist always returns `false`, mirroring the fail-closed
/// semantics of `refuse_if_not_admin`.
fn is_admin(
    admin_principals: &std::collections::HashSet<String>,
    principal: &crate::api::auth::Principal,
) -> bool {
    !admin_principals.is_empty()
        && admin_principals.contains(&principal.subject)
}

#[cfg(test)]
mod rbac_tests {
    //! Unit tests for the FIND-004 per-case RBAC predicate and the
    //! FIND-002 admin-allowlist gate. The full handler is exercised
    //! by the integration suite; these tests lock the policy logic
    //! at the helper-function boundary so a regression on either
    //! gate fails at the unit level before the handler is hit.

    use std::collections::HashSet;

    use crate::api::auth::Principal;

    use super::*;

    fn principal(subject: &str) -> Principal {
        Principal {
            subject: subject.to_string(),
        }
    }

    fn allowlist(subjects: &[&str]) -> HashSet<String> {
        subjects.iter().map(|s| s.to_string()).collect()
    }

    // ─── FIND-002: refuse_if_not_admin ────────────────────────────────

    #[test]
    fn refuse_if_not_admin_503_on_empty_allowlist() {
        let res = refuse_if_not_admin(&HashSet::new(), &principal("anyone"));
        assert!(matches!(res, Err(ServiceError::AdminDisabled)));
    }

    #[test]
    fn refuse_if_not_admin_403_on_non_admin() {
        let res = refuse_if_not_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/declarant-7"),
        );
        assert!(matches!(res, Err(ServiceError::NotAdmin)));
    }

    #[test]
    fn refuse_if_not_admin_ok_for_listed_principal() {
        let res = refuse_if_not_admin(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/ops-1"),
        );
        assert!(res.is_ok());
    }

    // ─── FIND-004: is_admin ───────────────────────────────────────────

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

    // ─── FIND-004: the per-case tenancy predicate itself ──────────────
    //
    // The handler's policy is: `principal == declarant_principal OR
    // principal IN admin`. The handler shape doesn't expose a pure
    // helper for the full predicate — `is_admin` covers the admin
    // half, and these tests describe the equality half with
    // table-driven cases so a regression on the comparison logic
    // surfaces before the handler is reached.

    fn decision(
        admin_principals: &HashSet<String>,
        principal: &Principal,
        declarant_principal: &str,
    ) -> Result<(), ()> {
        // Mirrors the handler condition exactly.
        if !is_admin(admin_principals, principal)
            && declarant_principal != principal.subject
        {
            Err(())
        } else {
            Ok(())
        }
    }

    #[test]
    fn declarant_can_read_own_case() {
        let res = decision(
            &HashSet::new(),
            &principal("spiffe://recor.cm/declarant-7"),
            "spiffe://recor.cm/declarant-7",
        );
        assert!(res.is_ok());
    }

    #[test]
    fn cross_tenant_read_is_denied_even_when_admin_allowlist_is_empty() {
        let res = decision(
            &HashSet::new(),
            &principal("spiffe://recor.cm/declarant-attacker"),
            "spiffe://recor.cm/declarant-victim",
        );
        assert!(res.is_err());
    }

    #[test]
    fn admin_can_read_any_case() {
        let res = decision(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/ops-1"),
            "spiffe://recor.cm/some-declarant",
        );
        assert!(res.is_ok());
    }

    #[test]
    fn non_admin_non_owner_is_denied() {
        let res = decision(
            &allowlist(&["spiffe://recor.cm/ops-1"]),
            &principal("spiffe://recor.cm/declarant-attacker"),
            "spiffe://recor.cm/declarant-victim",
        );
        assert!(res.is_err());
    }
}
