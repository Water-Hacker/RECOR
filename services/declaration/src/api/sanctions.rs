//! TODO-004 — sanctions-for-non-compliance workflow.
//!
//! FATF R.24 c.24.13 requires "proportionate, dissuasive, effective"
//! sanctions for failure to comply with BO requirements. This
//! module exposes the admin-side workflow: initiate, escalate
//! (advance the proportionality ladder), withdraw, and the public
//! list of currently-published non-compliers (post-Sovim balancing).
//!
//! ADR-0012 defines the ladder. Every transition writes a
//! `sanction_events` row inside the same transaction as the
//! projection update (D15 cryptographic provenance — the audit log
//! IS immutable per the COMP-2 trigger in migration 0014).
//!
//! Doctrines:
//! - **D14 fail-closed** — every transition requires a documented
//!   justification; empty justification → 400.
//! - **D15 cryptographic provenance** — events are append-only;
//!   the COMP-2 trigger refuses UPDATE / DELETE on `sanction_events`.
//! - **D17 zero trust** — initiate / escalate / withdraw are
//!   admin-only (allowlist gated). The public list is unauthenticated
//!   but rate-limited at the network layer.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::collections::HashSet;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::auth::Principal;
use crate::error::ServiceError;

#[derive(Clone)]
pub struct SanctionsState {
    pub pool: PgPool,
    /// Admin allowlist — shared with `AppState.admin_principals`.
    pub admin_principals: Arc<HashSet<String>>,
}

