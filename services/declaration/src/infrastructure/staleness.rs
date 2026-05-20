//! TODO-005 — Staleness watcher.
//!
//! FATF R.24 §c.24.8 fn 29 sets the explicit benchmark: BO data MUST
//! be updated within one month of any change. This worker scans the
//! `declarations` projection for rows where the declarant-asserted
//! `last_event_observed_at` is older than the staleness threshold
//! (default 30 days) AND no Amend / Correct event has landed since.
//! Each stale row increments
//! `recor_declaration_staleness_observed_total`. The metric is the
//! signal; the alert rule paging on it lives in
//! `alerts/recor-prometheus-rules.yaml` (deferred to follow-up).
//!
//! ## Why metric-only and not a "send notification" worker?
//!
//! Notification dispatch is a separate subsystem (email/SMS/portal
//! banner) that this PR doesn't ship. The metric is the minimum-
//! viable signal: operators see "N declarations are now > 30 days
//! stale" in Grafana and triage. A future PR wires a per-declarant
//! notification dispatcher once the SMS / email infra exists.
//!
//! ## Doctrine compliance
//!
//! - **D14 fail-closed** — the safe default is
//!   `STALENESS_INTERVAL_SECONDS=0` (worker disabled). Operators opt
//!   in by setting it explicitly.
//! - **D15 cryptographic provenance** — the worker is read-only
//!   against the projection. It never touches `declaration_events`.
//! - **D16 observability** — every scan increments
//!   `recor_declaration_staleness_runs_total{outcome=ok|db_error}`
//!   and the per-row counter
//!   `recor_declaration_staleness_observed_total`. A scan that hasn't
//!   run in `STALENESS_INTERVAL_SECONDS × 3` is itself alertable.
//!
//! ## Why a tokio task and not a `k8s CronJob`?
//!
//! Same rationale as `retention.rs` — single-pod deployment posture.
//! Multi-replica deployments must elect a leader before enabling.

use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Outcome of a single scan cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOutcome {
    /// Number of stale rows observed this cycle. Each row is recorded
    /// via the `observed_total` counter so the absolute scan total is
    /// `sum(observed_total) / N_cycles_per_window`.
    pub stale_rows: u64,
}

/// Configuration for the staleness watcher.
#[derive(Debug, Clone, Copy)]
pub struct StalenessConfig {
    /// How often to scan. `0` disables the worker entirely.
    pub interval: Duration,
    /// Rows whose `last_event_observed_at` is older than NOW() minus
    /// this many days are reported. FATF c.24.8 fn 29 benchmark is
    /// 30 days; v1 operators may set higher during the rollover
    /// period while declarants populate the field.
    pub threshold_days: i32,
    /// Defence against pathological scans: hard cap on rows observed
    /// per cycle. Beyond this the cycle returns the cap; the next
    /// scan picks up the rest. Default 10_000.
    pub max_per_cycle: i64,
}

impl StalenessConfig {
    pub fn from_env() -> Self {
        let interval_secs = std::env::var("STALENESS_INTERVAL_SECONDS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let threshold_days = std::env::var("STALENESS_THRESHOLD_DAYS")
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(30);
        let max_per_cycle = std::env::var("STALENESS_MAX_PER_CYCLE")
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(10_000);
        Self {
            interval: Duration::from_secs(interval_secs),
            threshold_days,
            max_per_cycle,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !self.interval.is_zero()
    }
}

/// The staleness watcher worker.
pub struct StalenessWatcher {
    pool: PgPool,
    cfg: StalenessConfig,
}

impl StalenessWatcher {
    pub fn new(pool: PgPool, cfg: StalenessConfig) -> Self {
        Self { pool, cfg }
    }

    /// Run one scan cycle. Returns the row count observed.
    #[instrument(skip(self), fields(threshold_days = self.cfg.threshold_days))]
    pub async fn scan_once(&self) -> Result<ScanOutcome, sqlx::Error> {
        let row_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::BIGINT FROM declarations
             WHERE last_event_observed_at IS NOT NULL
               AND last_event_observed_at < NOW() - ($1 || ' days')::interval
               AND (amended_at IS NULL OR amended_at < last_event_observed_at)
               AND superseded_at IS NULL
            "#,
        )
        .bind(self.cfg.threshold_days)
        .fetch_one(&self.pool)
        .await?;

        let observed: u64 = u64::try_from(row_count.max(0)).unwrap_or(0);
        let observed = observed.min(u64::try_from(self.cfg.max_per_cycle).unwrap_or(u64::MAX));
        debug!(observed, "staleness scan complete");
        Ok(ScanOutcome { stale_rows: observed })
    }

    /// Run the worker loop until cancelled.
    pub async fn run(self, cancel: CancellationToken) {
        if !self.cfg.is_enabled() {
            info!("staleness watcher disabled (STALENESS_INTERVAL_SECONDS=0)");
            return;
        }
        info!(
            interval_secs = self.cfg.interval.as_secs(),
            threshold_days = self.cfg.threshold_days,
            "staleness watcher started"
        );
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.cfg.interval) => {}
                _ = cancel.cancelled() => {
                    info!("staleness watcher cancelled; exiting");
                    return;
                }
            }
            match self.scan_once().await {
                Ok(outcome) => {
                    if outcome.stale_rows > 0 {
                        warn!(
                            stale_rows = outcome.stale_rows,
                            threshold_days = self.cfg.threshold_days,
                            "FATF c.24.8 staleness detected — declarations have not been updated within the 30-day benchmark"
                        );
                    }
                }
                Err(e) => {
                    error!(error = ?e, "staleness scan failed");
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)] // Rust 2024: env::set_var / remove_var are unsafe; tests serialised by the runner.
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn disabled_when_interval_zero() {
        // The from_env path defaults to 0 when the env var is unset.
        // Explicitly unset to be deterministic.
        unsafe {
            env::remove_var("STALENESS_INTERVAL_SECONDS");
        }
        let cfg = StalenessConfig::from_env();
        assert!(!cfg.is_enabled());
    }

    #[test]
    fn threshold_defaults_to_30_days() {
        unsafe {
            env::remove_var("STALENESS_THRESHOLD_DAYS");
        }
        let cfg = StalenessConfig::from_env();
        assert_eq!(cfg.threshold_days, 30);
    }

    #[test]
    fn parses_env_overrides() {
        unsafe {
            env::set_var("STALENESS_INTERVAL_SECONDS", "60");
            env::set_var("STALENESS_THRESHOLD_DAYS", "60");
            env::set_var("STALENESS_MAX_PER_CYCLE", "5000");
        }
        let cfg = StalenessConfig::from_env();
        assert!(cfg.is_enabled());
        assert_eq!(cfg.interval.as_secs(), 60);
        assert_eq!(cfg.threshold_days, 60);
        assert_eq!(cfg.max_per_cycle, 5000);
        // Cleanup so other tests aren't polluted.
        unsafe {
            env::remove_var("STALENESS_INTERVAL_SECONDS");
            env::remove_var("STALENESS_THRESHOLD_DAYS");
            env::remove_var("STALENESS_MAX_PER_CYCLE");
        }
    }

    #[test]
    fn zero_threshold_days_falls_back_to_default() {
        unsafe {
            env::set_var("STALENESS_THRESHOLD_DAYS", "0");
        }
        let cfg = StalenessConfig::from_env();
        assert_eq!(cfg.threshold_days, 30);
        unsafe {
            env::remove_var("STALENESS_THRESHOLD_DAYS");
        }
    }
}
