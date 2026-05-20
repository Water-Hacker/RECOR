//! Outbox retention worker (COMP-2) — person-service.
//!
//! Periodically prunes `outbox` rows whose `dispatched_at` is older
//! than `OUTBOX_RETENTION_DAYS` (default 0 = disabled). The
//! `person_events` log is NEVER touched — the event log is the
//! substrate the BLAKE3 receipts pin to (architecturally + via the
//! immutability triggers in migration `0001_init.sql`).
//!
//! Mirrors `services/declaration/src/infrastructure/retention.rs`
//! one-for-one, adapted for person-service's runtime-checked
//! `sqlx::query` pattern (R-PERSON-SQLX-CACHE follow-up will flip
//! the crate to the compile-time `query!` macro + committed
//! `.sqlx/` cache; until then this module uses runtime queries).
//!
//! ## Doctrine compliance
//!
//! - **D14 fail-closed** — the safe default is `OUTBOX_RETENTION_DAYS=0`,
//!   which DISABLES pruning entirely. Tests run with this default so
//!   they never accidentally delete data that another test depends on.
//!   A future deployment that wants real retention must opt in by
//!   setting the env explicitly.
//! - **D15 cryptographic provenance** — never touches `person_events`;
//!   the event log is forever-retained per the COMP-2 immutability
//!   triggers + REVOKE on PUBLIC.
//! - **D16 observability** — every prune cycle records a
//!   `tracing::info!` event and increments the
//!   `recor_outbox_retention_pruned_total` counter so operators can
//!   alert on a pruning loop that suddenly drops to zero (would
//!   indicate either a DB outage or the retention clock skewing).

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Result of a single prune cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOutcome {
    /// Number of outbox rows deleted.
    pub pruned: u64,
}

/// Background worker that prunes the `outbox` table on a tokio interval.
///
/// Construct with [`OutboxRetention::new`], optionally tune the
/// behaviour with the builder-style `with_*` methods, then spawn
/// `tokio::spawn(retention.run(cancel))` from the composition root.
pub struct OutboxRetention {
    pool: PgPool,
    /// Number of days after `dispatched_at` before a row is pruned.
    /// Zero disables pruning entirely — the test-safe default.
    retention_days: u64,
    /// Interval between prune cycles.
    interval: Duration,
    /// Optional metrics handle. Tests construct the worker without one.
    metrics: Option<Arc<crate::metrics::Metrics>>,
}

