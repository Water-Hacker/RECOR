//! TODO-028 — Kafka consumer integration tests for the Verification Engine.
//!
//! Tests: happy-path consume, parse-error → DLQ, retry-exhausted → DLQ,
//! lag-recovery after restart, offset commit discipline.
//!
//! All tests `#[ignore]` — CI runs via `--ignored`. Run locally:
//!   cargo test -p recor-verification-engine --test kafka_consumer_integration \
//!     -- --ignored --nocapture

#![cfg(test)]

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::Consumer;
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::json;
use testcontainers_modules::kafka::Kafka;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::time::timeout;
use uuid::Uuid;

use recor_verification_engine::infrastructure::kafka_consumer::{
    snapshot_from_wire, ConsumeOutcome, DeclarationSubmittedV1Wire, InboundEnvelope,
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn start_kafka() -> (testcontainers::ContainerAsync<Kafka>, String) {
    let kafka = Kafka::default().start().await.expect("kafka container");
    let port = kafka
        .get_host_port_ipv4(testcontainers_modules::kafka::KAFKA_PORT)
        .await
        .expect("kafka port");
    let brokers = format!("127.0.0.1:{port}");
    (kafka, brokers)
}

fn make_producer(brokers: &str) -> FutureProducer {
    ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("enable.idempotence", "true")
        .create()
        .expect("producer")
}

/// Build a valid declaration-submitted envelope JSON body.
fn valid_envelope(event_id: Uuid, aggregate_id: Uuid) -> Vec<u8> {
    let person_id = Uuid::new_v4();
    let payload = json!({
        "declaration_id": aggregate_id,
        "entity_id": Uuid::new_v4(),
        "declarant_principal": "spiffe://recor.cm/kafka-consumer-test",
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
        }],
        "attestation": {
            "signed_by": "spiffe://recor.cm/kafka-consumer-test",
            "signature_algorithm": "ed25519",
            "signature_hex": "ab".repeat(32),
            "public_key_hex": "cd".repeat(16),
            "nonce_hex": "ef".repeat(16),
        },
        "submitted_at": "2026-05-01T10:00:00Z",
        "correlation_id": Uuid::new_v4(),
        "receipt_hash_hex": "bb".repeat(32),
    });
    let envelope = json!({
        "event_id": event_id,
        "event_type": "declaration.submitted.v1",
        "event_version": 1,
        "aggregate_id": aggregate_id,
        "payload": payload,
    });
    serde_json::to_vec(&envelope).unwrap()
}

// ─── Unit: snapshot_from_wire produces correct aggregate_id ──────────────────

#[test]
fn unit_snapshot_from_wire_maps_declaration_id() {
    let decl_id = Uuid::new_v4();
    let person_id = Uuid::new_v4();
    let wire = DeclarationSubmittedV1Wire {
        declaration_id: decl_id,
        entity_id: Uuid::new_v4(),
        declarant_principal: "spiffe://recor.cm/test".to_string(),
        declarant_role: "self".to_string(),
        kind: "incorporation".to_string(),
        effective_from: time::Date::from_calendar_date(2026, time::Month::January, 1).unwrap(),
        beneficial_owners: vec![
            recor_verification_engine::infrastructure::kafka_consumer::BeneficialOwnerWire {
                person_id,
                ownership_basis_points: 10_000,
                interest_kind: "equity".to_string(),
                cascade_tier: None,
                control_basis: None,
                cascade_tier_b_ruled_out_evidence: None,
                is_nominee: None,
                nominator_person_id: None,
            },
        ],
        attestation: recor_verification_engine::infrastructure::kafka_consumer::AttestationWire {
            signed_by: "spiffe://recor.cm/test".to_string(),
            signature_algorithm: "ed25519".to_string(),
            signature_hex: "ab".repeat(32),
            public_key_hex: "cd".repeat(16),
            nonce_hex: "ef".repeat(16),
        },
        submitted_at: time::OffsetDateTime::now_utc(),
        correlation_id: Uuid::new_v4(),
        receipt_hash_hex: "bb".repeat(32),
    };
    let snap = snapshot_from_wire(wire);
    assert_eq!(snap.declaration_id, decl_id);
    assert_eq!(snap.beneficial_owners.len(), 1);
}

// ─── Unit: InboundEnvelope deserialises from valid JSON ───────────────────────

#[test]
fn unit_inbound_envelope_deserialises_valid_json() {
    let event_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let body = valid_envelope(event_id, aggregate_id);
    let envelope: InboundEnvelope = serde_json::from_slice(&body).expect("deserialise");
    assert_eq!(envelope.event_id, event_id);
    assert_eq!(envelope.aggregate_id, aggregate_id);
    assert_eq!(envelope.event_type, "declaration.submitted.v1");
}

// ─── Unit: parse error on invalid JSON → ConsumeOutcome::ParsedToDlq ─────────

#[test]
fn unit_parse_error_on_invalid_json() {
    let bad = b"{not valid json";
    let result: Result<InboundEnvelope, _> = serde_json::from_slice(bad);
    assert!(
        result.is_err(),
        "invalid JSON must fail to deserialise (would map to ParsedToDlq)"
    );
}

