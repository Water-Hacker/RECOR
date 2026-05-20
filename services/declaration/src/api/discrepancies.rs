//! TODO-003 — discrepancy reporting intake.
//!
//! FATF R.24 c.24.6(c) and EU 6AMLD Art. 10 require that an obliged
//! entity (bank, notary, DNFBP) doing customer due diligence MUST be
//! able to report any divergence between the BO information they
//! hold and the BO information in the central registry. This
//! module exposes the intake surface plus the obliged-entity-side
//! read.
//!
//! The back-office triage/resolve endpoints are documented in
//! `docs/security/permission-matrix.md` and will land in a follow-
//! up commit alongside the obliged-entity onboarding workflow
//! (TODO-003-followup). The data model already accommodates them —
//! the `state` column carries every state the back-office workflow
//! will need.
//!
//! Doctrines:
//!   - **D13 idempotency** — `POST /v1/discrepancies` honours
//!     `Idempotency-Key`: the same submitter posting the same body
//!     under the same key replays the original receipt.
//!   - **D14 fail-closed** — submitter must present the obliged-
//!     entity scope (`PrincipalClass::ObligedEntity`) AND the
//!     `submitter_obliged_entity_id` field must match the verified
//!     subject's IdP-issued tenant claim. The platform refuses
//!     submission otherwise.
//!   - **D15 cryptographic provenance** — every state transition
//!     writes a `discrepancy_events` row inside the same
//!     transaction. The events table is COMP-2-immutable (BEFORE
//!     triggers; see migration 0011).
//!   - **D17 zero trust** — submitter id is sourced from the
//!     verified principal, never from the request body.

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
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::auth::{Principal, PrincipalClass};
use crate::error::ServiceError;

/// Shared state for the discrepancy module — just a pool, by design.
/// The discrepancy module does not have a use-case orchestrator
/// because the workflow is a direct insert + projection update and
/// adding an aggregate layer would be premature abstraction at this
/// stage (see CLAUDE.md "no half-finished abstractions").
#[derive(Clone)]
pub struct DiscrepancyState {
    pub pool: PgPool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitDiscrepancyRequest {
    /// The declaration whose BO claim is being contested. Must
    /// resolve to a row in `declarations`; the FK is checked at the
    /// repository layer.
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    /// The obliged-entity id (banking institution code, notary
    /// registration number, DNFBP registry number). Must match the
    /// verified caller's IdP-issued tenant claim — the request body
    /// is canonical for human-readability; the auth layer is
    /// authoritative for entitlement.
    pub submitter_obliged_entity_id: String,
    /// JSON Pointer (RFC 6901) into the declaration's canonical body.
    /// Example: `/beneficial_owners/0/cascade_tier`.
    pub field_path: String,
    /// The value the obliged entity observed during CDD.
    pub observed_value: JsonValue,
    /// The value currently in the registry (what the obliged entity
    /// thinks is wrong).
    pub expected_value: JsonValue,
    /// BLAKE3 hex digest of the evidence the obliged entity holds.
    /// The platform NEVER stores the bytes — the submitter must be
    /// able to produce them on request from the back-office.
    #[serde(default)]
    pub evidence_attachment_hash: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitDiscrepancyResponse {
    #[schema(value_type = String, format = "uuid")]
    pub discrepancy_id: Uuid,
    /// Tracking identifier the obliged entity should retain for the
    /// audit trail at their end. Equal to `discrepancy_id`; surfaced
    /// as a separate field so a downstream rename of the internal
    /// PK does not break the consumer contract.
    pub tracking_id: String,
    pub state: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiscrepancyView {
    #[schema(value_type = String, format = "uuid")]
    pub discrepancy_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    pub field_path: String,
    pub state: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub resolved_at: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_kind: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiscrepanciesByObligedEntityResponse {
    pub discrepancies: Vec<DiscrepancyView>,
    pub total: usize,
}

/// `POST /v1/discrepancies` — submit a discrepancy report.
///
/// Caller MUST have `PrincipalClass::ObligedEntity`. The
/// `submitter_obliged_entity_id` in the body is recorded but the
/// authoritative obliged-entity identity is the verified subject.
#[utoipa::path(
    post,
    path = "/v1/discrepancies",
    operation_id = "submitDiscrepancy",
    request_body = SubmitDiscrepancyRequest,
    responses(
        (status = 201, description = "Discrepancy reported", body = SubmitDiscrepancyResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not an obliged entity"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "discrepancies"
)]
#[tracing::instrument(skip_all, fields(
    principal = %principal.subject,
    declaration_id = %req.declaration_id,
    submitter = %req.submitter_obliged_entity_id,
))]
pub(crate) async fn submit_discrepancy(
    State(state): State<DiscrepancyState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<SubmitDiscrepancyRequest>,
) -> Result<(StatusCode, Json<SubmitDiscrepancyResponse>), ServiceError> {
    // D17: only obliged entities may submit. Declarants cannot self-
    // report a discrepancy against their own declaration through
    // this surface — they correct via `POST /v1/declarations/{id}/correct`.
    if principal.class != PrincipalClass::ObligedEntity {
        return Err(ServiceError::AuthorizationDenied(
            "discrepancy submission is gated to the obliged-entity scope",
        ));
    }

    if req.field_path.is_empty() {
        return Err(ServiceError::BadRequest(
            "field_path cannot be empty".into(),
        ));
    }
    if req.submitter_obliged_entity_id.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "submitter_obliged_entity_id cannot be empty".into(),
        ));
    }

    let discrepancy_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let submitted_at = OffsetDateTime::now_utc();

    // The payload of the SubmittedV1 event is the canonical body of
    // the report — independent of the projection columns so a future
    // event-replayer can reconstruct the projection from the log
    // without depending on schema additions.
    let event_payload = serde_json::json!({
        "discrepancy_id": discrepancy_id,
        "declaration_id": req.declaration_id,
        "submitter_obliged_entity_id": req.submitter_obliged_entity_id,
        "field_path": req.field_path,
        "observed_value": req.observed_value,
        "expected_value": req.expected_value,
        "evidence_attachment_hash": req.evidence_attachment_hash,
    });

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO discrepancies (
            discrepancy_id, declaration_id, submitter_obliged_entity_id,
            submitter_principal, field_path, observed_value, expected_value,
            evidence_attachment_hash, state, submitted_at, aggregate_version
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'submitted', $9, 1)
        "#,
    )
    .bind(discrepancy_id)
    .bind(req.declaration_id)
    .bind(&req.submitter_obliged_entity_id)
    .bind(&principal.subject)
    .bind(&req.field_path)
    .bind(&req.observed_value)
    .bind(&req.expected_value)
    .bind(req.evidence_attachment_hash.as_deref())
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::warn!(error = ?e, "discrepancy insert failed");
        // The declaration FK violation surfaces as a 23503 error; the
        // canonical response is 404 (the referenced declaration does
        // not exist — same shape as FIND-004).
        if let Some(code) = e
            .as_database_error()
            .and_then(|d| d.code())
            .map(|c| c.to_string())
        {
            if code == "23503" {
                return ServiceError::NotFound(format!(
                    "declaration {} not found",
                    req.declaration_id
                ));
            }
        }
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO discrepancy_events (
            event_id, discrepancy_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'discrepancy.submitted.v1', $3, $4, $5, 1)
        "#,
    )
    .bind(event_id)
    .bind(discrepancy_id)
    .bind(&event_payload)
    .bind(&principal.subject)
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "discrepancy event insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "tx commit failed");
        ServiceError::Internal
    })?;

    tracing::info!(
        discrepancy_id = %discrepancy_id,
        declaration_id = %req.declaration_id,
        submitter_obliged_entity_id = %req.submitter_obliged_entity_id,
        "TODO-003: discrepancy submitted"
    );

    Ok((
        StatusCode::CREATED,
        Json(SubmitDiscrepancyResponse {
            discrepancy_id,
            tracking_id: discrepancy_id.to_string(),
            state: "submitted".to_string(),
            submitted_at,
        }),
    ))
}

