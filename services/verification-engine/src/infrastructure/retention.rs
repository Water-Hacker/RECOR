//! Verification outbox retention worker (COMP-2).
//!
//! Mirror of `services/declaration/src/infrastructure/retention.rs`.
//! See that file for the full doctrine commentary. The only delta
//! here is the table name (`verification_outbox`).
//!
//! Pruning policy (per `docs/compliance/data-retention.md`):
//!   * `verification_outbox` — pruned 30 days after `dispatched_at`
//!   * `verification_outbox_dlq` — NEVER touched (forensic surface)
//!   * `verification_cases` — NEVER touched (D15; immutability
//!     enforced at the SQL layer by migration 0003)

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOutcome {
    pub pruned: u64,
}

pub struct VerificationOutboxRetention {
    pool: PgPool,
    retention_days: u64,
    interval: Duration,
    metrics: Option<Arc<crate::metrics::Metrics>>,
}

impl VerificationOutboxRetention {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            retention_days: 0,
            interval: Duration::from_secs(86_400),
            metrics: None,
        }
    }

    #[must_use]
    pub fn with_retention_days(mut self, days: u64) -> Self {
        self.retention_days = days;
        self
    }

    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    #[instrument(skip_all, fields(retention_days = self.retention_days, interval_s = self.interval.as_secs()))]
    pub async fn run(&self, cancel: CancellationToken) {
        if self.retention_days == 0 {
            info!(
                "verification outbox retention worker disabled (OUTBOX_RETENTION_DAYS=0); verification_cases and verification_outbox_dlq are NEVER touched"
            );
            cancel.cancelled().await;
            return;
        }
        info!(
            retention_days = self.retention_days,
            interval_s = self.interval.as_secs(),
            "verification outbox retention worker started"
        );
        let mut tick = tokio::time::interval(self.interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("verification outbox retention worker shutting down");
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
                                "verification outbox retention cycle complete"
                            );
                        }
                        Err(e) => {
                            if let Some(m) = self.metrics.as_ref() {
                                m.outbox_retention_pruned_total
                                    .with_label_values(&["error"])
                                    .inc();
                            }
                            error!(error = ?e, "verification outbox retention cycle failed");
                        }
                    }
                }
            }
        }
    }

    pub async fn prune_once(&self) -> Result<PruneOutcome, sqlx::Error> {
        if self.retention_days == 0 {
            debug!("retention disabled; prune_once is a no-op");
            return Ok(PruneOutcome { pruned: 0 });
        }
        let days = i64::try_from(self.retention_days).unwrap_or(i64::MAX);
        let cutoff_secs = days.saturating_mul(86_400);
        let result = sqlx::query!(
            r#"
            DELETE FROM verification_outbox
            WHERE dispatched_at IS NOT NULL
              AND dispatched_at < NOW() - make_interval(secs => $1::double precision)
            "#,
            cutoff_secs as f64,
        )
        .execute(&self.pool)
        .await?;
        let pruned = result.rows_affected();
        if pruned == 0 {
            debug!("verification outbox retention prune: no eligible rows");
        } else {
            debug!(pruned, "verification outbox retention prune: rows deleted");
        }
        Ok(PruneOutcome { pruned })
    }
}

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
        // sqlx `connect_lazy` requires a tokio context even without
        // a network call — hence `#[tokio::test]`.
        let worker = VerificationOutboxRetention::new(mock_pool());
        assert_eq!(worker.retention_days, 0);
        let configured = VerificationOutboxRetention::new(mock_pool())
            .with_retention_days(30)
            .with_interval(Duration::from_secs(3600));
        assert_eq!(configured.retention_days, 30);
        assert_eq!(configured.interval, Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn prune_once_is_noop_when_disabled() {
        let worker = VerificationOutboxRetention::new(mock_pool());
        let outcome = worker
            .prune_once()
            .await
            .expect("disabled prune_once must succeed without DB access");
        assert_eq!(outcome.pruned, 0);
    }

    #[tokio::test]
    async fn run_with_disabled_retention_returns_on_cancel() {
        let cancel = CancellationToken::new();
        let worker = VerificationOutboxRetention::new(mock_pool());
        let cancel_c = cancel.clone();
        let h = tokio::spawn(async move { worker.run(cancel_c).await });
        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), h)
            .await
            .expect("disabled retention worker must shut down on cancel");
    }

    #[test]
    fn warn_helper_does_not_panic() {
        warn_if_misconfigured(0, 0);
        warn_if_misconfigured(30, 86_400);
        warn_if_misconfigured(30, 5);
    }

    fn mock_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://does-not-matter:5432/x")
            .expect("connect_lazy cannot fail")
    }
}