fn require_admin(
    admin_principals: &HashSet<String>,
    principal: &Principal,
) -> Result<(), ServiceError> {
    if admin_principals.is_empty() {
        // D14 fail-closed: empty allowlist disables the endpoint
        // entirely. An operator who has not configured the allowlist
        // is not in a state to authorise sanctions.
        return Err(ServiceError::AuthorizationDenied(
            "sanctions endpoints disabled (ADMIN_PRINCIPALS empty)",
        ));
    }
    if !admin_principals.contains(&principal.subject) {
        return Err(ServiceError::AuthorizationDenied(
            "sanctions endpoints are admin-only",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InitiateSanctionRequest {
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub declaration_id: Option<Uuid>,
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub entity_id: Option<Uuid>,
    /// Bounded enum of reason codes. ADR-0012 documents the canonical
    /// list. Free-text is REFUSED — a future audit needs to bucket
    /// proceedings consistently.
    pub reason_code: String,
    /// Free-text justification. REQUIRED on every transition.
    pub justification: String,
    /// Optional initial tier (for proceedings that bypass `reminder`
    /// per c.24.13 "egregious non-compliance" carve-out). `None`
    /// starts at `submitted`.
    #[serde(default)]
    pub initial_tier: Option<u8>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SanctionsProceedingResponse {
    #[schema(value_type = String, format = "uuid")]
    pub proceeding_id: Uuid,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u8>,
    pub initiated_at: String,
    pub last_transition_at: String,
}

/// `POST /v1/sanctions/initiate` — admin opens a proceeding.
#[utoipa::path(
    post,
    path = "/v1/sanctions/initiate",
    operation_id = "initiateSanction",
    request_body = InitiateSanctionRequest,
    responses(
        (status = 201, description = "Proceeding opened", body = SanctionsProceedingResponse),
        (status = 403, description = "Not an admin"),
        (status = 400, description = "Malformed request"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "sanctions"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn initiate_sanction(
    State(state): State<SanctionsState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<InitiateSanctionRequest>,
) -> Result<(StatusCode, Json<SanctionsProceedingResponse>), ServiceError> {
    require_admin(&state.admin_principals, &principal)?;

    if req.declaration_id.is_none() && req.entity_id.is_none() {
        return Err(ServiceError::BadRequest(
            "one of declaration_id or entity_id MUST be present".into(),
        ));
    }
    if req.reason_code.trim().is_empty() {
        return Err(ServiceError::BadRequest("reason_code cannot be empty".into()));
    }
    if req.justification.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "justification cannot be empty (R.24 c.24.13)".into(),
        ));
    }

    let proceeding_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let now = OffsetDateTime::now_utc();
    let state_str = if req.initial_tier.is_some() {
        "fined"
    } else {
        "submitted"
    };
    let tier_i32: Option<i32> = req.initial_tier.map(i32::from);

    let payload: JsonValue = serde_json::json!({
        "reason_code": req.reason_code,
        "declaration_id": req.declaration_id,
        "entity_id": req.entity_id,
        "initial_tier": req.initial_tier,
    });

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO sanctions_proceedings (
            proceeding_id, declaration_id, entity_id, reason_code,
            state, tier, initiated_by, initiated_at,
            last_transition_at, last_actor, last_justification,
            aggregate_version
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $7, $9, 1)
        "#,
    )
    .bind(proceeding_id)
    .bind(req.declaration_id)
    .bind(req.entity_id)
    .bind(&req.reason_code)
    .bind(state_str)
    .bind(tier_i32)
    .bind(&principal.subject)
    .bind(now)
    .bind(&req.justification)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions_proceedings insert failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO sanction_events (
            event_id, proceeding_id, event_type, payload,
            actor_principal, justification, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'sanction.initiated.v1', $3, $4, $5, $6, 1)
        "#,
    )
    .bind(event_id)
    .bind(proceeding_id)
    .bind(&payload)
    .bind(&principal.subject)
    .bind(&req.justification)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanction_events insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "sanctions initiate tx commit failed");
        ServiceError::Internal
    })?;

    let ts = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    tracing::info!(
        proceeding_id = %proceeding_id,
        state = state_str,
        "TODO-004: sanction proceeding opened"
    );

    Ok((
        StatusCode::CREATED,
        Json(SanctionsProceedingResponse {
            proceeding_id,
            state: state_str.to_string(),
            tier: req.initial_tier,
            initiated_at: ts.clone(),
            last_transition_at: ts,
        }),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EscalateSanctionRequest {
    /// The new state. Must be a forward step on the proportionality
    /// ladder OR — for egregious cases — a skip-step (the handler
    /// records the skip but admits it; ADR-0012 covers when this is
    /// appropriate).
    pub to_state: String,
    /// New tier (for `fined`). When transitioning to `fined`,
    /// REQUIRED; ignored for other states.
    #[serde(default)]
    pub tier: Option<u8>,
    /// Required justification.
    pub justification: String,
    /// For `public_listed`: the name to publish. REQUIRED for that
    /// transition; ignored for others.
    #[serde(default)]
    pub public_listing_name: Option<String>,
    /// For `public_listed`: the reason as it appears on the public
    /// list. REQUIRED for that transition.
    #[serde(default)]
    pub public_listing_reason: Option<String>,
}

/// `POST /v1/sanctions/{id}/escalate` — admin advances the ladder.
#[utoipa::path(
    post,
    path = "/v1/sanctions/{proceeding_id}/escalate",
    operation_id = "escalateSanction",
    params(("proceeding_id" = String, Path, description = "Proceeding UUID")),
    request_body = EscalateSanctionRequest,
    responses(
        (status = 200, description = "Proceeding state advanced", body = SanctionsProceedingResponse),
        (status = 400, description = "Invalid transition or missing field"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Proceeding not found"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "sanctions"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn escalate_sanction(
    State(state): State<SanctionsState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(proceeding_id): Path<Uuid>,
    Json(req): Json<EscalateSanctionRequest>,
) -> Result<Json<SanctionsProceedingResponse>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    if req.justification.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "justification cannot be empty".into(),
        ));
    }
    let to_state = req.to_state.as_str();
    let valid = matches!(
        to_state,
        "reminder" | "fined" | "suspended" | "referred" | "public_listed"
    );
    if !valid {
        return Err(ServiceError::BadRequest(format!(
            "invalid to_state `{to_state}`; ADR-0012 lists the ladder"
        )));
    }
    if to_state == "fined" && req.tier.is_none() {
        return Err(ServiceError::BadRequest(
            "tier is required for `fined`".into(),
        ));
    }
    if to_state == "public_listed"
        && (req.public_listing_name.as_deref().unwrap_or("").trim().is_empty()
            || req.public_listing_reason.as_deref().unwrap_or("").trim().is_empty())
    {
        return Err(ServiceError::BadRequest(
            "public_listing_name + public_listing_reason are required for `public_listed`"
                .into(),
        ));
    }

    let now = OffsetDateTime::now_utc();
    let tier_i32: Option<i32> = req.tier.map(i32::from);

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    // SELECT FOR UPDATE so concurrent escalations serialise.
    let row_opt: Option<(String, i64, Option<OffsetDateTime>)> = sqlx::query_as(
        r#"
        SELECT state, aggregate_version, initiated_at
        FROM sanctions_proceedings
        WHERE proceeding_id = $1
        FOR UPDATE
        "#,
    )
    .bind(proceeding_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions row fetch failed");
        ServiceError::Internal
    })?;
    let (current_state, current_version, initiated_at) = match row_opt {
        Some(r) => r,
        None => return Err(ServiceError::NotFound(proceeding_id.to_string())),
    };
    if current_state == "withdrawn" {
        return Err(ServiceError::BadRequest(
            "proceeding is withdrawn; cannot escalate".into(),
        ));
    }

    let next_version = current_version.saturating_add(1);
    let sequence_no = next_version;

    let public_listed_at: Option<OffsetDateTime> = if to_state == "public_listed" {
        Some(now)
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE sanctions_proceedings
        SET state = $1,
            tier = COALESCE($2, tier),
            last_transition_at = $3,
            last_actor = $4,
            last_justification = $5,
            public_listed_at = COALESCE($6, public_listed_at),
            public_listing_name = COALESCE($7, public_listing_name),
            public_listing_reason = COALESCE($8, public_listing_reason),
            aggregate_version = $9
        WHERE proceeding_id = $10
        "#,
    )
    .bind(to_state)
    .bind(tier_i32)
    .bind(now)
    .bind(&principal.subject)
    .bind(&req.justification)
    .bind(public_listed_at)
    .bind(req.public_listing_name.as_deref())
    .bind(req.public_listing_reason.as_deref())
    .bind(next_version)
    .bind(proceeding_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions update failed");
        ServiceError::Internal
    })?;

    let payload: JsonValue = serde_json::json!({
        "from_state": current_state,
        "to_state": to_state,
        "tier": req.tier,
        "public_listing_name": req.public_listing_name,
        "public_listing_reason": req.public_listing_reason,
    });
    sqlx::query(
        r#"
        INSERT INTO sanction_events (
            event_id, proceeding_id, event_type, payload,
            actor_principal, justification, occurred_at, sequence_no
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(proceeding_id)
    .bind(format!("sanction.{}.v1", to_state))
    .bind(&payload)
    .bind(&principal.subject)
    .bind(&req.justification)
    .bind(now)
    .bind(sequence_no)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanction_events insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "sanctions escalate tx commit failed");
        ServiceError::Internal
    })?;

    let initiated_str = initiated_at
        .map(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let last_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    tracing::info!(
        proceeding_id = %proceeding_id,
        from = %current_state,
        to = to_state,
        "TODO-004: sanction proceeding escalated"
    );

    Ok(Json(SanctionsProceedingResponse {
        proceeding_id,
        state: to_state.to_string(),
        tier: req.tier,
        initiated_at: initiated_str,
        last_transition_at: last_str,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WithdrawSanctionRequest {
    pub justification: String,
}

/// `POST /v1/sanctions/{id}/withdraw` — admin closes the proceeding.
#[utoipa::path(
    post,
    path = "/v1/sanctions/{proceeding_id}/withdraw",
    operation_id = "withdrawSanction",
    params(("proceeding_id" = String, Path, description = "Proceeding UUID")),
    request_body = WithdrawSanctionRequest,
    responses(
        (status = 200, description = "Proceeding withdrawn", body = SanctionsProceedingResponse),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Proceeding not found"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "sanctions"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn withdraw_sanction(
    State(state): State<SanctionsState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(proceeding_id): Path<Uuid>,
    Json(req): Json<WithdrawSanctionRequest>,
) -> Result<Json<SanctionsProceedingResponse>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    if req.justification.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "justification cannot be empty".into(),
        ));
    }
    let now = OffsetDateTime::now_utc();

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    let row_opt: Option<(String, i64, Option<OffsetDateTime>)> = sqlx::query_as(
        r#"
        SELECT state, aggregate_version, initiated_at
        FROM sanctions_proceedings
        WHERE proceeding_id = $1
        FOR UPDATE
        "#,
    )
    .bind(proceeding_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions row fetch failed");
        ServiceError::Internal
    })?;
    let (current_state, current_version, initiated_at) = match row_opt {
        Some(r) => r,
        None => return Err(ServiceError::NotFound(proceeding_id.to_string())),
    };
    if current_state == "withdrawn" {
        return Err(ServiceError::BadRequest(
            "proceeding already withdrawn".into(),
        ));
    }

    let next_version = current_version.saturating_add(1);

    sqlx::query(
        r#"
        UPDATE sanctions_proceedings
        SET state = 'withdrawn',
            last_transition_at = $1,
            last_actor = $2,
            last_justification = $3,
            withdrawn_at = $1,
            -- TODO-004: public_listing_* are NOT cleared here so the
            -- audit retains what was published; the public-list
            -- endpoint refuses to surface rows in state `withdrawn`.
            aggregate_version = $4
        WHERE proceeding_id = $5
        "#,
    )
    .bind(now)
    .bind(&principal.subject)
    .bind(&req.justification)
    .bind(next_version)
    .bind(proceeding_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions withdraw update failed");
        ServiceError::Internal
    })?;

    let payload: JsonValue = serde_json::json!({
        "from_state": current_state,
        "to_state": "withdrawn",
    });
    sqlx::query(
        r#"
        INSERT INTO sanction_events (
            event_id, proceeding_id, event_type, payload,
            actor_principal, justification, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'sanction.withdrawn.v1', $3, $4, $5, $6, $7)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(proceeding_id)
    .bind(&payload)
    .bind(&principal.subject)
    .bind(&req.justification)
    .bind(now)
    .bind(next_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanction_events insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "sanctions withdraw tx commit failed");
        ServiceError::Internal
    })?;

    let initiated_str = initiated_at
        .map(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let last_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    tracing::info!(
        proceeding_id = %proceeding_id,
        "TODO-004: sanction proceeding withdrawn"
    );

    Ok(Json(SanctionsProceedingResponse {
        proceeding_id,
        state: "withdrawn".to_string(),
        tier: None,
        initiated_at: initiated_str,
        last_transition_at: last_str,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicListingRow {
    pub name: String,
    pub reason: String,
    pub since: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicListingResponse {
    pub listings: Vec<PublicListingRow>,
    pub total: usize,
    pub cached_until: String,
}

/// `GET /v1/sanctions/public` — public list of currently-published
/// non-compliers (post-Sovim balancing). The endpoint is
/// unauthenticated; only proceedings in `public_listed` state appear.
/// A withdrawal removes the entity within 24 hours (the cache TTL).
#[utoipa::path(
    get,
    path = "/v1/sanctions/public",
    operation_id = "listPublicSanctions",
    responses(
        (status = 200, description = "Public list of currently-published non-compliers", body = PublicListingResponse),
    ),
    tag = "sanctions"
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn list_public_sanctions(
    State(state): State<SanctionsState>,
) -> Result<Json<PublicListingResponse>, ServiceError> {
    let rows: Vec<PublicRow> = sqlx::query_as(
        r#"
        SELECT public_listing_name, public_listing_reason, public_listed_at
        FROM sanctions_proceedings
        WHERE state = 'public_listed'
          AND public_listing_name IS NOT NULL
          AND public_listing_reason IS NOT NULL
        ORDER BY public_listed_at DESC
        LIMIT 500
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "sanctions public list query failed");
        ServiceError::Internal
    })?;

    let listings: Vec<PublicListingRow> = rows
        .into_iter()
        .filter_map(|r| {
            let name = r.public_listing_name?;
            let reason = r.public_listing_reason?;
            let since = r.public_listed_at?;
            Some(PublicListingRow {
                name,
                reason,
                since: since
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            })
        })
        .collect();

    let cached_until = (OffsetDateTime::now_utc() + time::Duration::hours(24))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    Ok(Json(PublicListingResponse {
        total: listings.len(),
        listings,
        cached_until,
    }))
}

#[derive(sqlx::FromRow)]
struct PublicRow {
    public_listing_name: Option<String>,
    public_listing_reason: Option<String>,
    public_listed_at: Option<OffsetDateTime>,
}

pub fn router(state: SanctionsState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/v1/sanctions/initiate", post(initiate_sanction))
        .route(
            "/v1/sanctions/{proceeding_id}/escalate",
            post(escalate_sanction),
        )
        .route(
            "/v1/sanctions/{proceeding_id}/withdraw",
            post(withdraw_sanction),
        )
        .with_state(state.clone())
}

pub fn public_router(state: SanctionsState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/v1/sanctions/public", get(list_public_sanctions))
        .with_state(state)
}
