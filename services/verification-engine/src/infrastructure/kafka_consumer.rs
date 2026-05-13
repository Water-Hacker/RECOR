//! R-LOOP-2 — Kafka consumer for declaration events.
//!
//! Sibling of `crate::api::internal::handle_declaration_event` (the
//! HTTP webhook). The consumer reads messages from
//! `recor.declaration.events.v1`, deserialises the envelope into a
//! [`DeclarationSnapshot`], and feeds it through the same
//! [`SubmitVerificationUseCase`] the HTTP handler calls.
//!
//! ## Delivery semantics
//!
//! - **At-least-once delivery.** Offsets are committed only after the
//!   use case returns `Ok`. A crash mid-process replays the message.
//! - **Idempotency at apply time.** `SubmitVerificationUseCase` is
//!   idempotent on `event_id`/`declaration_id` (existing invariant
//!   from the HTTP path). Replays are absorbed without double-
//!   applying state — see `services/verification-engine/src/application/submit_verification.rs`.
//! - **Bounded retries.** A use-case error retries with exponential
//!   backoff up to `max_retries`; beyond that the message is dead-
//!   lettered to `kafka_consumer_dlq` and the offset committed.
//! - **Parse errors are permanent.** A message that does not
//!   deserialise into the expected envelope shape goes straight to
//!   the DLQ — no amount of retry helps a schema regression.
//!
//! ## Cross-cutting
//!
//! - D14 fail-closed: a poisoned message lands in the DLQ and the
//!   consumer moves on. The topic is never blocked on one bad message.
//! - D16 observability: every poll updates `recor_kafka_consume_total`
//!   (per-result counter) and `recor_kafka_consume_lag_seconds` (gauge
//!   sampled per message based on broker-stamped consume time).
//! - D17 zero trust: production deployments wire SASL+mTLS on the
//!   broker; this skeleton uses PLAINTEXT for the dev compose.

use std::sync::Arc;
use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::{BorrowedMessage, Headers, Message};
use serde::Deserialize;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::application::SubmitVerificationUseCase;
use crate::domain::declaration_snapshot::{DeclarationSnapshot, OwnerSnapshot};

/// Envelope shape on the wire. Matches the declaration service's
/// kafka_producer / outbox-relay envelope byte-for-byte.
#[derive(Debug, Deserialize)]
pub struct InboundEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: i32,
    pub aggregate_id: Uuid,
    pub payload: serde_json::Value,
}

