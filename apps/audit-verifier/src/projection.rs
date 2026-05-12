//! Read-side access to the Declaration service's projection.
//!
//! The verifier needs only the canonical event payload (and the
//! receipt_hash_hex the service originally produced); we read directly
//! from the `declaration_events` table because that is the immutable
//! source of truth (the projection table is reconstructed by replay).

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("event missing required field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRow {
    pub event_id: Uuid,
    pub declaration_id: Uuid,
    pub event_type: String,
    pub event_payload: JsonValue,
    pub receipt_hash_hex: String,
    pub ts: String,
}

#[async_trait]
pub trait ProjectionRepo: Send + Sync + std::fmt::Debug {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError>;
}

#[derive(Debug, Clone)]
pub struct PostgresProjectionRepo {
    pool: PgPool,
}

impl PostgresProjectionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProjectionRepo for PostgresProjectionRepo {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError> {
        // The Declaration service's `declaration_events` table doesn't
        // carry an `event_id` column directly — the outbox carries it
        // and the event is correlated by (correlation_id) or by index
        // in the per-aggregate sequence. For the verifier skeleton we
        // join through `outbox` (and `outbox_dlq` as a fallback) to
        // recover the event row, then read the canonical payload from
        // `declaration_events`.
        //
        // This query is intentionally written in the runtime-checked
        // form (non-macro) so it does not require an entry in the
        // declaration service's sqlx cache.
        let row = sqlx::query_as::<_, ProjectionQueryRow>(
            r#"
            SELECT
                o.event_id           AS event_id,
                o.aggregate_id       AS declaration_id,
                e.event_type         AS event_type,
                e.event_payload      AS event_payload
            FROM outbox o
            JOIN declaration_events e
              ON e.declaration_id = o.aggregate_id
             AND e.event_type    = o.event_type
            WHERE o.event_id = $1
            LIMIT 1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_row()?)),
            None => Ok(None),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectionQueryRow {
    event_id: Uuid,
    declaration_id: Uuid,
    event_type: String,
    event_payload: JsonValue,
}

impl ProjectionQueryRow {
    fn into_row(self) -> Result<ProjectionRow, ProjectionError> {
        let receipt_hash_hex = self
            .event_payload
            .get("receipt_hash_hex")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or(ProjectionError::MissingField("receipt_hash_hex"))?;
        let ts = ["submitted_at", "amended_at", "corrected_at", "superseded_at"]
            .iter()
            .find_map(|k| self.event_payload.get(*k).and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .ok_or(ProjectionError::MissingField("timestamp"))?;
        Ok(ProjectionRow {
            event_id: self.event_id,
            declaration_id: self.declaration_id,
            event_type: self.event_type,
            event_payload: self.event_payload,
            receipt_hash_hex,
            ts,
        })
    }
}

/// In-memory implementation for unit testing the report layer.
#[derive(Debug, Default)]
pub struct InMemoryProjectionRepo {
    pub rows: tokio::sync::Mutex<std::collections::HashMap<Uuid, ProjectionRow>>,
}

impl InMemoryProjectionRepo {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn add(&self, row: ProjectionRow) {
        self.rows.lock().await.insert(row.event_id, row);
    }
}

#[async_trait]
impl ProjectionRepo for InMemoryProjectionRepo {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError> {
        Ok(self.rows.lock().await.get(&event_id).cloned())
    }
}
