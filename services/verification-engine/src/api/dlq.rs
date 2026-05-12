//! Operator-facing DLQ endpoints for the verification engine
//! (R-LOOP-DLQ-3).
//!
//!   GET  /v1/internal/verification-outbox-dlq             — list rows
//!   POST /v1/internal/verification-outbox-dlq/{id}/replay — atomic move
//!                                                            back to outbox
//!
//! Authorisation: both endpoints require an authenticated principal
//! whose subject string appears in `Config::admin_principals_list()`.
//! Authentication is handled by the existing `auth_middleware`:
//! dev-mode `X-Recor-Dev-Principal`, production OIDC JWT. The
//! authorisation gate is implemented here, after auth.
//!
//! An empty admin-principals list disables both endpoints entirely
//! (they return 503). This is the safe default: no env wired → no
//! admin surface.
//!
//! NOTE: the path differs from the declaration's
//! `/v1/internal/outbox-dlq` to make it unambiguous which service's
//! DLQ an operator is hitting when both services are deployed.

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::api::auth::Principal;
use crate::infrastructure::{DlqRow, OutboxAdminError, OutboxAdminStore};

#[derive(Clone)]
pub struct DlqAdminState {
    pub store: Arc<OutboxAdminStore>,
    /// Allowed admin principal subjects (deduplicated). Empty
    /// disables the endpoints (returns 503).
    pub admin_principals: Arc<HashSet<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ListDlqQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct ListDlqResponse {
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub items: Vec<DlqItem>,
}

#[derive(Debug, Serialize)]
pub struct DlqItem {
    pub id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: i32,
    pub aggregate_id: Uuid,
    pub partition_key: String,
    pub payload: serde_json::Value,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub dead_lettered_at: OffsetDateTime,
    pub dispatch_attempts: i32,
    pub last_error: Option<String>,
}

impl From<DlqRow> for DlqItem {
    fn from(row: DlqRow) -> Self {
        Self {
            id: row.id,
            event_id: row.event_id,
            event_type: row.event_type,
            event_version: row.event_version,
            aggregate_id: row.aggregate_id,
            partition_key: row.partition_key,
            payload: row.payload,
            created_at: row.created_at,
            dead_lettered_at: row.dead_lettered_at,
            dispatch_attempts: row.dispatch_attempts,
            last_error: row.last_error,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReplayDlqResponse {
    pub id: Uuid,
    pub replayed: bool,
}

#[instrument(
    skip_all,
    fields(principal = %principal.subject, limit = query.limit, offset = query.offset)
)]
pub async fn list_dlq(
    State(state): State<DlqAdminState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Query(query): Query<ListDlqQuery>,
) -> Result<Json<ListDlqResponse>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(err) = enforce_admin(&state.admin_principals, &principal) {
        return Err(err);
    }
    let total = state.store.count_dlq().await.map_err(backend_error)?;
    let rows = state
        .store
        .list_dlq(query.limit, query.offset)
        .await
        .map_err(backend_error)?;
    Ok(Json(ListDlqResponse {
        total,
        limit: query.limit,
        offset: query.offset,
        items: rows.into_iter().map(DlqItem::from).collect(),
    }))
}

#[instrument(skip_all, fields(principal = %principal.subject, id = %id))]
pub async fn replay_dlq(
    State(state): State<DlqAdminState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<ReplayDlqResponse>), (StatusCode, Json<serde_json::Value>)> {
    if let Some(err) = enforce_admin(&state.admin_principals, &principal) {
        return Err(err);
    }
    match state.store.replay_dlq(id).await {
        Ok(()) => {
            info!(%id, replayed_by = %principal.subject, "verification DLQ row replayed by admin");
            Ok((
                StatusCode::OK,
                Json(ReplayDlqResponse { id, replayed: true }),
            ))
        }
        Err(OutboxAdminError::NotFound(_)) => Err(error_response(
            StatusCode::NOT_FOUND,
            "dlq_row_not_found",
            &format!("no DLQ row with id {id}"),
        )),
        Err(e) => Err(backend_error(e)),
    }
}

/// Authorisation gate: rejects requests from a principal not on the
/// configured allowlist, OR returns 503 if the allowlist is empty
/// (admin endpoints disabled). Returns `None` on the happy path.
fn enforce_admin(
    admin_principals: &HashSet<String>,
    principal: &Principal,
) -> Option<(StatusCode, Json<serde_json::Value>)> {
    if admin_principals.is_empty() {
        warn!(
            "admin endpoint hit but ADMIN_PRINCIPALS is empty — endpoint disabled"
        );
        return Some(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin_disabled",
            "admin endpoints disabled — ADMIN_PRINCIPALS not configured",
        ));
    }
    if !admin_principals.contains(&principal.subject) {
        warn!(
            principal = %principal.subject,
            "non-admin principal attempted DLQ admin endpoint"
        );
        return Some(error_response(
            StatusCode::FORBIDDEN,
            "not_admin",
            "this principal is not authorised for admin endpoints",
        ));
    }
    None
}

fn error_response(
    status: StatusCode,
    kind: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(json!({ "error": { "kind": kind, "message": message } })),
    )
}

fn backend_error(e: OutboxAdminError) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = ?e, "verification outbox-admin backend failure");
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "internal failure",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(subject: &str) -> Principal {
        Principal {
            subject: subject.to_string(),
        }
    }

    fn admins(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn admin_endpoint_disabled_when_allowlist_empty() {
        let set = admins(&[]);
        let p = principal("spiffe://recor.cm/anyone");
        let err = enforce_admin(&set, &p).expect("expected refusal");
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn admin_endpoint_refuses_non_admin_principal() {
        let set = admins(&["spiffe://recor.cm/admin-1"]);
        let p = principal("spiffe://recor.cm/regular-declarant");
        let err = enforce_admin(&set, &p).expect("expected refusal");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn admin_endpoint_admits_listed_principal() {
        let set = admins(&["spiffe://recor.cm/admin-1"]);
        let p = principal("spiffe://recor.cm/admin-1");
        assert!(enforce_admin(&set, &p).is_none());
    }

    #[test]
    fn admin_endpoint_admits_any_of_multiple_admins() {
        let set = admins(&[
            "spiffe://recor.cm/admin-1",
            "spiffe://recor.cm/admin-2",
            "spiffe://recor.cm/admin-3",
        ]);
        for who in [
            "spiffe://recor.cm/admin-1",
            "spiffe://recor.cm/admin-2",
            "spiffe://recor.cm/admin-3",
        ] {
            assert!(enforce_admin(&set, &principal(who)).is_none(), "{who}");
        }
        assert!(enforce_admin(&set, &principal("spiffe://recor.cm/intruder")).is_some());
    }
}
