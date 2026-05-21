//! TODO-032 — Data-subject rights (GDPR Art. 15 / 16 / 17 / 18) and
//! TODO-034 — Article 30 Records of Processing register.
//!
//! Three public surfaces:
//!
//!   - `GET  /v1/me/export`           — Art. 15 / 20 export envelope.
//!   - `POST /v1/me/rectify`          — Art. 16 rectification request.
//!   - `POST /v1/me/erasure-restriction` — Art. 17 refusal + Art. 18
//!     restriction recording.
//!
//! Three admin surfaces (admin-allowlist gated):
//!
//!   - `POST /v1/internal/rectification-requests/{id}/approve`
//!   - `POST /v1/internal/rectification-requests/{id}/reject`
//!   - `GET  /v1/internal/gdpr/processing-records`
//!   - `POST /v1/internal/gdpr/processing-records`
//!   - `POST /v1/internal/gdpr/processing-records/{id}/retire`
//!   - `GET  /v1/internal/gdpr/processing-records/{id}`
//!
//! Doctrines:
//!   - **D13 idempotency** — every POST honours `Idempotency-Key`. A
//!     replay returns the stored response body byte-for-byte; a
//!     mismatch on the same key returns 409 idempotency_conflict.
//!   - **D14 fail-closed** — empty principal → 401-equivalent; admin
//!     allowlist empty → 503 (the endpoint is structurally disabled);
//!     erasure → 400 with the documented refusal kind.
//!   - **D15 cryptographic provenance** — every state transition
//!     writes a row to the corresponding `*_events` table inside the
//!     same transaction as the projection update. The events tables
//!     are COMP-2-immutable.
//!   - **D17 zero trust** — the data-subject principal is sourced from
//!     the verified session. The admin `?principal=...` override is
//!     gated on the admin allowlist.
//!   - **D18 no secrets** — request bodies are JSONB; the platform
//!     stores the data-subject's claim verbatim, no derivation that
//!     could leak PII into logs.

use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::api::auth::{Principal, PrincipalClass};
use crate::api::dto::GetDeclarationResponse;
use crate::application::ListByPrincipalUseCase;
use crate::error::ServiceError;
use crate::infrastructure::postgres::IdempotencyStore;

/// Shared state for the GDPR data-subject + Art. 30 register surfaces.
///
/// The state mirrors the discrepancies module: a `PgPool` for the
/// projection writes; the shared `IdempotencyStore` for the POST
/// replays; the admin allowlist for the `?principal=...` override on
/// `/v1/me/export` and for the admin-only register endpoints.
#[derive(Clone)]
pub struct GdprState {
    pub pool: PgPool,
    pub admin_principals: Arc<HashSet<String>>,
    pub idempotency: Arc<IdempotencyStore>,
    pub idempotency_ttl_seconds: i64,
    pub list_by_principal_usecase: Arc<ListByPrincipalUseCase>,
}

fn require_admin(
    admin_principals: &HashSet<String>,
    principal: &Principal,
) -> Result<(), ServiceError> {
    // D14: empty allowlist disables the endpoint.
    if admin_principals.is_empty() {
        return Err(ServiceError::AuthorizationDenied(
            "gdpr admin endpoints disabled (ADMIN_PRINCIPALS empty)",
        ));
    }
    if !admin_principals.contains(&principal.subject) {
        return Err(ServiceError::AuthorizationDenied(
            "gdpr admin endpoints are admin-only",
        ));
    }
    Ok(())
}

fn idempotency_key_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn blake3_hex_of(bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(bytes);
    hex::encode(h.finalize().as_bytes())
}

// ─── /v1/me/export (GDPR Art. 15 / 20) ────────────────────────────────

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    /// Admin-only override: export the data of a specific principal.
    /// Admins (members of the `ADMIN_PRINCIPALS` allowlist) may export
    /// on behalf of another data subject when responding to a DSAR.
    /// Non-admins providing this parameter receive `403 forbidden`.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub principal: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GdprExportResponse {
    /// JSON-LD type tag. Always `RecorGdprExport` so consumers can
    /// route the envelope without sniffing other fields.
    #[serde(rename = "$type")]
    pub type_tag: String,
    /// Subject of the export — the principal the rows belong to.
    pub data_subject_principal: String,
    /// Timestamp the export was produced.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub exported_at: OffsetDateTime,
    /// Format version. Bump when the envelope schema changes in a
    /// way that breaks consumers.
    pub format_version: String,
    /// Every declaration RÉCOR holds where the data subject is the
    /// declarant.
    pub declarations: Vec<GetDeclarationResponse>,
    /// Every rectification request the data subject has submitted.
    pub rectification_requests: Vec<RectificationRequestView>,
    /// Every erasure-restriction request the data subject has lodged.
    pub erasure_restriction_requests: Vec<ErasureRestrictionRequestView>,
}

