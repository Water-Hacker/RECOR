//! TODO-040 — operator-facing access to the person-service outbox + DLQ.
//!
//! Mirrors `services/declaration/src/infrastructure/outbox_admin.rs`
//! and `services/entity-service/src/infrastructure/outbox_admin.rs`.
//! Same schema (migration `0004_add_outbox_dlq.sql`), same atomic
//! replay-move pattern, same operator-visible row shape.
//!
//! Architectural placement: this lives in `infrastructure/` because
//! it's a SQL surface over the existing tables. It deliberately does
//! NOT go through the `PersonRepository` port — those abstractions are
//! for the domain layer's writes, which all flow through the
//! aggregate. Admin queries skip the aggregate (the aggregate doesn't
//! care that an outbox row was retried).
//!
//! Uses runtime `sqlx::query` (not the compile-time macro). The
//! person-service postgres adapter is also runtime-checked (see
//! `services/person-service/CLAUDE.md`); the
//! R-PERSON-SQLX-CACHE follow-up flips both at once.

use sqlx::{PgPool, Row};
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
    pub aggregate_type: String,
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
        let rows = sqlx::query(
            "SELECT id, event_id, event_type, event_version, aggregate_type, \
                    aggregate_id, partition_key, payload, created_at, \
                    dead_lettered_at, dispatch_attempts, last_error \
             FROM outbox_dlq \
             ORDER BY dead_lettered_at DESC \
             LIMIT $1 OFFSET $2",
        )
        .bind(effective_limit)
        .bind(offset_clamped)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(DlqRow {
                id: row.try_get("id")?,
                event_id: row.try_get("event_id")?,
                event_type: row.try_get("event_type")?,
                event_version: row.try_get("event_version")?,
                aggregate_type: row.try_get("aggregate_type")?,
                aggregate_id: row.try_get("aggregate_id")?,
                partition_key: row.try_get("partition_key")?,
                payload: row.try_get("payload")?,
                created_at: row.try_get("created_at")?,
                dead_lettered_at: row.try_get("dead_lettered_at")?,
                dispatch_attempts: row.try_get("dispatch_attempts")?,
                last_error: row.try_get("last_error")?,
            });
        }
        Ok(out)
    }

    pub async fn count_dlq(&self) -> Result<i64, OutboxAdminError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM outbox_dlq")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }

    /// Atomically replay a DLQ row: move it back into `outbox` with
    /// dispatch_attempts reset to 0 and last_error cleared.
    ///
    /// Idempotency: if the row is not in outbox_dlq (perhaps already
    /// replayed by another operator), returns NotFound rather than
    /// silently doing nothing.
    #[instrument(skip(self), fields(id = %id))]
    pub async fn replay_dlq(&self, id: Uuid) -> Result<(), OutboxAdminError> {
        let mut tx = self.pool.begin().await?;

        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM outbox_dlq WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if exists.is_none() {
            return Err(OutboxAdminError::NotFound(id));
        }

        sqlx::query(
            "INSERT INTO outbox ( \
                 id, event_id, event_type, event_version, aggregate_type, \
                 aggregate_id, partition_key, payload, headers, \
                 created_at, dispatched_at, dispatch_attempts, last_error \
             ) \
             SELECT \
                 id, event_id, event_type, event_version, aggregate_type, \
                 aggregate_id, partition_key, payload, headers, \
                 created_at, NULL, 0, NULL \
             FROM outbox_dlq \
             WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM outbox_dlq WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        info!(%id, "DLQ row replayed back into outbox");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlq_row_carries_forensic_fields() {
        let row = DlqRow {
            id: Uuid::nil(),
            event_id: Uuid::nil(),
            event_type: "person.registered.v1".into(),
            event_version: 1,
            aggregate_type: "person".into(),
            aggregate_id: Uuid::nil(),
            partition_key: "p".into(),
            payload: serde_json::json!({"k": "v"}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            dead_lettered_at: OffsetDateTime::UNIX_EPOCH,
            dispatch_attempts: 12,
            last_error: Some("transport: connection refused".into()),
        };
        assert_eq!(row.event_type, "person.registered.v1");
        assert_eq!(row.dispatch_attempts, 12);
        assert_eq!(row.payload["k"], "v");
    }

    #[test]
    fn outbox_admin_error_not_found_carries_id() {
        let id = Uuid::now_v7();
        let err = OutboxAdminError::NotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn outbox_admin_error_backend_round_trips_sqlx() {
        let sqlx_err = sqlx::Error::Configuration("synthetic".into());
        let admin: OutboxAdminError = sqlx_err.into();
        match admin {
            OutboxAdminError::Backend(inner) => {
                assert!(inner.to_string().contains("synthetic"));
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }
}
