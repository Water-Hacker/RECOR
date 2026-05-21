//! TODO-030 — Audit-reconciler integration tests.
//!
//! Exercises `ReconcilerLoop::run_once` against a chaincode mock that
//! returns scripted on-chain entries and a stub event-log repo. Asserts
//! the report correctly identifies matched / mismatch / missing-projection /
//! missing-onchain cases.
//!
//! No Docker needed — everything is in-memory. Run with:
//!   cargo test -p audit-reconciler --test fabric_reconciliation_integration

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use audit_verifier::fabric_client::{FabricClient, FabricClientError, OnChainEntry};
use time::OffsetDateTime;
use uuid::Uuid;

use audit_reconciler::metrics::ReconcilerMetrics;
use audit_reconciler::reconciler::{ReconcileError, ReconcilerLoop};
use audit_reconciler::repo::{EventLogRepo, EventLogRow};

// ─── Test doubles ─────────────────────────────────────────────────────────────

/// Scripted in-memory Fabric client. Calls to `list_for_declaration` return
/// whatever was registered; non-registered declaration_ids return empty.
#[derive(Default, Debug)]
struct ScriptedFabric {
    on_chain: Mutex<HashMap<String, Vec<OnChainEntry>>>,
    fail_next: Mutex<bool>,
}

impl ScriptedFabric {
    fn add_entry(&self, declaration_id: &str, entry: OnChainEntry) {
        self.on_chain
            .lock()
            .unwrap()
            .entry(declaration_id.to_string())
            .or_default()
            .push(entry);
    }
    fn set_fail(&self, v: bool) {
        *self.fail_next.lock().unwrap() = v;
    }
}

#[async_trait]
impl FabricClient for ScriptedFabric {
    async fn list_for_declaration(
        &self,
        declaration_id: &str,
    ) -> Result<Vec<OnChainEntry>, FabricClientError> {
        if *self.fail_next.lock().unwrap() {
            return Err(FabricClientError::Transport(
                "scripted fabric failure".into(),
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

/// Stub event-log repo backed by a mutex'd Vec.
#[derive(Default, Debug)]
struct StubRepo {
    rows: Mutex<Vec<EventLogRow>>,
}

impl StubRepo {
    fn push(&self, row: EventLogRow) {
        self.rows.lock().unwrap().push(row);
    }
}

#[async_trait]
impl EventLogRepo for StubRepo {
    async fn fetch_eligible(
        &self,
        _lookback: time::Duration,
        _grace_period: time::Duration,
        _limit: i64,
    ) -> Result<Vec<EventLogRow>, sqlx::Error> {
        Ok(self.rows.lock().unwrap().clone())
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn make_loop(
    repo: Arc<StubRepo>,
    fabric: Arc<ScriptedFabric>,
) -> (ReconcilerLoop, Arc<ReconcilerMetrics>) {
    let metrics = Arc::new(ReconcilerMetrics::new().expect("metrics registry"));
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

fn event_row(declaration_id: Uuid, event_type: &str) -> EventLogRow {
    EventLogRow {
        event_id: Uuid::now_v7(),
        declaration_id,
        event_type: event_type.to_string(),
        event_time: OffsetDateTime::now_utc(),
    }
}

fn on_chain_entry(event_id: Uuid, declaration_id: Uuid) -> OnChainEntry {
    OnChainEntry {
        event_id: event_id.to_string(),
        declaration_id: declaration_id.to_string(),
        receipt_hash_hex: "00".repeat(32),
        ts: "2026-05-01T00:00:00Z".to_string(),
        tx_id: "tx-stub".to_string(),
    }
}

// ─── Test 1: all events matched → 0 divergences ──────────────────────────────

#[tokio::test]
async fn matched_events_produce_zero_divergences() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());

    let decl = Uuid::now_v7();
    let row = event_row(decl, "declaration.submitted.v1");
    fabric.add_entry(&decl.to_string(), on_chain_entry(row.event_id, decl));
    repo.push(row);

    let (lp, metrics) = make_loop(repo, fabric);
    let outcome = lp.run_once().await.expect("pass ok");

    assert_eq!(outcome.divergences, 0);
    assert_eq!(outcome.events_examined, 1);
    assert_eq!(outcome.declarations_examined, 1);
    assert_eq!(metrics.runs_total.with_label_values(&["ok"]).get(), 1);
    assert_eq!(metrics.last_run_divergence_count.get(), 0);
}

// ─── Test 2: event in log but missing on chain → 1 divergence ────────────────

#[tokio::test]
async fn missing_onchain_event_counted_as_divergence() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());

    let decl = Uuid::now_v7();
    // Log has a row; Fabric returns nothing for this declaration.
    repo.push(event_row(decl, "declaration.submitted.v1"));

    let (lp, metrics) = make_loop(repo, fabric);
    let outcome = lp.run_once().await.expect("pass ok");

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

// ─── Test 3: multiple events same declaration — partial match ─────────────────

#[tokio::test]
async fn partial_match_within_declaration_counts_correctly() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());

    let decl = Uuid::now_v7();
    let r1 = event_row(decl, "declaration.submitted.v1");
    let r2 = event_row(decl, "declaration.amended.v1");
    // Only the first event is on chain.
    fabric.add_entry(&decl.to_string(), on_chain_entry(r1.event_id, decl));
    repo.push(r1);
    repo.push(r2);

    let (lp, _) = make_loop(repo, fabric);
    let outcome = lp.run_once().await.expect("pass ok");

    assert_eq!(outcome.events_examined, 2);
    assert_eq!(outcome.declarations_examined, 1);
    assert_eq!(outcome.divergences, 1, "only amended is missing");
}

// ─── Test 4: multiple declarations independently matched ──────────────────────

#[tokio::test]
async fn multiple_declarations_matched_independently() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());

    for _ in 0..3 {
        let decl = Uuid::now_v7();
        let row = event_row(decl, "declaration.submitted.v1");
        fabric.add_entry(&decl.to_string(), on_chain_entry(row.event_id, decl));
        repo.push(row);
    }

    let (lp, _) = make_loop(repo, fabric);
    let outcome = lp.run_once().await.expect("pass ok");

    assert_eq!(outcome.declarations_examined, 3);
    assert_eq!(outcome.divergences, 0);
}

// ─── Test 5: gateway failure → D14 fail-closed ────────────────────────────────

#[tokio::test]
async fn gateway_failure_fails_the_pass() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());
    fabric.set_fail(true);

    let decl = Uuid::now_v7();
    repo.push(event_row(decl, "declaration.submitted.v1"));

    let (lp, metrics) = make_loop(repo, fabric);
    let err = lp.run_once().await.expect_err("gateway failure must propagate");
    assert!(matches!(err, ReconcileError::Fabric(_)));
    assert_eq!(
        metrics.runs_total.with_label_values(&["gateway_error"]).get(),
        1
    );
}

// ─── Test 6: empty event log → no divergences, declarations_examined = 0 ──────

#[tokio::test]
async fn empty_log_produces_zero_divergences_and_zero_declarations() {
    let repo = Arc::new(StubRepo::default());
    let fabric = Arc::new(ScriptedFabric::default());

    let (lp, metrics) = make_loop(repo, fabric);
    let outcome = lp.run_once().await.expect("pass ok");

    assert_eq!(outcome.events_examined, 0);
    assert_eq!(outcome.declarations_examined, 0);
    assert_eq!(outcome.divergences, 0);
    assert_eq!(metrics.runs_total.with_label_values(&["ok"]).get(), 1);
}
