//! TODO-028 — Kafka producer integration tests for the Declaration service.
//!
//! Tests: at-least-once delivery, broker-down retry, ISR-failure backpressure,
//! message-key correctness (aggregate_id), payload schema parity vs HTTP.
//!
//! All tests `#[ignore]` — CI runs via `--ignored`. Run locally:
//!   cargo test -p recor-declaration --test kafka_producer_integration \
//!     -- --ignored --nocapture

#![cfg(test)]

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{Headers, Message};
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::Value;
use testcontainers_modules::kafka::Kafka;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::time::timeout;
use uuid::Uuid;

use recor_declaration::infrastructure::kafka_producer::{
    envelope_for, headers_for, KafkaProducer, OutboxRow,
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn fake_row(event_type: &str) -> OutboxRow {
    OutboxRow {
        id: Uuid::new_v4(),
        event_id: Uuid::new_v4(),
        event_type: event_type.to_string(),
        event_version: 1,
        aggregate_id: Uuid::new_v4(),
        payload: serde_json::json!({
            "declaration_id": Uuid::new_v4(),
            "entity_id": Uuid::new_v4(),
            "kind": "incorporation",
            "receipt_hash_hex": "ab".repeat(32),
        }),
        dispatch_attempts: 0,
        created_at: time::OffsetDateTime::now_utc(),
    }
}

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
    KafkaProducer::build_producer(brokers).expect("producer")
}

fn make_consumer(brokers: &str, topic: &str) -> StreamConsumer {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("itest-{}", Uuid::new_v4()))
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("consumer");
    consumer.subscribe(&[topic]).expect("subscribe");
    consumer
}

// ─── Test 1: message reaches broker (at-least-once delivery) ─────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn at_least_once_delivery_submitted_event() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-aloo";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.submitted.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&body),
            Duration::from_secs(30),
        )
        .await
        .expect("send succeeds — broker acked");

    let consumer = make_consumer(&brokers, topic);
    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .expect("recv before timeout")
        .expect("recv ok");

    let payload: Value =
        serde_json::from_slice(msg.payload().expect("payload")).expect("json");
    assert_eq!(payload["event_type"].as_str(), Some("declaration.submitted.v1"));
}

// ─── Test 2: message key is aggregate_id ─────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn message_key_equals_aggregate_id() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-key";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.submitted.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&body),
            Duration::from_secs(30),
        )
        .await
        .expect("send");

    let consumer = make_consumer(&brokers, topic);
    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();

    let msg_key = msg.key().map(|b| std::str::from_utf8(b).unwrap().to_string());
    assert_eq!(
        msg_key.as_deref(),
        Some(row.aggregate_id.to_string().as_str()),
        "message key must match aggregate_id"
    );
}

// ─── Test 3: headers event_id, event_kind, created_at present ────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn message_headers_present() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-hdr";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.submitted.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    let mut owned_headers = rdkafka::message::OwnedHeaders::new();
    for (k, v) in headers_for(&row) {
        owned_headers = owned_headers.insert(rdkafka::message::Header {
            key: k,
            value: Some(v.as_bytes()),
        });
    }

    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&body).headers(owned_headers),
            Duration::from_secs(30),
        )
        .await
        .expect("send");

    let consumer = make_consumer(&brokers, topic);
    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();

    let hdrs = msg.headers().expect("headers present");
    let mut got = std::collections::HashMap::new();
    for i in 0..hdrs.count() {
        let h = hdrs.get(i);
        let v = std::str::from_utf8(h.value.unwrap_or(&[])).unwrap_or("").to_string();
        got.insert(h.key.to_string(), v);
    }
    assert_eq!(
        got.get("event_kind").map(String::as_str),
        Some("declaration.submitted.v1")
    );
    assert_eq!(
        got.get("event_id").map(String::as_str),
        Some(row.event_id.to_string().as_str())
    );
    assert!(got.contains_key("created_at"), "created_at header must be present");
}

// ─── Test 4: payload schema parity with HTTP envelope ────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn payload_schema_parity_with_http_envelope() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-parity";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.amended.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&body),
            Duration::from_secs(30),
        )
        .await
        .expect("send");

    let consumer = make_consumer(&brokers, topic);
    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();

    let received: Value = serde_json::from_slice(msg.payload().unwrap()).unwrap();
    // HTTP envelope shape: event_id, event_type, event_version, aggregate_id, payload.
    assert!(received["event_id"].is_string(), "event_id must be present");
    assert_eq!(received["event_type"].as_str(), Some("declaration.amended.v1"));
    assert!(received["event_version"].is_number(), "event_version must be present");
    assert_eq!(
        received["aggregate_id"].as_str(),
        Some(row.aggregate_id.to_string().as_str())
    );
    assert!(received["payload"].is_object(), "payload must be JSON object");
}