/// Subset of the declaration-side wire DTO we need to build a
/// `DeclarationSnapshot`. Mirrors `api::internal::DeclarationSubmittedV1Wire`
/// — duplicated here so the consumer module is self-contained for
/// the cutover. A follow-up may consolidate the two into a shared crate.
#[derive(Debug, Deserialize)]
pub struct DeclarationSubmittedV1Wire {
    pub declaration_id: Uuid,
    pub entity_id: Uuid,
    pub declarant_principal: String,
    pub declarant_role: String,
    pub kind: String,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerWire>,
    pub attestation: AttestationWire,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: time::OffsetDateTime,
    pub correlation_id: Uuid,
    pub receipt_hash_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct BeneficialOwnerWire {
    pub person_id: Uuid,
    pub ownership_basis_points: u32,
    pub interest_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct AttestationWire {
    pub signed_by: String,
    pub signature_algorithm: String,
    pub signature_hex: String,
    pub public_key_hex: String,
    pub nonce_hex: String,
}

/// Outcome of consuming one message.
#[derive(Debug, PartialEq, Eq)]
pub enum ConsumeOutcome {
    /// Applied to the use case successfully — offset will be committed.
    Applied,
    /// Re-applied a previously-seen event_id (idempotency replay).
    /// Offset is still committed.
    Idempotent,
    /// Parse failure — message dead-lettered, offset committed so we
    /// don't get stuck.
    ParsedToDlq,
    /// Use-case error after exhausting retries — dead-lettered and
    /// offset committed.
    UsecaseToDlq,
}

/// Convert a wire payload to the domain snapshot the use case takes.
pub fn snapshot_from_wire(payload: DeclarationSubmittedV1Wire) -> DeclarationSnapshot {
    DeclarationSnapshot {
        declaration_id: payload.declaration_id,
        entity_id: payload.entity_id,
        declarant_principal: payload.declarant_principal,
        declarant_role: payload.declarant_role,
        kind: payload.kind,
        effective_from: payload.effective_from,
        beneficial_owners: payload
            .beneficial_owners
            .into_iter()
            .map(|o| OwnerSnapshot {
                person_id: o.person_id,
                ownership_basis_points: o.ownership_basis_points,
                interest_kind: o.interest_kind,
            })
            .collect(),
        attestation_signed_by: payload.attestation.signed_by,
        attestation_signature_hex: payload.attestation.signature_hex,
        attestation_public_key_hex: payload.attestation.public_key_hex,
        receipt_hash_hex: payload.receipt_hash_hex,
        correlation_id: payload.correlation_id,
        submitted_at: payload.submitted_at,
    }
}

/// Two-step parse: envelope → declaration payload → snapshot. Returns
/// either the snapshot + the envelope-level event_id (for idempotency
/// + DLQ correlation), or a structured failure that the consumer
/// turns into a DLQ row.
pub enum ParseResult {
    Ok {
        event_id: Uuid,
        snapshot: DeclarationSnapshot,
    },
    /// Envelope-level parse failure (whole message is unintelligible).
    EnvelopeFailure {
        message: String,
    },
    /// Envelope parsed but it carries an event we don't handle. The
    /// consumer treats this as a successful no-op (commits offset)
    /// rather than DLQ — same shape as the HTTP handler's 202.
    Skipped {
        event_id: Uuid,
        event_type: String,
    },
    /// Envelope parsed but the inner payload was malformed.
    PayloadFailure {
        event_id: Uuid,
        message: String,
    },
}

pub fn parse_message_bytes(bytes: &[u8]) -> ParseResult {
    let envelope: InboundEnvelope = match serde_json::from_slice(bytes) {
        Ok(e) => e,
        Err(e) => {
            return ParseResult::EnvelopeFailure {
                message: format!("envelope parse: {e}"),
            };
        }
    };
    if envelope.event_type != "declaration.submitted.v1" {
        return ParseResult::Skipped {
            event_id: envelope.event_id,
            event_type: envelope.event_type,
        };
    }
    let payload: DeclarationSubmittedV1Wire = match serde_json::from_value(envelope.payload) {
        Ok(p) => p,
        Err(e) => {
            return ParseResult::PayloadFailure {
                event_id: envelope.event_id,
                message: format!("payload parse: {e}"),
            };
        }
    };
    ParseResult::Ok {
        event_id: envelope.event_id,
        snapshot: snapshot_from_wire(payload),
    }
}

/// The Kafka consumer for declaration events.
pub struct KafkaConsumer {
    consumer: StreamConsumer,
    topic: String,
    pool: PgPool,
    submit_usecase: Arc<SubmitVerificationUseCase>,
    max_retries: u32,
    /// Initial backoff between in-process retries; doubled per attempt.
    initial_backoff: Duration,
    /// Optional metrics handle.
    metrics: Option<Arc<crate::metrics::Metrics>>,
}

impl KafkaConsumer {
    /// Build a [`StreamConsumer`] with the required group_id +
    /// at-least-once settings. Auto-commit is OFF; we commit only
    /// after the use case applies successfully.
    pub fn build_consumer(
        brokers: &str,
        group_id: &str,
    ) -> Result<StreamConsumer, rdkafka::error::KafkaError> {
        ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            // Start from the beginning on a fresh consumer group so the
            // smoke test doesn't miss the initial message. Production
            // operators set group.id explicitly and own the offset
            // lifecycle.
            .set("auto.offset.reset", "earliest")
            .set("session.timeout.ms", "30000")
            .set("max.poll.interval.ms", "300000")
            .create()
    }

    pub fn new(
        consumer: StreamConsumer,
        topic: impl Into<String>,
        pool: PgPool,
        submit_usecase: Arc<SubmitVerificationUseCase>,
    ) -> Self {
        Self {
            consumer,
            topic: topic.into(),
            pool,
            submit_usecase,
            max_retries: 5,
            initial_backoff: Duration::from_millis(200),
            metrics: None,
        }
    }

    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Subscribe to the configured topic. Required before `run`.
    pub fn subscribe(&self) -> Result<(), rdkafka::error::KafkaError> {
        self.consumer.subscribe(&[self.topic.as_str()])
    }

    /// Run until the cancellation token fires.
    #[instrument(skip_all, fields(topic = %self.topic))]
    pub async fn run(&self, cancel: CancellationToken) {
        info!(topic = %self.topic, "kafka consumer started");
        if let Err(e) = self.subscribe() {
            error!(error = ?e, "subscribe failed; consumer exiting");
            return;
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("kafka consumer shutting down");
                    return;
                }
                msg = self.consumer.recv() => {
                    match msg {
                        Ok(m) => {
                            let outcome = self.handle_message(&m).await;
                            debug!(?outcome, "kafka consume outcome");
                            // Commit the offset regardless of outcome:
                            // - Applied / Idempotent: success path.
                            // - ParsedToDlq / UsecaseToDlq: we wrote a
                            //   DLQ row and the message is no longer
                            //   in flight; committing prevents an
                            //   endless replay loop.
                            if let Err(e) = self.consumer.commit_message(&m, CommitMode::Async) {
                                warn!(error = ?e, "offset commit failed");
                            }
                        }
                        Err(e) => {
                            warn!(error = ?e, "consumer poll error");
                        }
                    }
                }
            }
        }
    }

