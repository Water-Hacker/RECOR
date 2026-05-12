//! Core event processor — transport-agnostic.
//!
//! The HTTP receiver and the (future) Kafka consumer both call into
//! `EventProcessor::process` with a normalised `EventEnvelope`. This is
//! where the bridge interaction + DLQ routing lives; it's exhaustively
//! unit-testable without any HTTP plumbing.

use std::sync::Arc;

use fabric_bridge::{BridgeError, CommitOutcome, FabricBridge};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::dlq::{DlqRepo, DlqRow};
use crate::is_anchorable;

/// Envelope shape the relay POSTs (matches the declaration service's
/// outbox-relay body — see services/declaration/src/infrastructure/relay.rs).
#[derive(Debug, Clone, Deserialize)]
pub struct EventEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: i32,
    pub aggregate_id: Uuid,
    pub payload: JsonValue,
}

/// Outcome of one `process()` call. The HTTP handler maps these to
/// 200/4xx/5xx as appropriate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// Anchored on the chain in this call.
    Committed { tx_id: String },
    /// Already anchored prior; no-op.
    AlreadyCommitted { tx_id: String },
    /// Event type not in the anchorable set; acknowledged-and-ignored.
    Skipped,
    /// Bridge failure; the row has been written to the DLQ. Returned to
    /// the relay as 200 so it stops retrying — the DLQ is now the
    /// durable record.
    DeadLettered { cause: String },
    /// Transient error that the worker could not recover. Returned as
    /// 503 so the relay retries.
    Retryable { message: String },
}

#[derive(Debug)]
pub struct EventProcessor {
    bridge: Arc<FabricBridge>,
    dlq: Arc<dyn DlqRepo>,
    dlq_writes_counter: Option<prometheus::CounterVec>,
}

impl EventProcessor {
    pub fn new(bridge: Arc<FabricBridge>, dlq: Arc<dyn DlqRepo>) -> Self {
        Self {
            bridge,
            dlq,
            dlq_writes_counter: None,
        }
    }

    pub fn with_dlq_metric(mut self, dlq_writes: prometheus::CounterVec) -> Self {
        self.dlq_writes_counter = Some(dlq_writes);
        self
    }

