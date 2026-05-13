//! R-LOOP-2 — Kafka producer/consumer round-trip integration test.
//!
//! Spins up a single-broker Kafka via testcontainers, builds a real
//! `FutureProducer`, publishes one declaration-events envelope keyed
//! by `aggregate_id`, then consumes it back with a `StreamConsumer`
//! and asserts:
//!   1. the message reaches the topic (broker ack)
//!   2. the key matches `aggregate_id`
//!   3. the payload bytes deserialise to the same envelope shape
//!   4. headers `event_id`, `event_kind`, `created_at` are present
//!
//! Gated `#[ignore]` because it needs a Docker daemon. Run with:
//!     cargo test --test kafka_integration -- --ignored --nocapture

#![cfg(test)]

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{Headers, Message};
use rdkafka::producer::{FutureProducer, FutureRecord};
use testcontainers_modules::kafka::Kafka;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::time::timeout;

use recor_declaration::infrastructure::kafka_producer::{
    envelope_for, headers_for, OutboxRow,
};

fn fake_row() -> OutboxRow {
    OutboxRow {
        id: uuid::Uuid::new_v4(),
        event_id: uuid::Uuid::new_v4(),
        event_type: "declaration.submitted.v1".to_string(),
        event_version: 1,
        aggregate_id: uuid::Uuid::new_v4(),
        payload: serde_json::json!({
            "declaration_id": uuid::Uuid::new_v4(),
            "kind": "incorporation",
        }),
        dispatch_attempts: 0,
        created_at: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
#[ignore]
async fn kafka_round_trip_preserves_envelope_key_and_headers() {
    let kafka = Kafka::default()
        .start()
        .await
        .expect("kafka container starts");
    let host_port = kafka
        .get_host_port_ipv4(testcontainers_modules::kafka::KAFKA_PORT)
        .await
        .expect("kafka port");
    let brokers = format!("127.0.0.1:{host_port}");
    let topic = "recor.declaration.events.v1.itest";

    // Build the producer using the same factory the production code uses.
    let producer: FutureProducer =
        recor_declaration::infrastructure::kafka_producer::KafkaProducer::build_producer(
            &brokers,
        )
        .expect("producer builds");

    let row = fake_row();
    let envelope = envelope_for(&row);
    let body = serde_json::to_vec(&envelope).unwrap();
    let key = row.aggregate_id.to_string();

    // Headers built via the same helper used by the production
    // `send_one` path — this is the unit-of-test for the wire shape.
    let mut owned_headers = rdkafka::message::OwnedHeaders::new();
    for (k, v) in headers_for(&row) {
        owned_headers = owned_headers.insert(rdkafka::message::Header {
            key: k,
            value: Some(v.as_bytes()),
        });
    }
    let record = FutureRecord::to(topic)
        .key(&key)
        .payload(&body)
        .headers(owned_headers);

    let (partition, offset) = producer
        .send(record, Duration::from_secs(30))
        .await
        .expect("producer.send acks");
    eprintln!("produced partition={partition} offset={offset}");

    // Consume it back.
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", "kafka-itest")
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("consumer builds");
    consumer.subscribe(&[topic]).expect("subscribe");

    let msg = timeout(Duration::from_secs(20), consumer.recv())
        .await
        .expect("consumer.recv before timeout")
        .expect("consumer.recv ok");

    // Key matches aggregate_id.
    assert_eq!(
        msg.key()
            .map(|b| std::str::from_utf8(b).unwrap().to_string())
            .as_deref(),
        Some(key.as_str())
    );

    // Payload round-trip — the envelope shape is preserved byte-for-byte
    // (deserialise to JSON and compare).
    let payload_bytes = msg.payload().expect("payload non-empty");
    let recv_envelope: serde_json::Value =
        serde_json::from_slice(payload_bytes).expect("payload is JSON");
    assert_eq!(recv_envelope, envelope);

    // Headers present.
    let hdrs = msg.headers().expect("headers present");
    let mut got = std::collections::HashMap::new();
    for i in 0..hdrs.count() {
        let h = hdrs.get(i);
        let v = std::str::from_utf8(h.value.unwrap_or(&[]))
            .unwrap_or("")
            .to_string();
        got.insert(h.key.to_string(), v);
    }
    assert_eq!(got.get("event_kind").map(String::as_str), Some("declaration.submitted.v1"));
    assert_eq!(got.get("event_id").map(String::as_str), Some(row.event_id.to_string().as_str()));
    assert!(got.contains_key("created_at"));
}