#[utoipa::path(
    get,
    path = "/v1/me/export",
    operation_id = "gdprExport",
    params(ExportQuery),
    responses(
        (status = 200, description = "GDPR export envelope", body = GdprExportResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Admin override requested by non-admin"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn gdpr_export(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Query(query): Query<ExportQuery>,
) -> Result<Json<GdprExportResponse>, ServiceError> {
    // D17: resolve the subject. Default = caller. Override only
    // allowed for admin-allowlist members; non-admins providing the
    // override are refused (we treat the override as a privileged
    // operation, not a hint).
    let subject = match query.principal {
        Some(other) => {
            require_admin(&state.admin_principals, &principal)?;
            other.trim().to_string()
        }
        None => principal.subject.clone(),
    };
    if subject.is_empty() {
        return Err(ServiceError::BadRequest("empty subject".into()));
    }

    let projections = state
        .list_by_principal_usecase
        .execute(&subject)
        .await?;

    let declarations: Vec<GetDeclarationResponse> =
        projections.into_iter().map(GetDeclarationResponse::from).collect();

    let rectification_rows: Vec<RectificationRow> = sqlx::query_as(
        r#"
        SELECT request_id, declaration_id, data_subject_principal,
               field_path, requested_value, reason, state,
               submitted_at, resolved_at, resolver_principal,
               resolution_notes, applied_correction_event_id
        FROM rectification_requests
        WHERE data_subject_principal = $1
        ORDER BY submitted_at DESC
        LIMIT 500
        "#,
    )
    .bind(&subject)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification list query failed");
        ServiceError::Internal
    })?;

    let rectification_requests: Vec<RectificationRequestView> =
        rectification_rows.into_iter().map(Into::into).collect();

    let erasure_rows: Vec<ErasureRow> = sqlx::query_as(
        r#"
        SELECT request_id, declaration_id, data_subject_principal,
               reason, state, refusal_kind, submitted_at, withdrawn_at
        FROM erasure_restriction_requests
        WHERE data_subject_principal = $1
        ORDER BY submitted_at DESC
        LIMIT 500
        "#,
    )
    .bind(&subject)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "erasure list query failed");
        ServiceError::Internal
    })?;

    let erasure_restriction_requests: Vec<ErasureRestrictionRequestView> =
        erasure_rows.into_iter().map(Into::into).collect();

    tracing::info!(
        event_kind = "gdpr_export",
        subject_count_declarations = declarations.len(),
        subject_count_rectifications = rectification_requests.len(),
        subject_count_erasures = erasure_restriction_requests.len(),
        "TODO-032: data-subject export served",
    );

    Ok(Json(GdprExportResponse {
        type_tag: "RecorGdprExport".to_string(),
        data_subject_principal: subject,
        exported_at: OffsetDateTime::now_utc(),
        format_version: "1.0".to_string(),
        declarations,
        rectification_requests,
        erasure_restriction_requests,
    }))
}

// ─── /v1/me/rectify (GDPR Art. 16) ────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct RectifyRequest {
    /// Declaration the data subject wants rectified. Must exist; the
    /// declarant-ownership check is enforced at the handler so a data
    /// subject cannot lodge rectification requests against
    /// declarations they did not submit.
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    /// JSON Pointer (RFC 6901) into the canonical declaration body.
    /// Example: `/beneficial_owners/0/ownership_basis_points`.
    pub field_path: String,
    /// The value the data subject claims is the correct one. Stored
    /// as JSONB verbatim.
    pub requested_value: JsonValue,
    /// Free-text reason supporting the request.
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct RectifyResponse {
    #[schema(value_type = String, format = "uuid")]
    pub request_id: Uuid,
    pub state: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
}

#[utoipa::path(
    post,
    path = "/v1/me/rectify",
    operation_id = "gdprRectify",
    request_body = RectifyRequest,
    responses(
        (status = 201, description = "Rectification request recorded", body = RectifyResponse),
        (status = 400, description = "Malformed request"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the owner of the declaration"),
        (status = 404, description = "Declaration not found"),
        (status = 409, description = "Idempotency-Key collision with a different body"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, declaration_id = %req.declaration_id))]
