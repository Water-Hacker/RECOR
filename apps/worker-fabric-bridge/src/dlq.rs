//! `fabric_bridge_dlq` repository.
//!
//! The DLQ table mirrors the declaration service's `outbox_dlq` shape
//! so an operator can use the same forensics tooling. See the migration
//! at `apps/worker-fabric-bridge/migrations/0001_create_fabric_bridge_dlq.sql`.

use serde_json::Value as JsonValue;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DlqError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct DlqRow {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub payload: JsonValue,
    pub attempts: i32,
    pub last_error: String,
    /// One of "permanent", "non_retryable", "config" — pairs with the
    /// `recor_fabric_dlq_writes_total{cause}` counter for forensics.
    pub cause: String,
}

#[async_trait::async_trait]
pub trait DlqRepo: Send + Sync + std::fmt::Debug {
    async fn insert(&self, row: DlqRow) -> Result<(), DlqError>;
    async fn count(&self) -> Result<i64, DlqError>;
}

#[derive(Debug, Clone)]
pub struct PostgresDlqRepo {
    pool: PgPool,
}

impl PostgresDlqRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DlqRepo for PostgresDlqRepo {
    async fn insert(&self, row: DlqRow) -> Result<(), DlqError> {
        // `INSERT ... ON CONFLICT DO NOTHING` — a row landing twice for
        // the same event_id is the bridge worker hitting the same
        // permanent failure twice; we don't want a duplicate-key error
        // bubbling up as a separate incident.
        sqlx::query(
            r#"
            INSERT INTO fabric_bridge_dlq (
                event_id, event_type, aggregate_id, payload,
                attempts, last_error, cause
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(row.event_id)
        .bind(&row.event_type)
        .bind(row.aggregate_id)
        .bind(&row.payload)
        .bind(row.attempts)
        .bind(&row.last_error)
        .bind(&row.cause)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, DlqError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fabric_bridge_dlq")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}

/// In-memory implementation for unit tests and local development.
#[derive(Debug, Default)]
pub struct InMemoryDlqRepo {
    rows: tokio::sync::Mutex<Vec<DlqRow>>,
}

impl InMemoryDlqRepo {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn rows(&self) -> Vec<DlqRow> {
        self.rows.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl DlqRepo for InMemoryDlqRepo {
    async fn insert(&self, row: DlqRow) -> Result<(), DlqError> {
        self.rows.lock().await.push(row);
        Ok(())
    }
    async fn count(&self) -> Result<i64, DlqError> {
        Ok(self.rows.lock().await.len() as i64)
    }
}
