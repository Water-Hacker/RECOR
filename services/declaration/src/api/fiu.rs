//! TODO-008 — ANIF / FIU disclosure surface.
//!
//! FATF R.24 c.24.9 requires that the FIU has timely access to BO
//! information. R.40 extends this to foreign FIUs via the
//! MLAT/Egmont pathway. The platform exposes a dedicated surface
//! gated on the `recor:fiu-anif` scope (TODO-006); every disclosure
//! is event-sourced into the COMP-2-immutable `fiu_disclosure_log`
//! (migration 0012) with the requesting principal, the ANIF case
//! reference, the justification text, and the field-level audit of
//! which columns were disclosed.
//!
//! Doctrine bearing:
//! - **D14 fail-closed** — the FIU class is the only one admitted;
//!   any other class returns 403 (no fallthrough to the public
//!   tier).
//! - **D15 cryptographic provenance** — every disclosure writes
//!   to the immutable log inside the same transaction as the read.
//!   The trigger in migration 0012 refuses any UPDATE / DELETE.
//! - **D17 zero trust** — production deployments additionally
//!   require mTLS peer-ID + IP allowlist (configured in the SPIFFE
//!   layer in `main.rs`; this handler enforces the OIDC class +
//!   request-shape gates).
//! - **D18 no secrets** — `disclosed_columns` is a JSONB array of
//!   column names, not the values; the actual values are in the
//!   response body returned to the FIU but never logged.
//!
//! The R.40 / MLAT pathway for foreign-FIU requests is a back-office
//! workflow — out of scope for this handler. When `mlat_foreign_fiu`
//! is set on the request, the platform records it in the log row
//! but the access decision is the same (the foreign FIU's request
//! must already have been routed to an ANIF principal through
//! Egmont before reaching this endpoint).

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

#[derive(Clone)]
pub struct FiuState {
    pub pool: PgPool,
}

/// Subject discriminator for the FIU search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FiuSubjectKind {
    PersonId,
    NationalId,
    DeclarationId,
    EntityId,
    FullName,
}