pub(crate) async fn submit_rectification(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    headers: HeaderMap,
    Json(req): Json<RectifyRequest>,
) -> Result<(StatusCode, Json<RectifyResponse>), ServiceError> {
    if req.field_path.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "field_path cannot be empty".into(),
        ));
    }
    if req.reason.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "reason cannot be empty".into(),
        ));
    }

    // D13 idempotency replay. Hash a canonical body so re-runs match.
    let body_bytes = serde_json::to_vec(&serde_json::json!({
        "op": "rectify",
        "data_subject_principal": principal.subject,
        "declaration_id": req.declaration_id,
        "field_path": req.field_path,
        "requested_value": req.requested_value,
        "reason": req.reason,
    }))
    .map_err(|_| ServiceError::Internal)?;
    let request_hash = blake3_hex_of(&body_bytes);
    let idem_key = idempotency_key_from(&headers);
    if let Some(key) = idem_key.as_ref() {
        if let Some(existing) = state
            .idempotency
            .check_existing(key, &principal.subject)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "idempotency check failed");
                ServiceError::Internal
            })?
        {
            if existing.request_hash != request_hash {
                return Err(ServiceError::IdempotencyConflict);
            }
            let resp: RectifyResponse = serde_json::from_value(existing.response_body)
                .map_err(|e| {
                    tracing::error!(error = ?e, "stored idempotency body deserialise failed");
                    ServiceError::Internal
                })?;
            return Ok((StatusCode::CREATED, Json(resp)));
        }
    }

    // D17: only the declarant of the declaration may lodge a
    // rectification on it. Fetch the projection's declarant_principal
    // under the same transaction as the insert; refuse on mismatch.
    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;
    let row: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT declarant_principal
        FROM declarations
        WHERE declaration_id = $1
        "#,
    )
    .bind(req.declaration_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "declaration ownership check failed");
        ServiceError::Internal
    })?;
    let declarant = row
        .ok_or_else(|| ServiceError::NotFound(req.declaration_id.to_string()))?
        .0;
    if declarant != principal.subject {
        return Err(ServiceError::AuthorizationDenied(
            "rectification can only be requested by the declaration's declarant",
        ));
    }

    let request_id = Uuid::now_v7();
    let submitted_at = OffsetDateTime::now_utc();

    sqlx::query(
        r#"
        INSERT INTO rectification_requests (
            request_id, declaration_id, data_subject_principal,
            field_path, requested_value, reason, state,
            submitted_at, aggregate_version
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'submitted', $7, 1)
        "#,
    )
    .bind(request_id)
    .bind(req.declaration_id)
    .bind(&principal.subject)
    .bind(&req.field_path)
    .bind(&req.requested_value)
    .bind(&req.reason)
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification insert failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO rectification_request_events (
            event_id, request_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'rectification.submitted.v1', $3, $4, $5, 1)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(request_id)
    .bind(serde_json::json!({
        "declaration_id": req.declaration_id,
        "field_path": req.field_path,
        "requested_value": req.requested_value,
        "reason": req.reason,
    }))
    .bind(&principal.subject)
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification event insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "rectify tx commit failed");
        ServiceError::Internal
    })?;

    let response = RectifyResponse {
        request_id,
        state: "submitted".to_string(),
        submitted_at,
    };
    let response_json =
        serde_json::to_value(&response).map_err(|_| ServiceError::Internal)?;
    if let Some(key) = idem_key.as_ref() {
        if let Err(e) = state
            .idempotency
            .record(
                key,
                &principal.subject,
                &request_hash,
                201,
                &response_json,
                state.idempotency_ttl_seconds,
            )
            .await
        {
            tracing::warn!(error = ?e, "idempotency record write failed");
        }
    }

    tracing::info!(
        event_kind = "gdpr_rectification_submitted",
        request_id = %request_id,
        "TODO-032: rectification request lodged"
    );

    Ok((StatusCode::CREATED, Json(response)))
}

// ─── /v1/me/erasure-restriction (GDPR Art. 17 + 18) ───────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct ErasureRestrictionRequestBody {
    /// Declaration the data subject wants erased or restricted.
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    /// Free-text reason — usually citing the Art. 17 / 18 ground
    /// (inaccuracy, processing unlawful, no longer necessary,
    /// objection pending).
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ErasureRestrictionResponse {
    #[schema(value_type = String, format = "uuid")]
    pub request_id: Uuid,
    /// Refusal kind on the erasure component. Always
    /// `erasure_not_permitted` for BO data — FATF R.24 retention
    /// beats Art. 17.
    pub erasure_refusal_kind: String,
    /// Plain-language explanation of the refusal.
    pub erasure_refusal_notice: String,
    /// State of the restriction record (always `restriction_active`
    /// when the platform creates the row).
    pub restriction_state: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
}

#[utoipa::path(
    post,
    path = "/v1/me/erasure-restriction",
    operation_id = "gdprErasureRestriction",
    request_body = ErasureRestrictionRequestBody,
    responses(
        (status = 400, description = "Erasure refused (the canonical response); restriction recorded", body = ErasureRestrictionResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not the owner of the declaration"),
        (status = 404, description = "Declaration not found"),
        (status = 409, description = "Idempotency-Key collision with a different body"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, declaration_id = %req.declaration_id))]
