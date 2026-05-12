//! `worker-fabric-bridge` — anchor declaration events to Fabric.
//!
//! The worker exposes:
//!
//! - An **inbound HTTP receiver** at `POST /v1/relay` that the
//!   Declaration service's outbox-relay forwards events to (the same
//!   shape as the verification-engine relay endpoint, with HMAC-SHA256
//!   verification). When R-LOOP-2 ships the Kafka transport, the worker
//!   can additionally consume from a topic — the receiver and the Kafka
//!   consumer share the `process_event` entry point so the transport
//!   choice is environmentally configurable.
//!
//! - A **health + metrics surface** at `GET /healthz` (liveness) and
//!   `GET /metrics` (OBS-1).
//!
//! For every accepted event whose `event_type` is in `ANCHORABLE_EVENTS`,
//! the worker:
//!
//! 1. Extracts `event_id`, `declaration_id`, `receipt_hash_hex`, and
//!    `ts` from the payload.
//! 2. Calls `FabricBridge::commit_audit_entry`.
//! 3. On `Ok(Committed)` / `Ok(AlreadyCommitted)`: records success +
//!    returns 200.
//! 4. On `Err(Permanent)` / `Err(NonRetryable)`: writes the row to
//!    `fabric_bridge_dlq` and returns 200 (the relay's contract is
//!    "delivery acknowledged"; the DLQ is the durable forensic record).
//!    On `Err(Config)` returns 500 because the worker is misconfigured.
//!
//! See `docs/runbooks/fabric-bridge.md` for operator procedures.

pub mod config;
pub mod dlq;
pub mod handlers;
pub mod metrics;
pub mod processor;

pub use config::WorkerConfig;
pub use metrics::WorkerMetrics;
pub use processor::{EventEnvelope, EventProcessor, ProcessOutcome};

/// Event-type discriminators the worker anchors. Other event types
/// arriving on the relay are acknowledged-and-ignored (the relay is
/// fan-out; only a subset of consumers care about each event_type).
pub const ANCHORABLE_EVENTS: &[&str] = &[
    "declaration.submitted.v1",
    "declaration.amended.v1",
    "declaration.corrected.v1",
    "declaration.superseded.v1",
];

/// Returns true if the event_type is one we anchor.
pub fn is_anchorable(event_type: &str) -> bool {
    ANCHORABLE_EVENTS.contains(&event_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchorable_set_contains_all_lifecycle_events() {
        for kind in [
            "declaration.submitted.v1",
            "declaration.amended.v1",
            "declaration.corrected.v1",
            "declaration.superseded.v1",
        ] {
            assert!(is_anchorable(kind), "missing: {kind}");
        }
    }

    #[test]
    fn anchorable_set_excludes_non_declaration_events() {
        assert!(!is_anchorable("declaration.verified.v1"));
        assert!(!is_anchorable("verification.case.completed.v1"));
        assert!(!is_anchorable(""));
    }
}
