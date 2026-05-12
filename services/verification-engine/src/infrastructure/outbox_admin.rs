//! Operator-facing access to the verification outbox + DLQ.
//!
//! Closes R-LOOP-DLQ-3: oncall needs a way to inspect dead-lettered
//! rows on the verification engine and selectively replay them after
//! the underlying cause has been resolved (declaration writeback came
//! back up, schema was fixed, etc.). Mirror of
//! `services/declaration/src/infrastructure/outbox_admin.rs`.
//!
//! Note: the V-engine outbox schema is smaller than the declaration's
//! — there is no `aggregate_type` column and no `headers` column.
//! The replay SQL is adjusted accordingly.
//!
//! Architectural placement: this lives in `infrastructure/` because
//! it's a SQL surface over the existing tables. It deliberately
//! does NOT go through the `VerificationRepository` port — those
//! abstractions are for domain writes that all flow through the
//! aggregate. Admin queries skip the aggregate (the aggregate doesn't
//! care that an outbox row was retried).

use sqlx::PgPool;
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info, instrument};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OutboxAdminStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct DlqRow {
    pub id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: i32,
    pub aggregate_id: Uuid,
    pub partition_key: String,
    pub payload: serde_json::Value,
    pub created_at: OffsetDateTime,
    pub dead_lettered_at: OffsetDateTime,
    pub dispatch_attempts: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Error)]
pub enum OutboxAdminError {
    #[error("DLQ row {0} not found")]
    NotFound(Uuid),

    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),
}

impl OutboxAdminStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List DLQ rows in reverse chronological order. Caller bounds
    /// `limit` to a sensible page size; the SQL query enforces a hard
    /// ceiling of 200 rows even if the caller asks for more.
    #[instrument(skip(self), fields(limit, offset))]
    pub async fn list_dlq(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DlqRow>, OutboxAdminError> {
        let effective_limit = limit.clamp(1, 200);
        let offset_clamped = offset.max(0);
        let rows = sqlx::query!(
            r#"
            SELECT id, event_id, event_type, event_version,
                   aggregate_id, partition_key, payload, created_at,
                   dead_lettered_at, dispatch_attempts, last_error
            FROM verification_outbox_dlq
            ORDER BY dead_lettered_at DESC
            LIMIT $1 OFFSET $2
            "#,
            effective_limit,
            offset_clamped,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DlqRow {
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
            })
            .collect())
    }

    /// Count of DLQ rows. Cheap; used by the list endpoint for pagination.
    pub async fn count_dlq(&self) -> Result<i64, OutboxAdminError> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*)::bigint AS "n!" FROM verification_outbox_dlq"#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.n)
    }

    /// Atomically replay a DLQ row: move it back into
    /// `verification_outbox` with dispatch_attempts reset to 0,
    /// last_error cleared, and dispatched_at NULL. The writeback-relay
    /// polling loop will pick it up on the next tick.
    ///
    /// The atomic move:
    ///   BEGIN;
    ///   INSERT INTO verification_outbox (...) SELECT ... WHERE id = $1;
    ///   DELETE FROM verification_outbox_dlq WHERE id = $1;
    ///   COMMIT;
    /// guarantees the row exists in exactly one table at any time.
    ///
    /// Idempotency: if the row is not in verification_outbox_dlq
    /// (perhaps already replayed by another operator), returns
    /// NotFound rather than silently doing nothing. Operators see a
    /// 404 from the endpoint and know to investigate.
    #[instrument(skip(self), fields(id = %id))]
    pub async fn replay_dlq(&self, id: Uuid) -> Result<(), OutboxAdminError> {
        let mut tx = self.pool.begin().await?;

        let exists = sqlx::query_scalar!(
            "SELECT id FROM verification_outbox_dlq WHERE id = $1 FOR UPDATE",
            id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_none() {
            return Err(OutboxAdminError::NotFound(id));
        }

        sqlx::query!(
            r#"
            INSERT INTO verification_outbox (
                id, event_id, event_type, event_version,
                aggregate_id, partition_key, payload,
                created_at, dispatched_at, dispatch_attempts, last_error
            )
            SELECT
                id, event_id, event_type, event_version,
                aggregate_id, partition_key, payload,
                created_at, NULL, 0, NULL
            FROM verification_outbox_dlq
            WHERE id = $1
            "#,
            id,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!("DELETE FROM verification_outbox_dlq WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        info!(%id, "verification DLQ row replayed back into verification_outbox");
        Ok(())
    }
}