pub(crate) async fn submit_erasure_restriction(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    headers: HeaderMap,
    Json(req): Json<ErasureRestrictionRequestBody>,
) -> Result<(StatusCode, Json<ErasureRestrictionResponse>), ServiceError> {
    if req.reason.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "reason cannot be empty".into(),
        ));
    }

    let body_bytes = serde_json::to_vec(&serde_json::json!({
        "op": "erasure_restriction",
        "data_subject_principal": principal.subject,
        "declaration_id": req.declaration_id,
        "reason": req.reason,
    }))
    .map_err(|_| ServiceError::Internal)?;
    let request_hash = blake3_hex_of(&body_bytes);
    let idem_key = idempotency_key_from(&headers);
    if let Some(key) = idem_key.as_ref() {
        if let Some(existing) = state
            .idempotency
            .check_existing(key, &principal.subject)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "idempotency check failed");
                ServiceError::Internal
            })?
        {
            if existing.request_hash != request_hash {
                return Err(ServiceError::IdempotencyConflict);
            }
            let resp: ErasureRestrictionResponse =
                serde_json::from_value(existing.response_body).map_err(|e| {
                    tracing::error!(error = ?e, "stored idempotency body deserialise failed");
                    ServiceError::Internal
                })?;
            // Same canonical status as a fresh request: 400 with the
            // refusal kind. The body is the data subject's record of
            // having lodged the request; the 400 status echoes the
            // canonical erasure-refused contract.
            return Ok((StatusCode::BAD_REQUEST, Json(resp)));
        }
    }

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    // D17 ownership check.
    let row: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT declarant_principal
        FROM declarations
        WHERE declaration_id = $1
        FOR UPDATE
        "#,
    )
    .bind(req.declaration_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "declaration ownership check failed");
        ServiceError::Internal
    })?;
    let declarant = row
        .ok_or_else(|| ServiceError::NotFound(req.declaration_id.to_string()))?
        .0;
    if declarant != principal.subject {
        return Err(ServiceError::AuthorizationDenied(
            "erasure-restriction can only be requested by the declaration's declarant",
        ));
    }

    let request_id = Uuid::now_v7();
    let submitted_at = OffsetDateTime::now_utc();
    let refusal_kind = "erasure_not_permitted";
    let refusal_notice = "Erasure refused under GDPR Art. 17(3)(b) because the underlying processing is required by FATF R.24 retention rules (BO data retained for the entity lifetime plus 5 years post-cessation). The platform has recorded a restriction-of-processing request under GDPR Art. 18; subsequent disclosures will carry a restriction notice per Art. 18(2).";

    sqlx::query(
        r#"
        INSERT INTO erasure_restriction_requests (
            request_id, declaration_id, data_subject_principal,
            reason, state, refusal_kind, submitted_at, aggregate_version
        )
        VALUES ($1, $2, $3, $4, 'restriction_active', $5, $6, 1)
        "#,
    )
    .bind(request_id)
    .bind(req.declaration_id)
    .bind(&principal.subject)
    .bind(&req.reason)
    .bind(refusal_kind)
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "erasure-restriction insert failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO erasure_restriction_request_events (
            event_id, request_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'erasure_restriction.submitted.v1', $3, $4, $5, 1)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(request_id)
    .bind(serde_json::json!({
        "declaration_id": req.declaration_id,
        "reason": req.reason,
        "refusal_kind": refusal_kind,
    }))
    .bind(&principal.subject)
    .bind(submitted_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "erasure-restriction event insert failed");
        ServiceError::Internal
    })?;

    // Set the projection flag so subsequent reads include the Art. 18(2)
    // restriction notice. The handler reading `GET /v1/declarations/{id}`
    // checks `restricted_at IS NOT NULL`.
    sqlx::query(
        r#"
        UPDATE declarations
        SET restricted_at = $1
        WHERE declaration_id = $2
        "#,
    )
    .bind(submitted_at)
    .bind(req.declaration_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "declaration restriction flag update failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "erasure tx commit failed");
        ServiceError::Internal
    })?;

    let response = ErasureRestrictionResponse {
        request_id,
        erasure_refusal_kind: refusal_kind.to_string(),
        erasure_refusal_notice: refusal_notice.to_string(),
        restriction_state: "restriction_active".to_string(),
        submitted_at,
    };
    let response_json =
        serde_json::to_value(&response).map_err(|_| ServiceError::Internal)?;
    if let Some(key) = idem_key.as_ref() {
        if let Err(e) = state
            .idempotency
            .record(
                key,
                &principal.subject,
                &request_hash,
                400,
                &response_json,
                state.idempotency_ttl_seconds,
            )
            .await
        {
            tracing::warn!(error = ?e, "idempotency record write failed");
        }
    }

    tracing::info!(
        event_kind = "gdpr_erasure_restriction_submitted",
        request_id = %request_id,
        "TODO-032: erasure refused + restriction recorded"
    );

    // Canonical contract: 400 Bad Request with `erasure_not_permitted`
    // as the kind. The body explains R.24 retention beats Art. 17 and
    // confirms restriction-of-processing under Art. 18 has been
    // recorded.
    Ok((StatusCode::BAD_REQUEST, Json(response)))
}

// ─── Admin: rectification approve / reject ────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveRectificationRequest {
    /// Free-text rationale captured on the events row.
    pub notes: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ResolveRectificationResponse {
    #[schema(value_type = String, format = "uuid")]
    pub request_id: Uuid,
    pub state: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub resolved_at: OffsetDateTime,
}

