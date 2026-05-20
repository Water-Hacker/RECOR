//! Read-only port into the declaration service's `declaration_events`
//! table. The reconciler never writes — its only output is metrics +
//! structured logs.

use async_trait::async_trait;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct EventLogRow {
    pub event_id: Uuid,
    pub declaration_id: Uuid,
    pub event_type: String,
    pub event_time: OffsetDateTime,
}

#[async_trait]
pub trait EventLogRepo: Send + Sync {
    /// Fetch event-log rows whose `event_time` falls within
    /// `[now - lookback, now - grace_period]`. Caller chooses the
    /// bounds; the trait does not enforce a default.
    async fn fetch_eligible(
        &self,
        lookback: time::Duration,
        grace_period: time::Duration,
        limit: i64,
    ) -> Result<Vec<EventLogRow>, sqlx::Error>;
}

pub struct PostgresEventLogRepo {
    pool: sqlx::PgPool,
}

impl PostgresEventLogRepo {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventLogRepo for PostgresEventLogRepo {
    async fn fetch_eligible(
        &self,
        lookback: time::Duration,
        grace_period: time::Duration,
        limit: i64,
    ) -> Result<Vec<EventLogRow>, sqlx::Error> {
        // The query is intentionally written without compile-time
        // verification so the reconciler doesn't need the declaration
        // service's `.sqlx/` offline cache. The shape is stable since
        // declaration migration 0001.
        let now = OffsetDateTime::now_utc();
        let oldest = now - lookback;
        let youngest = now - grace_period;

        let rows = sqlx::query(
            r#"SELECT event_id, declaration_id, event_type, event_time
               FROM declaration_events
               WHERE event_time >= $1 AND event_time <= $2
               ORDER BY event_time ASC
               LIMIT $3"#,
        )
        .bind(oldest)
        .bind(youngest)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(EventLogRow {
                event_id: row.try_get("event_id")?,
                declaration_id: row.try_get("declaration_id")?,
                event_type: row.try_get("event_type")?,
                event_time: row.try_get("event_time")?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the in-memory repo double used by the
    //! reconciler's behaviour tests. The Postgres path is exercised
    //! by the integration test in `tests/integration.rs`.

    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemoryEventLog {
        pub rows: Mutex<Vec<EventLogRow>>,
    }

    impl InMemoryEventLog {
        pub fn push(&self, row: EventLogRow) {
            self.rows.lock().unwrap().push(row);
        }
    }

    #[async_trait]
    impl EventLogRepo for InMemoryEventLog {
        async fn fetch_eligible(
            &self,
            _lookback: time::Duration,
            _grace_period: time::Duration,
            _limit: i64,
        ) -> Result<Vec<EventLogRow>, sqlx::Error> {
            Ok(self.rows.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn in_memory_event_log_returns_pushed_rows() {
        let repo = InMemoryEventLog::default();
        repo.push(EventLogRow {
            event_id: Uuid::now_v7(),
            declaration_id: Uuid::now_v7(),
            event_type: "declaration.submitted.v1".to_string(),
            event_time: OffsetDateTime::now_utc(),
        });
        let rows = repo
            .fetch_eligible(time::Duration::seconds(60), time::Duration::seconds(0), 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
