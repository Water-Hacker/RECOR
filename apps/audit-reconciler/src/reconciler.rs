//! The reconciliation loop. Pure-ish: takes ports (event-log repo +
//! Fabric client + metrics) and runs the diff. Test-friendly because
//! `ReconcilerLoop::run_once` is callable directly with in-memory
//! doubles.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use audit_verifier::fabric_client::{FabricClient, FabricClientError};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

use crate::metrics::ReconcilerMetrics;
use crate::repo::EventLogRepo;

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("event-log query failed: {0}")]
    EventLog(#[from] sqlx::Error),
    #[error("fabric gateway query failed: {0}")]
    Fabric(#[from] FabricClientError),
}

/// Per-pass summary surfaced to the caller (and to the metrics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// Total events examined across every declaration this pass.
    pub events_examined: usize,
    /// Number of (event_id, declaration_id) pairs found in the local
    /// log but absent from chaincode.
    pub divergences: usize,
    /// Distinct declarations the pass queried Fabric for.
    pub declarations_examined: usize,
}

pub struct ReconcilerLoop {
    repo: Arc<dyn EventLogRepo>,
    fabric: Arc<dyn FabricClient>,
    metrics: Arc<ReconcilerMetrics>,
    interval: Duration,
    grace_period: time::Duration,
    lookback: time::Duration,
    max_declarations_per_run: i64,
}

impl ReconcilerLoop {
    pub fn new(
        repo: Arc<dyn EventLogRepo>,
        fabric: Arc<dyn FabricClient>,
        metrics: Arc<ReconcilerMetrics>,
        interval: Duration,
        grace_period: Duration,
        lookback: Duration,
        max_declarations_per_run: i64,
    ) -> Self {
        Self {
            repo,
            fabric,
            metrics,
            interval,
            grace_period: time::Duration::seconds_f64(grace_period.as_secs_f64()),
            lookback: time::Duration::seconds_f64(lookback.as_secs_f64()),
            max_declarations_per_run,
        }
    }