    /// Process a single message. Pulled out so the unit + integration
    /// tests can drive the consumer without a long-running loop.
    pub async fn handle_message(&self, msg: &BorrowedMessage<'_>) -> ConsumeOutcome {
        let payload = match msg.payload() {
            Some(p) => p,
            None => {
                // A tombstone or empty payload — treat as parse error
                // for forensics so an operator can investigate why.
                let _ = self
                    .dead_letter(msg, None, b"", "parse_error", "empty payload", 0)
                    .await;
                self.note_consume(msg, "dlq");
                return ConsumeOutcome::ParsedToDlq;
            }
        };

        let parsed = parse_message_bytes(payload);
        match parsed {
            ParseResult::EnvelopeFailure { message } => {
                let _ = self
                    .dead_letter(msg, None, payload, "parse_error", &message, 0)
                    .await;
                self.note_consume(msg, "dlq");
                ConsumeOutcome::ParsedToDlq
            }
            ParseResult::PayloadFailure { event_id, message } => {
                let _ = self
                    .dead_letter(
                        msg,
                        Some(event_id),
                        payload,
                        "parse_error",
                        &message,
                        0,
                    )
                    .await;
                self.note_consume(msg, "dlq");
                ConsumeOutcome::ParsedToDlq
            }
            ParseResult::Skipped { event_id, event_type } => {
                debug!(
                    %event_id, event_type = %event_type,
                    "skipping non-declaration event"
                );
                self.note_consume(msg, "skipped");
                ConsumeOutcome::Idempotent
            }
            ParseResult::Ok { event_id, snapshot } => {
                self.apply_with_retry(msg, event_id, snapshot, payload).await
            }
        }
    }