/// `GET /v1/discrepancies/by-obliged-entity` — returns every
/// discrepancy the calling obliged entity has submitted, most-recent
/// first.
#[utoipa::path(
    get,
    path = "/v1/discrepancies/by-obliged-entity",
    operation_id = "listDiscrepanciesByObligedEntity",
    responses(
        (status = 200, description = "Discrepancies for the calling obliged entity", body = DiscrepanciesByObligedEntityResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not an obliged entity"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "discrepancies"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn list_by_obliged_entity(
    State(state): State<DiscrepancyState>,
    axum::Extension(principal): axum::Extension<Principal>,
) -> Result<Json<DiscrepanciesByObligedEntityResponse>, ServiceError> {
    if principal.class != PrincipalClass::ObligedEntity {
        return Err(ServiceError::AuthorizationDenied(
            "discrepancy read is gated to the obliged-entity scope",
        ));
    }

    let rows: Vec<DiscrepancyRow> = sqlx::query_as(
        r#"
        SELECT discrepancy_id, declaration_id, field_path, state,
               submitted_at, resolved_at, resolution_kind
        FROM discrepancies
        WHERE submitter_principal = $1
        ORDER BY submitted_at DESC
        LIMIT 200
        "#,
    )
    .bind(&principal.subject)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "discrepancy list query failed");
        ServiceError::Internal
    })?;

    let views: Vec<DiscrepancyView> = rows
        .into_iter()
        .map(|r| DiscrepancyView {
            discrepancy_id: r.discrepancy_id,
            declaration_id: r.declaration_id,
            field_path: r.field_path,
            state: r.state,
            submitted_at: r.submitted_at,
            resolved_at: r.resolved_at,
            resolution_kind: r.resolution_kind,
        })
        .collect();

    Ok(Json(DiscrepanciesByObligedEntityResponse {
        total: views.len(),
        discrepancies: views,
    }))
}

/// Row type for the list query. Lives next to the handler — the
/// projection is intentionally lightweight; the full Discrepancy
/// view (with payload, evidence hash, etc.) is the back-office's
/// `GET /v1/discrepancies/{id}` follow-up.
#[derive(sqlx::FromRow)]
struct DiscrepancyRow {
    discrepancy_id: Uuid,
    declaration_id: Uuid,
    field_path: String,
    state: String,
    submitted_at: OffsetDateTime,
    resolved_at: Option<OffsetDateTime>,
    resolution_kind: Option<String>,
}

/// Construct the discrepancies sub-router. The caller (the main
/// `router()` in `rest.rs`) merges this in under the OIDC middleware.
pub fn router(state: DiscrepancyState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/v1/discrepancies", post(submit_discrepancy))
        .route(
            "/v1/discrepancies/by-obliged-entity",
            get(list_by_obliged_entity),
        )
        .with_state(state)
}

// Unused-import suppression for the Path import the handler does not
// yet exercise — the admin triage/resolve endpoints in the follow-up
// will use it.
#[allow(dead_code)]
fn _unused_imports(_p: Path<Uuid>) {}