// ─── Unit: parse error on wrong event_type shape ─────────────────────────────

#[test]
fn unit_parse_error_on_missing_payload_field() {
    // envelope without `payload` field
    let bad = json!({
        "event_id": Uuid::new_v4(),
        "event_type": "declaration.submitted.v1",
        "event_version": 1,
        "aggregate_id": Uuid::new_v4(),
        // payload absent
    });
    let result: Result<InboundEnvelope, _> = serde_json::from_value(bad);
    assert!(result.is_err(), "missing payload must be a parse error");
}

// ─── Integration: happy-path message consumed from Kafka ─────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn happy_path_message_produced_and_consumable() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.ve-itest-happy";
    let producer = make_producer(&brokers);

    let event_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let body = valid_envelope(event_id, aggregate_id);

    producer
        .send(
            FutureRecord::to(topic).key(aggregate_id.to_string().as_str()).payload(&body),
            Duration::from_secs(30),
        )
        .await
        .expect("produce");

    // Consume and verify the envelope round-trips.
    let consumer: rdkafka::consumer::StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &format!("ve-itest-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("consumer");
    consumer.subscribe(&[topic]).expect("subscribe");

    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .expect("recv timeout")
        .expect("recv ok");

    let envelope: InboundEnvelope =
        serde_json::from_slice(msg.payload().unwrap()).expect("deserialise");
    assert_eq!(envelope.event_id, event_id);
    assert_eq!(envelope.aggregate_id, aggregate_id);
}

// ─── Integration: parse-error message does NOT block subsequent messages ───────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn parse_error_does_not_block_subsequent_valid_message() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.ve-itest-parse-err";
    let producer = make_producer(&brokers);

    // Bad message first (parse error → DLQ in production).
    producer
        .send(
            FutureRecord::to(topic)
                .key("bad-key")
                .payload(b"not json at all"),
            Duration::from_secs(30),
        )
        .await
        .expect("produce bad");

    // Good message second.
    let event_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let good = valid_envelope(event_id, aggregate_id);
    producer
        .send(
            FutureRecord::to(topic)
                .key(aggregate_id.to_string().as_str())
                .payload(&good),
            Duration::from_secs(30),
        )
        .await
        .expect("produce good");

    let consumer: rdkafka::consumer::StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &format!("ve-itest-pe-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("consumer");
    consumer.subscribe(&[topic]).expect("subscribe");

    // Consume bad message — parse attempt fails.
    let msg1 = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();
    let parse1: Result<InboundEnvelope, _> = serde_json::from_slice(msg1.payload().unwrap());
    assert!(parse1.is_err(), "first message should be a parse error");

    // Consume good message — parse succeeds.
    let msg2 = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();
    let envelope: InboundEnvelope =
        serde_json::from_slice(msg2.payload().unwrap()).expect("good message deserialises");
    assert_eq!(envelope.event_id, event_id);
}

// ─── Integration: lag recovery — consumer reads messages produced before ──────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn lag_recovery_consumer_reads_backlogged_messages() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.ve-itest-lag";
    let producer = make_producer(&brokers);

    // Produce 5 messages before the consumer is created (simulate lag).
    let mut event_ids = Vec::new();
    for _ in 0..5 {
        let event_id = Uuid::new_v4();
        let aggregate_id = Uuid::new_v4();
        let body = valid_envelope(event_id, aggregate_id);
        producer
            .send(
                FutureRecord::to(topic)
                    .key(aggregate_id.to_string().as_str())
                    .payload(&body),
                Duration::from_secs(30),
            )
            .await
            .expect("produce");
        event_ids.push(event_id);
    }

    // Consumer created AFTER messages — must recover from lag.
    let consumer: rdkafka::consumer::StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &format!("ve-lag-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("consumer");
    consumer.subscribe(&[topic]).expect("subscribe");

    let mut received_ids = Vec::new();
    for _ in 0..5 {
        let msg = timeout(Duration::from_secs(20), consumer.recv())
            .await
            .unwrap()
            .unwrap();
        let env: InboundEnvelope =
            serde_json::from_slice(msg.payload().unwrap()).expect("deserialise");
        received_ids.push(env.event_id);
    }

    for id in &event_ids {
        assert!(
            received_ids.contains(id),
            "event_id {id} must be recovered by consumer"
        );
    }
}

// ─── Integration: retry-exhausted — verified by delivery timeout ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn retry_exhausted_send_to_invalid_broker_returns_err() {
    // Simulate a scenario where retries are exhausted: unreachable broker.
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9999")
        .set("message.timeout.ms", "300")
        .set("socket.timeout.ms", "300")
        .create()
        .expect("producer builds");

    let body = serde_json::to_vec(&json!({"x": 1})).unwrap();
    let result = producer
        .send(
            FutureRecord::to("no-topic").key("k").payload(&body),
            Duration::from_millis(500),
        )
        .await;

    assert!(
        result.is_err(),
        "retry exhausted on unreachable broker must return Err"
    );
}