    async fn apply_with_retry(
        &self,
        msg: &BorrowedMessage<'_>,
        event_id: Uuid,
        snapshot: DeclarationSnapshot,
        raw: &[u8],
    ) -> ConsumeOutcome {
        let mut attempt: u32 = 0;
        let mut last_error: String = String::new();
        while attempt <= self.max_retries {
            match self.submit_usecase.execute(snapshot.clone()).await {
                Ok(case) => {
                    info!(
                        %event_id,
                        case_id = %case.case_id,
                        lane = case.lane.as_str(),
                        attempt,
                        "kafka consumer applied event"
                    );
                    self.note_consume(msg, "applied");
                    return ConsumeOutcome::Applied;
                }
                Err(e) => {
                    last_error = format!("usecase: {e}");
                    warn!(%event_id, attempt, error = %e, "kafka consumer apply failed");
                    attempt += 1;
                    if attempt > self.max_retries {
                        break;
                    }
                    let backoff = self.initial_backoff * 2u32.pow(attempt - 1);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
        // Exhausted retries — DLQ.
        let _ = self
            .dead_letter(
                msg,
                Some(event_id),
                raw,
                "retry_exhausted",
                &last_error,
                attempt.saturating_sub(1) as i32,
            )
            .await;
        self.note_consume(msg, "dlq");
        ConsumeOutcome::UsecaseToDlq
    }

    /// Persist a DLQ row. INSERT ... ON CONFLICT (event_id) DO NOTHING so
    /// a duplicate write (across consumer restarts) collapses cleanly.
    async fn dead_letter(
        &self,
        msg: &BorrowedMessage<'_>,
        event_id: Option<Uuid>,
        raw_payload: &[u8],
        failure_kind: &str,
        last_error: &str,
        retry_attempts: i32,
    ) -> Result<(), sqlx::Error> {
        let id = Uuid::now_v7();
        let topic = msg.topic().to_string();
        let partition = msg.partition();
        let offset = msg.offset();
        let consumed_at = msg
            .timestamp()
            .to_millis()
            .and_then(|ms| time::OffsetDateTime::from_unix_timestamp_nanos(
                (ms as i128) * 1_000_000,
            )
            .ok())
            .unwrap_or_else(time::OffsetDateTime::now_utc);
        let truncated_error: String = last_error.chars().take(8 * 1024).collect();

        let payload_json: Option<serde_json::Value> =
            serde_json::from_slice(raw_payload).ok();

        let res = sqlx::query(
            r#"
            INSERT INTO kafka_consumer_dlq (
                id, event_id, topic, partition, "offset",
                payload, raw_payload,
                failure_kind, last_error, retry_attempts,
                consumed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(event_id)
        .bind(&topic)
        .bind(partition)
        .bind(offset)
        .bind(payload_json)
        .bind(raw_payload)
        .bind(failure_kind)
        .bind(&truncated_error)
        .bind(retry_attempts)
        .bind(consumed_at)
        .execute(&self.pool)
        .await;

        if let Err(e) = &res {
            error!(error = ?e, %topic, partition, offset, "kafka DLQ insert failed");
        } else {
            warn!(
                %topic, partition, offset, failure_kind,
                event_id = ?event_id,
                "kafka message dead-lettered"
            );
        }
        res.map(|_| ())
    }

    /// OBS-1: bump the consume counter + sample the lag gauge.
    fn note_consume(&self, msg: &BorrowedMessage<'_>, result: &str) {
        if let Some(m) = self.metrics.as_ref() {
            m.kafka_consume_total
                .with_label_values(&[result])
                .inc();
            // Sample broker→consumer lag as wall-clock seconds.
            if let Some(broker_ms) = msg.timestamp().to_millis() {
                if let Ok(broker_ts) = time::OffsetDateTime::from_unix_timestamp_nanos(
                    (broker_ms as i128) * 1_000_000,
                ) {
                    let now = time::OffsetDateTime::now_utc();
                    let lag = (now - broker_ts).as_seconds_f64().max(0.0);
                    m.kafka_consume_lag_seconds.set(lag);
                }
            }
        }
        // Header walk for tracing only — does not affect routing.
        if tracing::enabled!(tracing::Level::DEBUG) {
            if let Some(hdrs) = msg.headers() {
                for i in 0..hdrs.count() {
                    let h = hdrs.get(i);
                    let v = h
                        .value
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .unwrap_or("");
                    debug!(header_key = %h.key, header_value = %v, "kafka header");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_envelope_bytes(decl_id: Uuid) -> Vec<u8> {
        let envelope = serde_json::json!({
            "event_id": Uuid::nil(),
            "event_type": "declaration.submitted.v1",
            "event_version": 1,
            "aggregate_id": decl_id,
            "payload": {
                "declaration_id": decl_id,
                "entity_id": Uuid::nil(),
                "declarant_principal": "spiffe://recor.cm/test",
                "declarant_role": "self",
                "kind": "incorporation",
                "effective_from": "2026-01-01",
                "beneficial_owners": [
                    {
                        "person_id": Uuid::nil(),
                        "ownership_basis_points": 10_000u32,
                        "interest_kind": "equity"
                    }
                ],
                "attestation": {
                    "signed_by": "spiffe://recor.cm/test",
                    "signature_algorithm": "ed25519",
                    "signature_hex": "deadbeef",
                    "public_key_hex": "cafebabe",
                    "nonce_hex": "01020304"
                },
                "submitted_at": "2026-05-12T10:00:00Z",
                "correlation_id": Uuid::nil(),
                "receipt_hash_hex": "00"
            }
        });
        serde_json::to_vec(&envelope).unwrap()
    }

    #[test]
    fn parse_message_bytes_handles_a_well_formed_envelope() {
        let decl_id = Uuid::new_v4();
        let bytes = good_envelope_bytes(decl_id);
        match parse_message_bytes(&bytes) {
            ParseResult::Ok { event_id: _, snapshot } => {
                assert_eq!(snapshot.declaration_id, decl_id);
                assert_eq!(snapshot.kind, "incorporation");
                assert_eq!(snapshot.beneficial_owners.len(), 1);
                assert_eq!(
                    snapshot.beneficial_owners[0].ownership_basis_points,
                    10_000
                );
            }
            other => panic!("expected Ok, got {other:?}", other = match other {
                ParseResult::EnvelopeFailure { message } => format!("EnvelopeFailure({message})"),
                ParseResult::PayloadFailure { message, .. } => format!("PayloadFailure({message})"),
                ParseResult::Skipped { event_type, .. } => format!("Skipped({event_type})"),
                ParseResult::Ok { .. } => unreachable!(),
            }),
        }
    }

    #[test]
    fn parse_message_bytes_skips_unknown_event_types() {
        let envelope = serde_json::json!({
            "event_id": Uuid::nil(),
            "event_type": "declaration.something_else.v1",
            "event_version": 1,
            "aggregate_id": Uuid::nil(),
            "payload": {}
        });
        let bytes = serde_json::to_vec(&envelope).unwrap();
        match parse_message_bytes(&bytes) {
            ParseResult::Skipped { event_type, .. } => {
                assert_eq!(event_type, "declaration.something_else.v1");
            }
            _ => panic!("expected Skipped"),
        }
    }

    #[test]
    fn parse_message_bytes_envelope_failure_on_garbage() {
        match parse_message_bytes(b"not json") {
            ParseResult::EnvelopeFailure { message } => {
                assert!(message.contains("envelope parse"));
            }
            _ => panic!("expected EnvelopeFailure"),
        }
    }

    #[test]
    fn parse_message_bytes_payload_failure_on_malformed_payload() {
        let envelope = serde_json::json!({
            "event_id": Uuid::nil(),
            "event_type": "declaration.submitted.v1",
            "event_version": 1,
            "aggregate_id": Uuid::nil(),
            "payload": {"declaration_id": "not-a-uuid"}
        });
        let bytes = serde_json::to_vec(&envelope).unwrap();
        match parse_message_bytes(&bytes) {
            ParseResult::PayloadFailure { event_id: _, message } => {
                assert!(message.contains("payload parse"));
            }
            _ => panic!("expected PayloadFailure"),
        }
    }

    #[test]
    fn snapshot_from_wire_preserves_owner_basis_points() {
        let decl_id = Uuid::new_v4();
        let bytes = good_envelope_bytes(decl_id);
        let env: InboundEnvelope = serde_json::from_slice(&bytes).unwrap();
        let payload: DeclarationSubmittedV1Wire =
            serde_json::from_value(env.payload).unwrap();
        let snap = snapshot_from_wire(payload);
        let total: u32 = snap.beneficial_owners.iter().map(|o| o.ownership_basis_points).sum();
        assert_eq!(total, 10_000);
    }
}