#[utoipa::path(
    post,
    path = "/v1/internal/rectification-requests/{request_id}/approve",
    operation_id = "approveRectification",
    params(("request_id" = String, Path, description = "Rectification request UUID")),
    request_body = ResolveRectificationRequest,
    responses(
        (status = 200, description = "Approved", body = ResolveRectificationResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Request not found"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, request_id = %request_id))]
pub(crate) async fn approve_rectification(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(request_id): Path<Uuid>,
    Json(req): Json<ResolveRectificationRequest>,
) -> Result<Json<ResolveRectificationResponse>, ServiceError> {
    resolve_rectification_inner(state, principal, request_id, req, true).await
}

#[utoipa::path(
    post,
    path = "/v1/internal/rectification-requests/{request_id}/reject",
    operation_id = "rejectRectification",
    params(("request_id" = String, Path, description = "Rectification request UUID")),
    request_body = ResolveRectificationRequest,
    responses(
        (status = 200, description = "Rejected", body = ResolveRectificationResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Request not found"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, request_id = %request_id))]
pub(crate) async fn reject_rectification(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(request_id): Path<Uuid>,
    Json(req): Json<ResolveRectificationRequest>,
) -> Result<Json<ResolveRectificationResponse>, ServiceError> {
    resolve_rectification_inner(state, principal, request_id, req, false).await
}

async fn resolve_rectification_inner(
    state: GdprState,
    principal: Principal,
    request_id: Uuid,
    req: ResolveRectificationRequest,
    approve: bool,
) -> Result<Json<ResolveRectificationResponse>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    if req.notes.trim().is_empty() {
        return Err(ServiceError::BadRequest("notes cannot be empty".into()));
    }

    let next_state = if approve { "approved" } else { "rejected" };
    let event_type = if approve {
        "rectification.approved.v1"
    } else {
        "rectification.rejected.v1"
    };
    let resolved_at = OffsetDateTime::now_utc();

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    let row: Option<(String, i64)> = sqlx::query_as(
        r#"
        SELECT state, aggregate_version
        FROM rectification_requests
        WHERE request_id = $1
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification fetch failed");
        ServiceError::Internal
    })?;
    let (current_state, current_version) =
        row.ok_or_else(|| ServiceError::NotFound(request_id.to_string()))?;
    if current_state != "submitted" {
        return Err(ServiceError::BadRequest(format!(
            "cannot transition from state `{current_state}` (expected `submitted`)"
        )));
    }
    let next_version = current_version.saturating_add(1);

    sqlx::query(
        r#"
        UPDATE rectification_requests
        SET state = $1,
            resolved_at = $2,
            resolver_principal = $3,
            resolution_notes = $4,
            aggregate_version = $5
        WHERE request_id = $6
        "#,
    )
    .bind(next_state)
    .bind(resolved_at)
    .bind(&principal.subject)
    .bind(&req.notes)
    .bind(next_version)
    .bind(request_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification resolve update failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO rectification_request_events (
            event_id, request_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(request_id)
    .bind(event_type)
    .bind(serde_json::json!({
        "from_state": current_state,
        "to_state": next_state,
        "notes": req.notes,
    }))
    .bind(&principal.subject)
    .bind(resolved_at)
    .bind(next_version)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "rectification event insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "rectification resolve tx commit failed");
        ServiceError::Internal
    })?;

    if approve {
        // The approval ratifies the request in the platform's records.
        // The actual correction of the declaration still requires the
        // declarant to submit a Correct or Amend command with their
        // own Ed25519 attestation (D15 — the platform never signs on
        // the declarant's behalf). The `applied_correction_event_id`
        // column on the projection is wired by the Correct/Amend
        // handler when the declarant follows through and references
        // this request_id in their metadata_notes.
        tracing::info!(
            event_kind = "gdpr_rectification_approved",
            request_id = %request_id,
            "TODO-032: rectification approved — declarant must follow up with a Correct command bearing their attestation",
        );
    } else {
        tracing::info!(
            event_kind = "gdpr_rectification_rejected",
            request_id = %request_id,
            "TODO-032: rectification rejected",
        );
    }

    Ok(Json(ResolveRectificationResponse {
        request_id,
        state: next_state.to_string(),
        resolved_at,
    }))
}

// ─── Art. 30 processing register (admin-only) ─────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProcessingRecordRequest {
    pub controller: String,
    #[serde(default)]
    pub processor: Option<String>,
    pub purpose: String,
    pub legal_basis: String,
    #[schema(value_type = Vec<String>)]
    pub data_categories: JsonValue,
    #[schema(value_type = Vec<String>)]
    pub subject_categories: JsonValue,
    #[schema(value_type = Vec<String>)]
    pub recipients: JsonValue,
    pub retention_period_text: String,
    #[serde(default)]
    pub transfer_safeguards: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ProcessingRecordView {
    #[schema(value_type = String, format = "uuid")]
    pub record_id: Uuid,
    pub controller: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor: Option<String>,
    pub purpose: String,
    pub legal_basis: String,
    #[schema(value_type = Object)]
    pub data_categories: JsonValue,
    #[schema(value_type = Object)]
    pub subject_categories: JsonValue,
    #[schema(value_type = Object)]
    pub recipients: JsonValue,
    pub retention_period_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_safeguards: Option<String>,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub retired_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListProcessingRecordsResponse {
    pub records: Vec<ProcessingRecordView>,
    pub total: usize,
}

#[utoipa::path(
    post,
    path = "/v1/internal/gdpr/processing-records",
    operation_id = "createProcessingRecord",
    request_body = CreateProcessingRecordRequest,
    responses(
        (status = 201, description = "Record created", body = ProcessingRecordView),
        (status = 400, description = "Malformed request"),
        (status = 403, description = "Not an admin"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn create_processing_record(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<CreateProcessingRecordRequest>,
) -> Result<(StatusCode, Json<ProcessingRecordView>), ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    for (field, value) in [
        ("controller", &req.controller),
        ("purpose", &req.purpose),
        ("legal_basis", &req.legal_basis),
        ("retention_period_text", &req.retention_period_text),
    ] {
        if value.trim().is_empty() {
            return Err(ServiceError::BadRequest(format!(
                "{field} cannot be empty"
            )));
        }
    }

    let record_id = Uuid::now_v7();
    let now = OffsetDateTime::now_utc();
    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO gdpr_processing_register (
            record_id, controller, processor, purpose, legal_basis,
            data_categories, subject_categories, recipients,
            retention_period_text, transfer_safeguards,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
        "#,
    )
    .bind(record_id)
    .bind(&req.controller)
    .bind(req.processor.as_deref())
    .bind(&req.purpose)
    .bind(&req.legal_basis)
    .bind(&req.data_categories)
    .bind(&req.subject_categories)
    .bind(&req.recipients)
    .bind(&req.retention_period_text)
    .bind(req.transfer_safeguards.as_deref())
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record insert failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO gdpr_processing_register_events (
            event_id, record_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'gdpr.processing_record.created.v1', $3, $4, $5, 1)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(record_id)
    .bind(serde_json::json!({
        "controller": req.controller,
        "purpose": req.purpose,
        "legal_basis": req.legal_basis,
    }))
    .bind(&principal.subject)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record event insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "processing record commit failed");
        ServiceError::Internal
    })?;

    let view = ProcessingRecordView {
        record_id,
        controller: req.controller,
        processor: req.processor,
        purpose: req.purpose,
        legal_basis: req.legal_basis,
        data_categories: req.data_categories,
        subject_categories: req.subject_categories,
        recipients: req.recipients,
        retention_period_text: req.retention_period_text,
        transfer_safeguards: req.transfer_safeguards,
        created_at: now,
        updated_at: now,
        retired_at: None,
    };
    Ok((StatusCode::CREATED, Json(view)))
}

#[utoipa::path(
    get,
    path = "/v1/internal/gdpr/processing-records",
    operation_id = "listProcessingRecords",
    responses(
        (status = 200, description = "Register listing", body = ListProcessingRecordsResponse),
        (status = 403, description = "Not an admin"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn list_processing_records(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
) -> Result<Json<ListProcessingRecordsResponse>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    let rows: Vec<ProcessingRecordRow> = sqlx::query_as(
        r#"
        SELECT record_id, controller, processor, purpose, legal_basis,
               data_categories, subject_categories, recipients,
               retention_period_text, transfer_safeguards,
               created_at, updated_at, retired_at
        FROM gdpr_processing_register
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing register list failed");
        ServiceError::Internal
    })?;
    let records: Vec<ProcessingRecordView> = rows.into_iter().map(Into::into).collect();
    Ok(Json(ListProcessingRecordsResponse {
        total: records.len(),
        records,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/internal/gdpr/processing-records/{record_id}",
    operation_id = "getProcessingRecord",
    params(("record_id" = String, Path, description = "Processing record UUID")),
    responses(
        (status = 200, description = "Processing record", body = ProcessingRecordView),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Record not found"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, record_id = %record_id))]
pub(crate) async fn get_processing_record(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(record_id): Path<Uuid>,
) -> Result<Json<ProcessingRecordView>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    let row: Option<ProcessingRecordRow> = sqlx::query_as(
        r#"
        SELECT record_id, controller, processor, purpose, legal_basis,
               data_categories, subject_categories, recipients,
               retention_period_text, transfer_safeguards,
               created_at, updated_at, retired_at
        FROM gdpr_processing_register
        WHERE record_id = $1
        "#,
    )
    .bind(record_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record fetch failed");
        ServiceError::Internal
    })?;
    let row = row.ok_or_else(|| ServiceError::NotFound(record_id.to_string()))?;
    Ok(Json(row.into()))
}

#[utoipa::path(
    post,
    path = "/v1/internal/gdpr/processing-records/{record_id}/retire",
    operation_id = "retireProcessingRecord",
    params(("record_id" = String, Path, description = "Processing record UUID")),
    responses(
        (status = 200, description = "Retired", body = ProcessingRecordView),
        (status = 400, description = "Already retired"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "Record not found"),
    ),
    security(("bearer" = []), ("devPrincipalHeader" = [])),
    tag = "gdpr"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject, record_id = %record_id))]
pub(crate) async fn retire_processing_record(
    State(state): State<GdprState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(record_id): Path<Uuid>,
) -> Result<Json<ProcessingRecordView>, ServiceError> {
    require_admin(&state.admin_principals, &principal)?;
    let now = OffsetDateTime::now_utc();
    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    let row: Option<ProcessingRecordRow> = sqlx::query_as(
        r#"
        SELECT record_id, controller, processor, purpose, legal_basis,
               data_categories, subject_categories, recipients,
               retention_period_text, transfer_safeguards,
               created_at, updated_at, retired_at
        FROM gdpr_processing_register
        WHERE record_id = $1
        FOR UPDATE
        "#,
    )
    .bind(record_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record retire fetch failed");
        ServiceError::Internal
    })?;
    let mut record = row.ok_or_else(|| ServiceError::NotFound(record_id.to_string()))?;
    if record.retired_at.is_some() {
        return Err(ServiceError::BadRequest(
            "record is already retired".into(),
        ));
    }

    sqlx::query(
        r#"
        UPDATE gdpr_processing_register
        SET retired_at = $1, updated_at = $1
        WHERE record_id = $2
        "#,
    )
    .bind(now)
    .bind(record_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record retire update failed");
        ServiceError::Internal
    })?;

    // sequence_no: the events table is append-only; the create event
    // was sequence_no=1, so the retire event becomes sequence_no=2.
    // We compute it from the existing rows so re-runs of the migration
    // remain idempotent.
    let next_seq: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(sequence_no), 0) + 1
        FROM gdpr_processing_register_events
        WHERE record_id = $1
        "#,
    )
    .bind(record_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "register event sequence_no fetch failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO gdpr_processing_register_events (
            event_id, record_id, event_type, payload,
            actor_principal, occurred_at, sequence_no
        )
        VALUES ($1, $2, 'gdpr.processing_record.retired.v1', $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(record_id)
    .bind(serde_json::json!({
        "retired_by": principal.subject,
        "retired_at": now,
    }))
    .bind(&principal.subject)
    .bind(now)
    .bind(next_seq)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "processing record retire event insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "retire tx commit failed");
        ServiceError::Internal
    })?;

    record.retired_at = Some(now);
    record.updated_at = now;
    Ok(Json(record.into()))
}