    #[instrument(skip_all, fields(event_id = %envelope.event_id, event_type = %envelope.event_type))]
    pub async fn process(&self, envelope: EventEnvelope) -> ProcessOutcome {
        if !is_anchorable(&envelope.event_type) {
            debug!("event type not anchored; skipping");
            return ProcessOutcome::Skipped;
        }

        let (decl_id, receipt_hash, ts) = match extract_fields(&envelope.payload) {
            Some(f) => f,
            None => {
                // Payload doesn't carry the expected fields. This is a
                // contract violation between Declaration and the bridge
                // — the relay can't fix it by retrying. DLQ and move on.
                let cause = "non_retryable";
                error!("payload missing required fields for anchoring");
                if let Err(e) = self
                    .dlq
                    .insert(DlqRow {
                        event_id: envelope.event_id,
                        event_type: envelope.event_type.clone(),
                        aggregate_id: envelope.aggregate_id,
                        payload: envelope.payload,
                        attempts: 0,
                        last_error: "payload missing required fields".to_string(),
                        cause: cause.to_string(),
                    })
                    .await
                {
                    error!(error = %e, "DLQ insert failed");
                    return ProcessOutcome::Retryable {
                        message: format!("DLQ insert failed: {e}"),
                    };
                }
                if let Some(c) = self.dlq_writes_counter.as_ref() {
                    c.with_label_values(&[cause]).inc();
                }
                return ProcessOutcome::DeadLettered {
                    cause: cause.to_string(),
                };
            }
        };

        let event_id_s = envelope.event_id.to_string();
        let decl_id_s = decl_id.to_string();

        match self
            .bridge
            .commit_audit_entry(&event_id_s, &decl_id_s, &receipt_hash, &ts)
            .await
        {
            Ok(CommitOutcome::Committed(tx)) => {
                info!(tx_id = %tx, "anchored event to Fabric");
                ProcessOutcome::Committed {
                    tx_id: tx.to_string(),
                }
            }
            Ok(CommitOutcome::AlreadyCommitted(tx)) => {
                info!(tx_id = %tx, "already anchored (idempotent)");
                ProcessOutcome::AlreadyCommitted {
                    tx_id: tx.to_string(),
                }
            }
            Err(BridgeError::Permanent { attempts, source }) => {
                warn!(attempts, error = %source, "permanent bridge failure; DLQ");
                let cause = "permanent";
                if let Err(e) = self
                    .dlq
                    .insert(DlqRow {
                        event_id: envelope.event_id,
                        event_type: envelope.event_type.clone(),
                        aggregate_id: envelope.aggregate_id,
                        payload: envelope.payload,
                        attempts: attempts as i32,
                        last_error: source.to_string(),
                        cause: cause.to_string(),
                    })
                    .await
                {
                    error!(error = %e, "DLQ insert failed");
                    return ProcessOutcome::Retryable {
                        message: format!("DLQ insert failed: {e}"),
                    };
                }
                if let Some(c) = self.dlq_writes_counter.as_ref() {
                    c.with_label_values(&[cause]).inc();
                }
                ProcessOutcome::DeadLettered {
                    cause: cause.to_string(),
                }
            }
            Err(BridgeError::NonRetryable(msg)) => {
                warn!(error = %msg, "non-retryable bridge failure; DLQ");
                let cause = "non_retryable";
                if let Err(e) = self
                    .dlq
                    .insert(DlqRow {
                        event_id: envelope.event_id,
                        event_type: envelope.event_type.clone(),
                        aggregate_id: envelope.aggregate_id,
                        payload: envelope.payload,
                        attempts: 0,
                        last_error: msg,
                        cause: cause.to_string(),
                    })
                    .await
                {
                    error!(error = %e, "DLQ insert failed");
                    return ProcessOutcome::Retryable {
                        message: format!("DLQ insert failed: {e}"),
                    };
                }
                if let Some(c) = self.dlq_writes_counter.as_ref() {
                    c.with_label_values(&[cause]).inc();
                }
                ProcessOutcome::DeadLettered {
                    cause: cause.to_string(),
                }
            }
            Err(BridgeError::Config(msg)) => {
                error!(error = %msg, "bridge config error; will not DLQ");
                ProcessOutcome::Retryable { message: msg }
            }
        }
    }
}

