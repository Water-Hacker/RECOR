//! REST API.
//
// TODO(R-VER-OPENAPI): wire utoipa-generated OpenAPI 3.1 spec for this
// service's public surface, mirroring DOC-1 (#70 — declaration). Same
// pattern: `#[utoipa::path(...)]` on every handler, `#[derive(ToSchema)]`
// on every DTO, build the document in `api::openapi`, mount
// `GET /openapi.json` + Scalar UI at `GET /docs`, commit the snapshot
// to `docs/openapi/verification-engine.json`, extend
// `tools/ci/check-openapi-drift.sh` to also assert that snapshot.

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
use crate::api::dlq::DlqAdminState;
use crate::api::oidc::OidcVerifier;
use crate::application::{GetVerificationUseCase, SubmitVerificationUseCase};
use crate::config::Config;
use crate::domain::{DeclarationSnapshot, VerificationCase, VerificationCaseId};
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

pub fn router(state: AppState, cfg: &Config) -> Router {
    let auth_state = AuthConfig {
        is_dev: state.is_dev,
        oidc: state.oidc.clone(),
        metrics: state.metrics.clone(),
    };

    let protected = Router::new()
        .route("/v1/verifications", post(submit_verification))
        .route("/v1/verifications/{case_id}", get(get_verification))
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

    // OBS-1: GET /metrics — Prometheus exposition. No auth; in-cluster
    // network only (see runbook). Mounted as a sibling router so the
    // metrics handler reads `State<Arc<Metrics>>` (the typed metrics
    // state) rather than the full AppState. The request-timing
    // middleware is NOT applied here so scrape traffic doesn't inflate
    // the latency histogram.
    let metrics_router: Router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state.metrics.clone());

    let app_routes = protected.merge(admin).merge(internal).merge(public);

    // Per-endpoint timing + counter middleware over the app routes
    // only (not /metrics).
    let metrics_state = state.metrics.clone();
    let app_routes = app_routes.layer(axum::middleware::from_fn_with_state(
        metrics_state,
        metrics_middleware,
    ));

    app_routes.merge(metrics_router).layer(
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

#[tracing::instrument(skip(state))]
async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let resp = (StatusCode::OK, Json(json!({"status": "ok"})));
    state
        .metrics
        .health_check_duration_seconds
        .with_label_values(&["healthz"])
        .observe(start.elapsed().as_secs_f64());
    resp
}

#[tracing::instrument(skip(state))]
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let probe = sqlx::query_scalar!(r#"SELECT 1 AS "probe!: i32""#).fetch_one(state.repository.pool());
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

#[tracing::instrument(skip(state))]
async fn get_verification(
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