impl OutboxRetention {
    /// Build a new retention worker bound to the given pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            retention_days: 0,
            interval: Duration::from_secs(86_400),
            metrics: None,
        }
    }

    /// Set the retention window in days. `0` disables pruning.
    #[must_use]
    pub fn with_retention_days(mut self, days: u64) -> Self {
        self.retention_days = days;
        self
    }

    /// Set the cycle interval.
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Wire the shared metrics registry so prune cycles increment the
    /// `recor_outbox_retention_pruned_total` counter.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Run the worker until `cancel` fires. Returns immediately (after
    /// emitting an `info!`) when retention is disabled — there is no
    /// reason to keep a sleeping task alive that will never do work.
    #[instrument(skip_all, fields(retention_days = self.retention_days, interval_s = self.interval.as_secs()))]
    pub async fn run(&self, cancel: CancellationToken) {
        if self.retention_days == 0 {
            info!(
                "outbox retention worker disabled (OUTBOX_RETENTION_DAYS=0); person_events is NEVER touched"
            );
            cancel.cancelled().await;
            return;
        }
        info!(
            retention_days = self.retention_days,
            interval_s = self.interval.as_secs(),
            "outbox retention worker started"
        );
        let mut tick = tokio::time::interval(self.interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("outbox retention worker shutting down");
                    return;
                }
                _ = tick.tick() => {
                    match self.prune_once().await {
                        Ok(outcome) => {
                            if let Some(m) = self.metrics.as_ref() {
                                m.outbox_retention_pruned_total
                                    .with_label_values(&["success"])
                                    .inc_by(outcome.pruned);
                            }
                            info!(
                                pruned = outcome.pruned,
                                retention_days = self.retention_days,
                                "outbox retention cycle complete"
                            );
                        }
                        Err(e) => {
                            if let Some(m) = self.metrics.as_ref() {
                                m.outbox_retention_pruned_total
                                    .with_label_values(&["error"])
                                    .inc();
                            }
                            error!(error = ?e, "outbox retention cycle failed");
                        }
                    }
                }
            }
        }
    }

    /// Execute one prune cycle. Public for tests + the future CronJob
    /// shell-out wrapper.
    ///
    /// SQL: `DELETE FROM outbox WHERE dispatched_at IS NOT NULL AND
    /// dispatched_at < NOW() - make_interval(secs => $1::double precision)`.
    /// The retention worker NEVER deletes rows where `dispatched_at IS
    /// NULL` — that would silently drop un-delivered events. It also
    /// never touches `person_events` (immutable event log).
    pub async fn prune_once(&self) -> Result<PruneOutcome, sqlx::Error> {
        if self.retention_days == 0 {
            debug!("retention disabled; prune_once is a no-op");
            return Ok(PruneOutcome { pruned: 0 });
        }
        let days = i64::try_from(self.retention_days).unwrap_or(i64::MAX);
        let cutoff_secs = days.saturating_mul(86_400);
        let result = sqlx::query(
            r#"
            DELETE FROM outbox
            WHERE dispatched_at IS NOT NULL
              AND dispatched_at < NOW() - make_interval(secs => $1::double precision)
            "#,
        )
        .bind(cutoff_secs as f64)
        .execute(&self.pool)
        .await?;
        let pruned = result.rows_affected();
        if pruned == 0 {
            debug!("outbox retention prune: no eligible rows");
        } else {
            debug!(pruned, "outbox retention prune: rows deleted");
        }
        Ok(PruneOutcome { pruned })
    }
}

/// Helper for `main.rs`: warns when retention is enabled but the
/// interval is suspiciously short (< 60 seconds) — usually a
/// configuration mistake (someone passing the value in days where the
/// env expected seconds). Loud at startup, never silent.
pub fn warn_if_misconfigured(retention_days: u64, interval_secs: u64) {
    if retention_days > 0 && interval_secs < 60 {
        warn!(
            interval_s = interval_secs,
            "OUTBOX_RETENTION_INTERVAL_SECONDS is very short (<60s); did you mean to express it in minutes?"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn disabled_when_days_zero() {
        let worker = OutboxRetention::new(mock_pool());
        assert_eq!(worker.retention_days, 0);
        let configured = OutboxRetention::new(mock_pool())
            .with_retention_days(30)
            .with_interval(Duration::from_secs(3600));
        assert_eq!(configured.retention_days, 30);
        assert_eq!(configured.interval, Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn run_with_disabled_retention_returns_on_cancel() {
        let cancel = CancellationToken::new();
        let worker = OutboxRetention::new(mock_pool());
        let cancel_c = cancel.clone();
        let h = tokio::spawn(async move { worker.run(cancel_c).await });
        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), h)
            .await
            .expect("disabled retention worker must shut down on cancel");
    }

    #[tokio::test]
    async fn prune_once_is_noop_when_disabled() {
        let worker = OutboxRetention::new(mock_pool());
        let outcome = worker
            .prune_once()
            .await
            .expect("disabled prune_once must succeed without DB access");
        assert_eq!(outcome.pruned, 0);
    }

    #[test]
    fn warn_helper_does_not_panic() {
        warn_if_misconfigured(0, 0);
        warn_if_misconfigured(30, 86_400);
        warn_if_misconfigured(30, 5);
    }

    /// Construct a `PgPool` that is never connected to anything. Used
    /// by unit tests that only exercise the builder + the cancel-path
    /// (which never executes SQL).
    fn mock_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://does-not-matter:5432/x")
            .expect("connect_lazy cannot fail without a network call")
    }
}