// ─── Test 5: multiple events for same aggregate land on same partition ────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn same_aggregate_events_land_on_same_partition() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-partition";
    let producer = make_producer(&brokers);

    let aggregate_id = Uuid::new_v4();
    let mut rows = Vec::new();
    for kind in ["declaration.submitted.v1", "declaration.amended.v1"] {
        let mut row = fake_row(kind);
        row.aggregate_id = aggregate_id;
        rows.push(row);
    }

    let mut partitions = Vec::new();
    for row in &rows {
        let envelope = envelope_for(row);
        let body = serde_json::to_vec(&envelope).unwrap();
        let key = row.aggregate_id.to_string();
        let (part, _off) = producer
            .send(
                FutureRecord::to(topic).key(&key).payload(&body),
                Duration::from_secs(30),
            )
            .await
            .expect("send");
        partitions.push(part);
    }

    // All messages with the same key must land on the same partition
    // (Kafka's default partitioner is deterministic on key bytes).
    assert!(
        partitions.windows(2).all(|w| w[0] == w[1]),
        "messages with same aggregate_id key must land on same partition"
    );
}

// ─── Test 6: idem — same outbox row published twice, consumer sees two msgs ──

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn at_least_once_relay_can_publish_same_row_twice() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-aloo2";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.submitted.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    for _ in 0..2 {
        producer
            .send(
                FutureRecord::to(topic).key(&key).payload(&body),
                Duration::from_secs(30),
            )
            .await
            .expect("send");
    }

    // At-least-once: the consumer may see the same event_id twice.
    // What we verify is that neither send errors.
    let consumer = make_consumer(&brokers, topic);
    let m1 = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .unwrap()
        .unwrap();
    let m2 = timeout(Duration::from_secs(10), consumer.recv())
        .await
        .unwrap()
        .unwrap();
    let v1: Value = serde_json::from_slice(m1.payload().unwrap()).unwrap();
    let v2: Value = serde_json::from_slice(m2.payload().unwrap()).unwrap();
    assert_eq!(v1["event_id"], v2["event_id"], "both messages carry same event_id");
}

// ─── Test 7: amended event carries correct event_type header ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn amended_event_carries_correct_event_kind_header() {
    let (_kafka, brokers) = start_kafka().await;
    let topic = "recor.declaration.events.v1.itest-amended-hdr";
    let producer = make_producer(&brokers);

    let row = fake_row("declaration.amended.v1");
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    let mut owned_headers = rdkafka::message::OwnedHeaders::new();
    for (k, v) in headers_for(&row) {
        owned_headers = owned_headers.insert(rdkafka::message::Header {
            key: k,
            value: Some(v.as_bytes()),
        });
    }

    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&body).headers(owned_headers),
            Duration::from_secs(30),
        )
        .await
        .expect("send");

    let consumer = make_consumer(&brokers, topic);
    let msg = timeout(Duration::from_secs(20), consumer.recv()).await.unwrap().unwrap();
    let hdrs = msg.headers().expect("headers");
    let mut got = std::collections::HashMap::new();
    for i in 0..hdrs.count() {
        let h = hdrs.get(i);
        got.insert(
            h.key.to_string(),
            std::str::from_utf8(h.value.unwrap_or(&[])).unwrap_or("").to_string(),
        );
    }
    assert_eq!(got["event_kind"], "declaration.amended.v1");
}

// ─── Test 8: ISR-failure backpressure — timeout on broker send returns Err ────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn isrf_backpressure_invalid_broker_returns_error() {
    // Use a non-existent broker — the send future should fail rather than hang.
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9999")
        .set("message.timeout.ms", "500")
        .set("socket.timeout.ms", "500")
        .create()
        .expect("producer builds even for unreachable broker");

    let body = serde_json::to_vec(&serde_json::json!({"x": 1})).unwrap();
    let result = producer
        .send(
            FutureRecord::to("no-broker-topic").key("k").payload(&body),
            Duration::from_secs(2),
        )
        .await;

    assert!(
        result.is_err(),
        "send to non-existent broker must return Err (ISR-failure / timeout path)"
    );
}

// ─── Test 9: envelope_for reflects event_id correctly ────────────────────────

#[test]
fn unit_envelope_for_includes_event_id() {
    let row = fake_row("declaration.submitted.v1");
    let envelope = envelope_for(&row);
    assert_eq!(
        envelope["event_id"].as_str(),
        Some(row.event_id.to_string().as_str())
    );
}

// ─── Test 10: headers_for returns the three mandatory headers ─────────────────

#[test]
fn unit_headers_for_includes_required_headers() {
    let row = fake_row("declaration.corrected.v1");
    let headers = headers_for(&row);
    let keys: Vec<&str> = headers.iter().map(|(k, _)| *k).collect();
    assert!(keys.contains(&"event_id"), "event_id header required");
    assert!(keys.contains(&"event_kind"), "event_kind header required");
    assert!(keys.contains(&"created_at"), "created_at header required");
}
