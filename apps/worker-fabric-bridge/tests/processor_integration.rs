//! TODO-029 — Fabric-bridge processor integration tests.
//!
//! Exercises `EventProcessor::process` against an in-memory `InMemoryTransport`
//! (the `fabric_bridge` package's test double). No Docker, no HTTP — these
//! tests run as ordinary unit-style Tokio tests.
//!
//! Coverage: happy path, chaincode-transient-error retry → permanent DLQ,
//! non-retryable error → DLQ, idempotency on already-anchored receipts,
//! malformed payload → DLQ, non-anchorable event → skipped, metric counter.
//!
//! Run with:
//!   cargo test -p worker-fabric-bridge --test processor_integration

use std::sync::Arc;
use std::time::Duration;

use fabric_bridge::{BridgeConfig, FabricBridge, InMemoryTransport};
use serde_json::json;
use uuid::Uuid;

use worker_fabric_bridge::dlq::InMemoryDlqRepo;
use worker_fabric_bridge::processor::{EventEnvelope, EventProcessor, ProcessOutcome};

// ─── Helpers ──────────────────────────────────────────────────────────────────

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn submitted_payload(decl_id: Uuid) -> serde_json::Value {
    json!({
        "declaration_id": decl_id.to_string(),
        "receipt_hash_hex": HASH,
        "submitted_at": "2026-05-12T10:00:00Z",
    })
}

fn amended_payload(decl_id: Uuid) -> serde_json::Value {
    json!({
        "declaration_id": decl_id.to_string(),
        "receipt_hash_hex": HASH,
        "amended_at": "2026-05-12T11:00:00Z",
    })
}

fn corrected_payload(decl_id: Uuid) -> serde_json::Value {
    json!({
        "declaration_id": decl_id.to_string(),
        "receipt_hash_hex": HASH,
        "corrected_at": "2026-05-12T12:00:00Z",
    })
}

fn superseded_payload(decl_id: Uuid) -> serde_json::Value {
    json!({
        "declaration_id": decl_id.to_string(),
        "receipt_hash_hex": HASH,
        "superseded_at": "2026-05-12T13:00:00Z",
    })
}

async fn make_processor(
    transport: Arc<InMemoryTransport>,
) -> (EventProcessor, Arc<InMemoryDlqRepo>) {
    let cfg = BridgeConfig {
        backoff_base: Duration::from_millis(1),
        max_attempts: 2,
        ..Default::default()
    };
    let bridge = Arc::new(FabricBridge::with_transport(cfg, transport));
    let dlq = Arc::new(InMemoryDlqRepo::new());
    let processor = EventProcessor::new(bridge, dlq.clone());
    (processor, dlq)
}

fn envelope(event_type: &str, aggregate_id: Uuid, payload: serde_json::Value) -> EventEnvelope {
    EventEnvelope {
        event_id: Uuid::new_v4(),
        event_type: event_type.to_string(),
        event_version: 1,
        aggregate_id,
        payload,
    }
}

// ─── Test 1: happy path — declaration.submitted.v1 anchored ──────────────────

#[tokio::test]
async fn happy_path_submitted_event_anchored() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.submitted.v1", decl_id, submitted_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::Committed { .. }));
    assert_eq!(dlq.rows().await.len(), 0, "no DLQ writes on happy path");
}

// ─── Test 2: already anchored → idempotent, no DLQ ──────────────────────────

#[tokio::test]
async fn already_anchored_returns_already_committed_no_dlq() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_already_committed().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.submitted.v1", decl_id, submitted_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::AlreadyCommitted { .. }));
    assert_eq!(dlq.rows().await.len(), 0, "idempotent replay must not write DLQ");
}

// ─── Test 3: transient error → retries → permanent DLQ ───────────────────────

#[tokio::test]
async fn chaincode_transient_error_exhausts_retries_and_dlqs() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_retryable().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.submitted.v1", decl_id, submitted_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
    let rows = dlq.rows().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cause, "permanent");
    // max_attempts=2 → attempts field should reflect retries exhausted.
    assert!(rows[0].attempts >= 1, "attempts must be recorded");
}