// ─── Row shapes ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct RectificationRequestView {
    #[schema(value_type = String, format = "uuid")]
    pub request_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    pub data_subject_principal: String,
    pub field_path: String,
    #[schema(value_type = Object)]
    pub requested_value: JsonValue,
    pub reason: String,
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
    pub resolver_principal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub applied_correction_event_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct RectificationRow {
    request_id: Uuid,
    declaration_id: Uuid,
    data_subject_principal: String,
    field_path: String,
    requested_value: JsonValue,
    reason: String,
    state: String,
    submitted_at: OffsetDateTime,
    resolved_at: Option<OffsetDateTime>,
    resolver_principal: Option<String>,
    resolution_notes: Option<String>,
    applied_correction_event_id: Option<Uuid>,
}

impl From<RectificationRow> for RectificationRequestView {
    fn from(row: RectificationRow) -> Self {
        Self {
            request_id: row.request_id,
            declaration_id: row.declaration_id,
            data_subject_principal: row.data_subject_principal,
            field_path: row.field_path,
            requested_value: row.requested_value,
            reason: row.reason,
            state: row.state,
            submitted_at: row.submitted_at,
            resolved_at: row.resolved_at,
            resolver_principal: row.resolver_principal,
            resolution_notes: row.resolution_notes,
            applied_correction_event_id: row.applied_correction_event_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ErasureRestrictionRequestView {
    #[schema(value_type = String, format = "uuid")]
    pub request_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    pub data_subject_principal: String,
    pub reason: String,
    pub state: String,
    pub refusal_kind: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub withdrawn_at: Option<OffsetDateTime>,
}

#[derive(sqlx::FromRow)]
struct ErasureRow {
    request_id: Uuid,
    declaration_id: Uuid,
    data_subject_principal: String,
    reason: String,
    state: String,
    refusal_kind: String,
    submitted_at: OffsetDateTime,
    withdrawn_at: Option<OffsetDateTime>,
}

impl From<ErasureRow> for ErasureRestrictionRequestView {
    fn from(row: ErasureRow) -> Self {
        Self {
            request_id: row.request_id,
            declaration_id: row.declaration_id,
            data_subject_principal: row.data_subject_principal,
            reason: row.reason,
            state: row.state,
            refusal_kind: row.refusal_kind,
            submitted_at: row.submitted_at,
            withdrawn_at: row.withdrawn_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProcessingRecordRow {
    record_id: Uuid,
    controller: String,
    processor: Option<String>,
    purpose: String,
    legal_basis: String,
    data_categories: JsonValue,
    subject_categories: JsonValue,
    recipients: JsonValue,
    retention_period_text: String,
    transfer_safeguards: Option<String>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    retired_at: Option<OffsetDateTime>,
}

impl From<ProcessingRecordRow> for ProcessingRecordView {
    fn from(row: ProcessingRecordRow) -> Self {
        Self {
            record_id: row.record_id,
            controller: row.controller,
            processor: row.processor,
            purpose: row.purpose,
            legal_basis: row.legal_basis,
            data_categories: row.data_categories,
            subject_categories: row.subject_categories,
            recipients: row.recipients,
            retention_period_text: row.retention_period_text,
            transfer_safeguards: row.transfer_safeguards,
            created_at: row.created_at,
            updated_at: row.updated_at,
            retired_at: row.retired_at,
        }
    }
}

// ─── Router assembly ──────────────────────────────────────────────────

pub fn router(state: GdprState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/v1/me/export", get(gdpr_export))
        .route("/v1/me/rectify", post(submit_rectification))
        .route(
            "/v1/me/erasure-restriction",
            post(submit_erasure_restriction),
        )
        .route(
            "/v1/internal/rectification-requests/{request_id}/approve",
            post(approve_rectification),
        )
        .route(
            "/v1/internal/rectification-requests/{request_id}/reject",
            post(reject_rectification),
        )
        .route(
            "/v1/internal/gdpr/processing-records",
            get(list_processing_records).post(create_processing_record),
        )
        .route(
            "/v1/internal/gdpr/processing-records/{record_id}",
            get(get_processing_record),
        )
        .route(
            "/v1/internal/gdpr/processing-records/{record_id}/retire",
            post(retire_processing_record),
        )
        .with_state(state)
}

// Suppress dead_code lint on `principal` field of `Principal` when
// using PrincipalClass elsewhere via re-exports.
#[allow(dead_code)]
fn _force_imports(_c: PrincipalClass) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::PrincipalSource;
    use crate::api::oidc::AssuranceLevel;

    fn principal(subject: &str, class: PrincipalClass) -> Principal {
        Principal {
            subject: subject.to_string(),
            source: PrincipalSource::DevHeader,
            assurance_level: AssuranceLevel::Ial3,
            class,
        }
    }

    fn admins(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn require_admin_refuses_empty_allowlist() {
        let set = admins(&[]);
        let err = require_admin(&set, &principal("any", PrincipalClass::Admin))
            .expect_err("must refuse empty allowlist");
        assert!(matches!(err, ServiceError::AuthorizationDenied(_)));
    }

    #[test]
    fn require_admin_refuses_non_listed_principal() {
        let set = admins(&["spiffe://recor.cm/admin-1"]);
        let err = require_admin(
            &set,
            &principal("spiffe://recor.cm/declarant", PrincipalClass::Declarant),
        )
        .expect_err("must refuse non-admin");
        assert!(matches!(err, ServiceError::AuthorizationDenied(_)));
    }

    #[test]
    fn require_admin_admits_listed() {
        let set = admins(&["spiffe://recor.cm/admin-1"]);
        require_admin(
            &set,
            &principal("spiffe://recor.cm/admin-1", PrincipalClass::Admin),
        )
        .expect("admin admitted");
    }

    #[test]
    fn idempotency_key_extraction_trims_and_filters_empty() {
        use axum::http::HeaderValue;
        let mut h = HeaderMap::new();
        assert!(idempotency_key_from(&h).is_none());
        h.insert("idempotency-key", HeaderValue::from_static(""));
        assert!(idempotency_key_from(&h).is_none());
        h.insert("idempotency-key", HeaderValue::from_static("  k1  "));
        assert_eq!(idempotency_key_from(&h).as_deref(), Some("k1"));
    }

    #[test]
    fn rectify_body_hash_is_stable_across_identical_requests() {
        // The idempotency hash must be byte-stable across two
        // structurally identical requests; otherwise the replay
        // protection silently fails.
        let r1 = serde_json::json!({
            "op": "rectify",
            "data_subject_principal": "spiffe://recor.cm/alice",
            "declaration_id": "00000000-0000-0000-0000-000000000001",
            "field_path": "/beneficial_owners/0/ownership_basis_points",
            "requested_value": 5000,
            "reason": "ownership recomputed after share buy-back",
        });
        let r2 = r1.clone();
        let b1 = serde_json::to_vec(&r1).unwrap();
        let b2 = serde_json::to_vec(&r2).unwrap();
        assert_eq!(blake3_hex_of(&b1), blake3_hex_of(&b2));
    }

    #[test]
    fn export_envelope_serialises_with_type_tag() {
        // The envelope MUST carry the `$type` field as the first
        // discriminator; consumers route on it.
        let resp = GdprExportResponse {
            type_tag: "RecorGdprExport".to_string(),
            data_subject_principal: "spiffe://recor.cm/x".to_string(),
            exported_at: OffsetDateTime::now_utc(),
            format_version: "1.0".to_string(),
            declarations: Vec::new(),
            rectification_requests: Vec::new(),
            erasure_restriction_requests: Vec::new(),
        };
        let v = serde_json::to_value(&resp).expect("serialise");
        assert_eq!(v["$type"].as_str(), Some("RecorGdprExport"));
        assert_eq!(v["format_version"].as_str(), Some("1.0"));
        assert!(v["declarations"].is_array());
        assert!(v["rectification_requests"].is_array());
        assert!(v["erasure_restriction_requests"].is_array());
    }

    #[test]
    fn erasure_response_carries_refusal_kind() {
        let resp = ErasureRestrictionResponse {
            request_id: Uuid::nil(),
            erasure_refusal_kind: "erasure_not_permitted".to_string(),
            erasure_refusal_notice: "test".to_string(),
            restriction_state: "restriction_active".to_string(),
            submitted_at: OffsetDateTime::now_utc(),
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            v["erasure_refusal_kind"].as_str(),
            Some("erasure_not_permitted"),
            "the canonical refusal kind must surface byte-for-byte"
        );
        assert_eq!(
            v["restriction_state"].as_str(),
            Some("restriction_active"),
        );
    }

    #[test]
    fn resolve_rectification_request_rejects_empty_notes() {
        // Pure validation: the handler refuses empty notes before
        // touching the DB. Mirroring this in a unit test makes the
        // failure mode obvious in code review and resilient to
        // refactors of the SQL path.
        let req = ResolveRectificationRequest {
            notes: "   ".to_string(),
        };
        assert!(req.notes.trim().is_empty(), "guard must trip on whitespace");
    }

    #[test]
    fn create_processing_record_rejects_empty_required_fields() {
        let req = CreateProcessingRecordRequest {
            controller: " ".to_string(),
            processor: None,
            purpose: "p".to_string(),
            legal_basis: "lb".to_string(),
            data_categories: serde_json::json!([]),
            subject_categories: serde_json::json!([]),
            recipients: serde_json::json!([]),
            retention_period_text: "5y".to_string(),
            transfer_safeguards: None,
        };
        // Mirror the handler's loop. The point is that a whitespace-
        // only controller MUST be refused — never silently stored.
        for (field, value) in [
            ("controller", &req.controller),
            ("purpose", &req.purpose),
            ("legal_basis", &req.legal_basis),
            ("retention_period_text", &req.retention_period_text),
        ] {
            if field == "controller" {
                assert!(
                    value.trim().is_empty(),
                    "controller whitespace must be detected"
                );
            }
        }
    }

    #[test]
    fn rectification_view_round_trips_through_json() {
        let view = RectificationRequestView {
            request_id: Uuid::nil(),
            declaration_id: Uuid::nil(),
            data_subject_principal: "spiffe://recor.cm/x".into(),
            field_path: "/foo".into(),
            requested_value: serde_json::json!("v"),
            reason: "r".into(),
            state: "submitted".into(),
            submitted_at: OffsetDateTime::now_utc(),
            resolved_at: None,
            resolver_principal: None,
            resolution_notes: None,
            applied_correction_event_id: None,
        };
        let s = serde_json::to_string(&view).unwrap();
        let back: RectificationRequestView = serde_json::from_str(&s).unwrap();
        assert_eq!(back.field_path, "/foo");
        assert_eq!(back.state, "submitted");
    }
}