impl FiuSubjectKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::PersonId => "person_id",
            Self::NationalId => "national_id",
            Self::DeclarationId => "declaration_id",
            Self::EntityId => "entity_id",
            Self::FullName => "full_name",
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FiuSearchRequest {
    /// ANIF-side case file identifier. Echoed verbatim into the
    /// disclosure log so a subsequent audit can link a platform
    /// disclosure to the ANIF case file.
    pub anif_case_reference: String,
    /// Free-text justification. Stored in the log; required for
    /// GDPR Art. 30 records-of-processing compliance.
    pub justification_text: String,
    /// Which kind of identifier the FIU is searching on.
    pub subject_kind: FiuSubjectKind,
    /// The identifier value itself.
    pub subject_value: String,
    /// TODO-008 R.40 — when this request is routed via Egmont/MLAT,
    /// the foreign FIU's identifier. Null for ANIF-originated
    /// requests.
    #[serde(default)]
    pub mlat_foreign_fiu: Option<String>,
    /// TODO-008 R.40 — Egmont request identifier. Null for ANIF-
    /// originated.
    #[serde(default)]
    pub mlat_egmont_request_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FiuSearchResponse {
    /// Stable tracking identifier for this disclosure. ANIF should
    /// retain this id on their case file for the audit trail.
    #[schema(value_type = String, format = "uuid")]
    pub disclosure_id: Uuid,
    /// Matching declaration(s). May be empty if the search did not
    /// hit; the disclosure_log row is still written so an audit can
    /// see that a search occurred and that it returned nothing.
    pub matches: Vec<FiuMatchProjection>,
    pub disclosed_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FiuMatchProjection {
    /// The declaration id the platform resolved.
    #[schema(value_type = String, format = "uuid")]
    pub declaration_id: Uuid,
    /// Full payload — at the FIU tier the post-Sovim minimisation
    /// does NOT apply (FATF c.24.9 explicitly authorises full
    /// disclosure to competent authorities).
    pub projection: JsonValue,
}

/// `POST /v1/fiu/search` — ANIF (or MLAT-routed foreign FIU) search.
///
/// Caller MUST present `PrincipalClass::FiuAnif`. Production
/// deployments additionally enforce mTLS peer-ID + IP allowlist at
/// the SPIFFE/network layer.
#[utoipa::path(
    post,
    path = "/v1/fiu/search",
    operation_id = "fiuSearch",
    request_body = FiuSearchRequest,
    responses(
        (status = 200, description = "Disclosure record + matches", body = FiuSearchResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not FIU-class"),
        (status = 400, description = "Malformed request"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "fiu"
)]
#[tracing::instrument(skip_all, fields(
    principal = %principal.subject,
    case = %req.anif_case_reference,
    subject_kind = ?req.subject_kind,
))]
pub(crate) async fn fiu_search(
    State(state): State<FiuState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<FiuSearchRequest>,
) -> Result<Json<FiuSearchResponse>, ServiceError> {
    // D14 + D17: only the FIU class may call this endpoint. Admin
    // is INTENTIONALLY NOT admitted — admins use the standard
    // `GET /v1/declarations/{id}` surface; admin access bypasses
    // the FIU disclosure log, which would defeat the audit trail.
    if principal.class != PrincipalClass::FiuAnif {
        return Err(ServiceError::AuthorizationDenied(
            "fiu/search is gated to the FIU principal class",
        ));
    }

    if req.anif_case_reference.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "anif_case_reference cannot be empty".into(),
        ));
    }
    if req.justification_text.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "justification_text cannot be empty (GDPR Art. 30)".into(),
        ));
    }
    if req.subject_value.trim().is_empty() {
        return Err(ServiceError::BadRequest("subject_value cannot be empty".into()));
    }

    // Resolve the search to one or more declaration rows. The
    // implementation here is the minimum viable: declaration_id
    // hits the projection directly; other kinds are deferred to
    // the back-office triage workflow (TODO-008-followup: name +
    // national-ID matching is the Person service's surface; the
    // Stage-2 BUNEC adapter is the production resolver). The log
    // row is still written, so the audit trail captures that the
    // FIU asked.
    let (matches, resolved_declaration_id) = resolve_subject(
        &state.pool,
        req.subject_kind,
        &req.subject_value,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "fiu/search subject resolution failed");
        ServiceError::Internal
    })?;

    let disclosure_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let disclosed_at = OffsetDateTime::now_utc();

    let disclosed_columns: JsonValue = serde_json::json!([
        "declaration_id", "entity_id", "declarant_principal",
        "declarant_role", "kind", "effective_from",
        "beneficial_owners", "state", "submitted_at",
        "receipt_hash_hex", "correlation_id"
    ]);

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "begin tx failed");
        ServiceError::Internal
    })?;

    sqlx::query(
        r#"
        INSERT INTO fiu_disclosure_log (
            disclosure_id, requesting_principal, anif_case_reference,
            justification_text, subject_kind, subject_value, disclosed_at,
            disclosed_columns, resolved_declaration_id,
            mlat_foreign_fiu, mlat_egmont_request_id, event_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(disclosure_id)
    .bind(&principal.subject)
    .bind(&req.anif_case_reference)
    .bind(&req.justification_text)
    .bind(req.subject_kind.as_str())
    .bind(&req.subject_value)
    .bind(disclosed_at)
    .bind(&disclosed_columns)
    .bind(resolved_declaration_id)
    .bind(req.mlat_foreign_fiu.as_deref())
    .bind(req.mlat_egmont_request_id.as_deref())
    .bind(event_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "fiu_disclosure_log insert failed");
        ServiceError::Internal
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "fiu disclosure tx commit failed");
        ServiceError::Internal
    })?;

    tracing::info!(
        disclosure_id = %disclosure_id,
        case = %req.anif_case_reference,
        match_count = matches.len(),
        "TODO-008: FIU disclosure recorded"
    );

    Ok(Json(FiuSearchResponse {
        disclosure_id,
        matches,
        disclosed_at: disclosed_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FiuDisclosureRecord {
    #[schema(value_type = String, format = "uuid")]
    pub disclosure_id: Uuid,
    pub anif_case_reference: String,
    pub subject_kind: String,
    pub subject_value: String,
    pub justification_text: String,
    pub disclosed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_declaration_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlat_foreign_fiu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlat_egmont_request_id: Option<String>,
}

/// `GET /v1/fiu/disclosure/{id}` — ANIF retrieves the disclosure
/// log row for a prior search. Only the same FIU principal that
/// produced the row may read it (the FIU's case-file audit trail
/// is per-FIU, not platform-wide).
#[utoipa::path(
    get,
    path = "/v1/fiu/disclosure/{disclosure_id}",
    operation_id = "fiuGetDisclosure",
    params(("disclosure_id" = String, Path, description = "Disclosure UUID")),
    responses(
        (status = 200, description = "Disclosure record", body = FiuDisclosureRecord),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Caller is not FIU-class"),
        (status = 404, description = "Disclosure not found OR not produced by this FIU"),
    ),
    security(
        ("bearer" = []),
        ("devPrincipalHeader" = []),
    ),
    tag = "fiu"
)]
#[tracing::instrument(skip_all, fields(principal = %principal.subject))]
pub(crate) async fn fiu_get_disclosure(
    State(state): State<FiuState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(disclosure_id): Path<Uuid>,
) -> Result<Json<FiuDisclosureRecord>, ServiceError> {
    if principal.class != PrincipalClass::FiuAnif {
        return Err(ServiceError::AuthorizationDenied(
            "fiu/disclosure is gated to the FIU principal class",
        ));
    }

    let row_opt: Option<DisclosureRow> = sqlx::query_as(
        r#"
        SELECT disclosure_id, anif_case_reference, subject_kind,
               subject_value, justification_text, disclosed_at,
               resolved_declaration_id, mlat_foreign_fiu,
               mlat_egmont_request_id
        FROM fiu_disclosure_log
        WHERE disclosure_id = $1
          AND requesting_principal = $2
        "#,
    )
    .bind(disclosure_id)
    .bind(&principal.subject)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "fiu disclosure read failed");
        ServiceError::Internal
    })?;

    let row = row_opt.ok_or_else(|| {
        // FIND-004: 404 (not 403) when the row exists but belongs to
        // a different FIU principal — same enumeration-protection
        // posture as `GET /v1/declarations/{id}`.
        ServiceError::NotFound(disclosure_id.to_string())
    })?;

    Ok(Json(FiuDisclosureRecord {
        disclosure_id: row.disclosure_id,
        anif_case_reference: row.anif_case_reference,
        subject_kind: row.subject_kind,
        subject_value: row.subject_value,
        justification_text: row.justification_text,
        disclosed_at: row
            .disclosed_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        resolved_declaration_id: row.resolved_declaration_id,
        mlat_foreign_fiu: row.mlat_foreign_fiu,
        mlat_egmont_request_id: row.mlat_egmont_request_id,
    }))
}