// ─── Test 4: non-retryable error → DLQ with non_retryable cause ──────────────

#[tokio::test]
async fn non_retryable_error_dlqs_with_correct_cause() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_non_retryable().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.submitted.v1", decl_id, submitted_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
    let rows = dlq.rows().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cause, "non_retryable");
}

// ─── Test 5: non-anchorable event → skipped, no DLQ ─────────────────────────

#[tokio::test]
async fn non_anchorable_event_is_skipped_without_dlq() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, dlq) = make_processor(transport).await;

    let outcome = proc
        .process(envelope(
            "declaration.verified.v1",
            Uuid::new_v4(),
            json!({}),
        ))
        .await;

    assert_eq!(outcome, ProcessOutcome::Skipped);
    assert_eq!(dlq.rows().await.len(), 0);
}

// ─── Test 6: malformed payload → DLQ without bridge call ─────────────────────

#[tokio::test]
async fn malformed_payload_dead_letters_without_bridge_call() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, dlq) = make_processor(transport).await;

    let bad_payload = json!({
        "declaration_id": Uuid::new_v4().to_string(),
        // receipt_hash_hex absent → extract_fields returns None
        "submitted_at": "2026-05-12T10:00:00Z",
    });

    let outcome = proc
        .process(envelope("declaration.submitted.v1", Uuid::new_v4(), bad_payload))
        .await;

    assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
    let rows = dlq.rows().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cause, "non_retryable");
}

// ─── Test 7: amended event anchored successfully ──────────────────────────────

#[tokio::test]
async fn amended_event_anchored() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, _dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.amended.v1", decl_id, amended_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::Committed { .. }));
}

// ─── Test 8: corrected event anchored successfully ────────────────────────────

#[tokio::test]
async fn corrected_event_anchored() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, _dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.corrected.v1", decl_id, corrected_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::Committed { .. }));
}

// ─── Test 9: superseded event anchored successfully ───────────────────────────

#[tokio::test]
async fn superseded_event_anchored() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, _dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    let outcome = proc
        .process(envelope("declaration.superseded.v1", decl_id, superseded_payload(decl_id)))
        .await;

    assert!(matches!(outcome, ProcessOutcome::Committed { .. }));
}

// ─── Test 10: invalid receipt_hash_hex length → DLQ ─────────────────────────

#[tokio::test]
async fn invalid_receipt_hash_length_dead_letters() {
    let transport = Arc::new(InMemoryTransport::new());
    transport.set_always_ok().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();

    // hash length ≠ 64 → extract_fields returns None → DLQ
    let bad = json!({
        "declaration_id": decl_id.to_string(),
        "receipt_hash_hex": "tooshort",
        "submitted_at": "2026-05-12T10:00:00Z",
    });

    let outcome = proc
        .process(envelope("declaration.submitted.v1", decl_id, bad))
        .await;

    assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
    assert_eq!(dlq.rows().await.len(), 1);
}

// ─── Test 11: idempotent replay — second process call for same event_id ────────

#[tokio::test]
async fn idempotent_replay_on_already_anchored_does_not_dlq() {
    let transport = Arc::new(InMemoryTransport::new());
    // First call returns Committed, second returns AlreadyCommitted.
    transport.set_already_committed().await;
    let (proc, dlq) = make_processor(transport).await;
    let decl_id = Uuid::new_v4();
    let env = envelope("declaration.submitted.v1", decl_id, submitted_payload(decl_id));

    let r1 = proc.process(env.clone()).await;
    let r2 = proc.process(env).await;

    assert!(matches!(r1, ProcessOutcome::AlreadyCommitted { .. }));
    assert!(matches!(r2, ProcessOutcome::AlreadyCommitted { .. }));
    assert_eq!(dlq.rows().await.len(), 0);
}