/// Extract the three fields the chaincode needs from a declaration
/// event payload. Returns None if any are missing or malformed.
fn extract_fields(payload: &JsonValue) -> Option<(Uuid, String, String)> {
    let obj = payload.as_object()?;
    let decl_id = obj.get("declaration_id").and_then(|v| v.as_str())?;
    let receipt = obj.get("receipt_hash_hex").and_then(|v| v.as_str())?;
    // The Submitted event uses `submitted_at`; Amended uses `amended_at`;
    // Corrected uses `corrected_at`; Superseded uses `superseded_at`. We
    // prefer the most specific then fall back. The chaincode treats ts
    // as an opaque RFC3339 string, so as long as we pick one it's valid.
    let ts = obj
        .get("submitted_at")
        .or_else(|| obj.get("amended_at"))
        .or_else(|| obj.get("corrected_at"))
        .or_else(|| obj.get("superseded_at"))
        .and_then(|v| v.as_str())?;
    let decl_uuid = Uuid::parse_str(decl_id).ok()?;
    if receipt.len() != 64 {
        return None;
    }
    Some((decl_uuid, receipt.to_string(), ts.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlq::InMemoryDlqRepo;
    use fabric_bridge::{BridgeConfig, FabricBridge, InMemoryTransport};
    use serde_json::json;
    use std::time::Duration;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn submitted_payload(decl_id: Uuid) -> JsonValue {
        json!({
            "declaration_id": decl_id.to_string(),
            "receipt_hash_hex": HASH,
            "submitted_at": "2026-05-12T10:00:00Z",
        })
    }

    fn make_processor(behaviour: &str) -> (EventProcessor, Arc<InMemoryDlqRepo>) {
        let transport = Arc::new(InMemoryTransport::new());
        // Synchronously apply behaviour pre-construction using a tokio
        // current-thread runtime spawn.
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            match behaviour {
                "ok" => transport.set_always_ok().await,
                "already" => transport.set_already_committed().await,
                "retryable" => transport.set_always_retryable().await,
                "non_retryable" => transport.set_non_retryable().await,
                _ => unreachable!(),
            }
        });
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

    #[tokio::test]
    async fn skips_non_anchorable_event() {
        let (p, dlq) = make_processor("ok");
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.verified.v1".to_string(),
            event_version: 1,
            aggregate_id: Uuid::new_v4(),
            payload: json!({}),
        };
        let outcome = p.process(env).await;
        assert_eq!(outcome, ProcessOutcome::Skipped);
        assert_eq!(dlq.rows().await.len(), 0);
    }

    #[tokio::test]
    async fn anchors_submitted_event() {
        let (p, dlq) = make_processor("ok");
        let decl_id = Uuid::new_v4();
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.submitted.v1".into(),
            event_version: 1,
            aggregate_id: decl_id,
            payload: submitted_payload(decl_id),
        };
        let outcome = p.process(env).await;
        assert!(matches!(outcome, ProcessOutcome::Committed { .. }));
        assert_eq!(dlq.rows().await.len(), 0);
    }

    #[tokio::test]
    async fn idempotent_replay_succeeds_without_dlq() {
        let (p, dlq) = make_processor("already");
        let decl_id = Uuid::new_v4();
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.submitted.v1".into(),
            event_version: 1,
            aggregate_id: decl_id,
            payload: submitted_payload(decl_id),
        };
        let outcome = p.process(env).await;
        assert!(matches!(outcome, ProcessOutcome::AlreadyCommitted { .. }));
        assert_eq!(dlq.rows().await.len(), 0);
    }

    #[tokio::test]
    async fn permanent_failure_writes_to_dlq() {
        let (p, dlq) = make_processor("retryable");
        let decl_id = Uuid::new_v4();
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.submitted.v1".into(),
            event_version: 1,
            aggregate_id: decl_id,
            payload: submitted_payload(decl_id),
        };
        let outcome = p.process(env).await;
        assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
        let rows = dlq.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cause, "permanent");
        assert_eq!(rows[0].attempts, 2);
    }

    #[tokio::test]
    async fn non_retryable_writes_to_dlq_with_correct_cause() {
        let (p, dlq) = make_processor("non_retryable");
        let decl_id = Uuid::new_v4();
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.amended.v1".into(),
            event_version: 1,
            aggregate_id: decl_id,
            payload: json!({
                "declaration_id": decl_id.to_string(),
                "receipt_hash_hex": HASH,
                "amended_at": "2026-05-12T11:00:00Z",
            }),
        };
        let outcome = p.process(env).await;
        assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
        let rows = dlq.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cause, "non_retryable");
    }

    #[tokio::test]
    async fn malformed_payload_dead_letters_without_bridge_call() {
        let (p, dlq) = make_processor("ok");
        let env = EventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "declaration.submitted.v1".into(),
            event_version: 1,
            aggregate_id: Uuid::new_v4(),
            // Missing receipt_hash_hex.
            payload: json!({
                "declaration_id": Uuid::new_v4().to_string(),
                "submitted_at": "2026-05-12T10:00:00Z",
            }),
        };
        let outcome = p.process(env).await;
        assert!(matches!(outcome, ProcessOutcome::DeadLettered { .. }));
        let rows = dlq.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cause, "non_retryable");
    }

    #[tokio::test]
    async fn extract_fields_prefers_submitted_at() {
        let decl_id = Uuid::new_v4();
        let payload = json!({
            "declaration_id": decl_id.to_string(),
            "receipt_hash_hex": HASH,
            "submitted_at": "S",
            "amended_at": "A",
        });
        let (_d, _h, ts) = extract_fields(&payload).unwrap();
        assert_eq!(ts, "S");
    }

    #[tokio::test]
    async fn extract_fields_falls_back_to_amended_at() {
        let decl_id = Uuid::new_v4();
        let payload = json!({
            "declaration_id": decl_id.to_string(),
            "receipt_hash_hex": HASH,
            "amended_at": "A",
        });
        let (_d, _h, ts) = extract_fields(&payload).unwrap();
        assert_eq!(ts, "A");
    }
}