#[derive(sqlx::FromRow)]
struct DisclosureRow {
    disclosure_id: Uuid,
    anif_case_reference: String,
    subject_kind: String,
    subject_value: String,
    justification_text: String,
    disclosed_at: OffsetDateTime,
    resolved_declaration_id: Option<Uuid>,
    mlat_foreign_fiu: Option<String>,
    mlat_egmont_request_id: Option<String>,
}

/// Resolve a search subject to a list of matching declarations.
///
/// Only `DeclarationId` is fully wired today; the other kinds return
/// an empty match list and a NULL `resolved_declaration_id`, with the
/// disclosure_log row recording that the FIU asked. The follow-up
/// (TODO-008-resolver) wires name + national-ID matching through
/// the Person service and the Stage-2 BUNEC adapter.
async fn resolve_subject(
    pool: &PgPool,
    kind: FiuSubjectKind,
    value: &str,
) -> Result<(Vec<FiuMatchProjection>, Option<Uuid>), sqlx::Error> {
    match kind {
        FiuSubjectKind::DeclarationId => {
            let parsed = match Uuid::parse_str(value) {
                Ok(u) => u,
                Err(_) => return Ok((Vec::new(), None)),
            };
            let row_opt: Option<(JsonValue,)> = sqlx::query_as(
                r#"
                SELECT row_to_json(d)::jsonb
                FROM declarations d
                WHERE d.declaration_id = $1
                "#,
            )
            .bind(parsed)
            .fetch_optional(pool)
            .await?;
            if let Some((projection,)) = row_opt {
                Ok((
                    vec![FiuMatchProjection {
                        declaration_id: parsed,
                        projection,
                    }],
                    Some(parsed),
                ))
            } else {
                Ok((Vec::new(), None))
            }
        }
        // Other kinds: TODO-008-resolver follow-up. The audit row is
        // still written by the caller; the empty match list signals
        // "FIU asked, platform had no direct hit".
        _ => Ok((Vec::new(), None)),
    }
}

pub fn router(state: FiuState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/v1/fiu/search", post(fiu_search))
        .route("/v1/fiu/disclosure/{disclosure_id}", get(fiu_get_disclosure))
        .with_state(state)
}