    /// Long-running loop. Cancellation-token aware so an operator
    /// shutdown is graceful. Loops forever in the happy path,
    /// running `run_once` at `interval` cadence. Per-pass failures
    /// are LOGGED (D14 fail-closed) but DO NOT crash the loop —
    /// a transient Fabric gateway outage shouldn't take the cron
    /// off the air.
    pub async fn run(self: Arc<Self>, cancel: CancellationToken) {
        let mut ticker = tokio::time::interval(self.interval);
        // Skip the initial Instant::now() tick — start with a delay
        // so a freshly-deployed pod doesn't immediately query Fabric
        // before its sidecars are ready.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("audit-reconciler shutting down");
                    return;
                }
                _ = ticker.tick() => {
                    match self.run_once().await {
                        Ok(outcome) => {
                            info!(
                                events = outcome.events_examined,
                                declarations = outcome.declarations_examined,
                                divergences = outcome.divergences,
                                "reconciliation pass complete"
                            );
                        }
                        Err(e) => {
                            warn!(error = ?e, "reconciliation pass failed");
                        }
                    }
                }
            }
        }
    }

    /// Single reconciliation pass. Pure I/O dependency on the ports.
    /// Always increments `runs_total` exactly once before returning
    /// (D16 — every pass is observable).
    #[instrument(skip_all)]
    pub async fn run_once(&self) -> Result<ReconcileOutcome, ReconcileError> {
        let rows = match self
            .repo
            .fetch_eligible(self.lookback, self.grace_period, self.max_declarations_per_run)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                self.metrics
                    .runs_total
                    .with_label_values(&["db_error"])
                    .inc();
                return Err(e.into());
            }
        };

        // Group by declaration_id so we make at most ONE chaincode
        // query per declaration in this pass.
        use std::collections::HashMap;
        let mut by_decl: HashMap<uuid::Uuid, Vec<&crate::repo::EventLogRow>> =
            HashMap::new();
        for row in &rows {
            by_decl.entry(row.declaration_id).or_default().push(row);
        }
        let declarations_examined = by_decl.len();
        let events_examined = rows.len();

        let mut divergences: usize = 0;
        for (decl_id, local_events) in &by_decl {
            let onchain = match self.fabric.list_for_declaration(&decl_id.to_string()).await
            {
                Ok(v) => v,
                Err(e) => {
                    // D14 fail-closed: surface and abort the pass —
                    // we can't safely conclude "no divergence" when
                    // the on-chain side is unreachable.
                    self.metrics
                        .runs_total
                        .with_label_values(&["gateway_error"])
                        .inc();
                    return Err(e.into());
                }
            };
            let onchain_ids: HashSet<String> =
                onchain.into_iter().map(|e| e.event_id).collect();

            for ev in local_events {
                if !onchain_ids.contains(&ev.event_id.to_string()) {
                    divergences += 1;
                    self.metrics
                        .divergence_total
                        .with_label_values(&[ev.event_type.as_str()])
                        .inc();
                    warn!(
                        declaration_id = %decl_id,
                        event_id = %ev.event_id,
                        event_type = %ev.event_type,
                        event_time = %ev.event_time,
                        "audit-chain divergence — event present in declaration_events but absent from chaincode"
                    );
                }
            }
        }

        self.metrics
            .last_run_divergence_count
            .set(divergences as i64);
        self.metrics
            .runs_total
            .with_label_values(&["ok"])
            .inc();

        Ok(ReconcileOutcome {
            events_examined,
            divergences,
            declarations_examined,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use audit_verifier::fabric_client::{
        FabricClient, FabricClientError, OnChainEntry,
    };
    use std::sync::Mutex;
    use time::OffsetDateTime;
    use uuid::Uuid;

    /// In-memory Fabric double — returns whatever's been registered
    /// for the given declaration_id; returns empty otherwise.
    #[derive(Default, Debug)]
    struct StubFabric {
        on_chain: Mutex<std::collections::HashMap<String, Vec<OnChainEntry>>>,
        fail_next: Mutex<bool>,
    }

    impl StubFabric {
        fn add(&self, declaration_id: &str, entry: OnChainEntry) {
            self.on_chain
                .lock()
                .unwrap()
                .entry(declaration_id.to_string())
                .or_default()
                .push(entry);
        }
        fn set_fail(&self, fail: bool) {
            *self.fail_next.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl FabricClient for StubFabric {
        async fn list_for_declaration(
            &self,
            declaration_id: &str,
        ) -> Result<Vec<OnChainEntry>, FabricClientError> {
            if *self.fail_next.lock().unwrap() {
                return Err(FabricClientError::Transport(
                    "stub fabric — failing as configured".into(),
                ));
            }
            Ok(self
                .on_chain
                .lock()
                .unwrap()
                .get(declaration_id)
                .cloned()
                .unwrap_or_default())
        }
    }

    /// Mirror of `repo::tests::InMemoryEventLog` — duplicated here
    /// because the reconciler crate doesn't want a test-only public
    /// dependency surface on the repo module.
    #[derive(Default, Debug)]
    struct StubRepo {
        rows: Mutex<Vec<crate::repo::EventLogRow>>,
    }

    impl StubRepo {
        fn push(&self, row: crate::repo::EventLogRow) {
            self.rows.lock().unwrap().push(row);
        }
    }

    #[async_trait]
    impl crate::repo::EventLogRepo for StubRepo {
        async fn fetch_eligible(
            &self,
            _lookback: time::Duration,
            _grace_period: time::Duration,
            _limit: i64,
        ) -> Result<Vec<crate::repo::EventLogRow>, sqlx::Error> {
            Ok(self.rows.lock().unwrap().clone())
        }
    }

    fn make_loop(
        repo: Arc<StubRepo>,
        fabric: Arc<StubFabric>,
    ) -> (ReconcilerLoop, Arc<ReconcilerMetrics>) {
        let metrics = Arc::new(ReconcilerMetrics::new().expect("metrics"));
        let lp = ReconcilerLoop::new(
            repo,
            fabric,
            metrics.clone(),
            Duration::from_secs(1),
            Duration::from_secs(0),
            Duration::from_secs(3600),
            100,
        );
        (lp, metrics)
    }

    fn row(declaration_id: Uuid, event_type: &str) -> crate::repo::EventLogRow {
        crate::repo::EventLogRow {
            event_id: Uuid::now_v7(),
            declaration_id,
            event_type: event_type.to_string(),
            event_time: OffsetDateTime::now_utc(),
        }
    }

    fn onchain(event_id: Uuid, declaration_id: Uuid) -> OnChainEntry {
        OnChainEntry {
            event_id: event_id.to_string(),
            declaration_id: declaration_id.to_string(),
            receipt_hash_hex: "00".repeat(32),
            ts: "2026-05-01T00:00:00Z".to_string(),
            tx_id: "tx-stub".to_string(),
        }
    }

    #[tokio::test]
    async fn happy_path_no_divergence_when_chain_matches_log() {
        let repo = Arc::new(StubRepo::default());
        let fabric = Arc::new(StubFabric::default());

        let decl = Uuid::now_v7();
        let r = row(decl, "declaration.submitted.v1");
        fabric.add(&decl.to_string(), onchain(r.event_id, decl));
        repo.push(r);

        let (lp, metrics) = make_loop(repo, fabric);
        let outcome = lp.run_once().await.expect("pass completes");
        assert_eq!(outcome.divergences, 0);
        assert_eq!(outcome.events_examined, 1);
        assert_eq!(outcome.declarations_examined, 1);
        // `runs_total{outcome="ok"}` incremented exactly once.
        assert_eq!(
            metrics.runs_total.with_label_values(&["ok"]).get(),
            1
        );
    }

    #[tokio::test]
    async fn divergence_is_counted_and_logged_when_event_missing_onchain() {
        let repo = Arc::new(StubRepo::default());
        let fabric = Arc::new(StubFabric::default());

        let decl = Uuid::now_v7();
        // Local event has no on-chain counterpart.
        repo.push(row(decl, "declaration.submitted.v1"));

        let (lp, metrics) = make_loop(repo, fabric);
        let outcome = lp.run_once().await.expect("pass completes");
        assert_eq!(outcome.divergences, 1);
        assert_eq!(
            metrics
                .divergence_total
                .with_label_values(&["declaration.submitted.v1"])
                .get(),
            1
        );
        assert_eq!(metrics.last_run_divergence_count.get(), 1);
    }

    #[tokio::test]
    async fn gateway_failure_fails_the_pass_fail_closed() {
        let repo = Arc::new(StubRepo::default());
        let fabric = Arc::new(StubFabric::default());
        fabric.set_fail(true);
        let decl = Uuid::now_v7();
        repo.push(row(decl, "declaration.submitted.v1"));

        let (lp, metrics) = make_loop(repo, fabric);
        let err = lp
            .run_once()
            .await
            .expect_err("gateway failure must surface as ReconcileError");
        assert!(matches!(err, ReconcileError::Fabric(_)));
        // Outcome counter records the gateway-error path.
        assert_eq!(
            metrics
                .runs_total
                .with_label_values(&["gateway_error"])
                .get(),
            1
        );
    }

    #[tokio::test]
    async fn multiple_events_in_same_declaration_share_one_chain_query() {
        // Two events for the same declaration; only one is on chain.
        let repo = Arc::new(StubRepo::default());
        let fabric = Arc::new(StubFabric::default());
        let decl = Uuid::now_v7();
        let r1 = row(decl, "declaration.submitted.v1");
        let r2 = row(decl, "declaration.amended.v1");
        fabric.add(&decl.to_string(), onchain(r1.event_id, decl));
        repo.push(r1);
        repo.push(r2);

        let (lp, _) = make_loop(repo, fabric);
        let outcome = lp.run_once().await.expect("pass completes");
        assert_eq!(outcome.events_examined, 2);
        assert_eq!(outcome.declarations_examined, 1);
        assert_eq!(outcome.divergences, 1);
    }
}
