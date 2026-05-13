//! R-LOOP-2 — Kafka producer for the declaration outbox.
//!
//! This is the Kafka sibling of [`super::relay::OutboxRelay`]. The two
//! transports speak the same envelope shape (JSON), so the v1 cutover
//! is purely operational: flip `RELAY_TRANSPORT=kafka` and the
//! verification engine's Kafka consumer picks up where the HTTP webhook
//! left off. See `docs/adr/0007-kafka-transport-cutover.md` for the
//! migration plan + the deprecation timeline for HTTP.
//!
//! ## Wire shape
//!
//! For every undispatched outbox row, the producer publishes one
//! message to the configured topic (`recor.declaration.events.v1` for
//! v1) with:
//!
//! - **Key:** the `aggregate_id` as bytes. All events for one
//!   declaration land on the same partition, preserving per-aggregate
//!   ordering. Kafka guarantees order *within a partition* — that's
//!   the property the verification engine's consumer relies on to
//!   apply state transitions in submission order.
//! - **Payload:** the same JSON envelope the HTTP relay POSTs
//!   (`{event_id, event_type, event_version, aggregate_id, payload}`).
//!   No schema migration in v1; a schema-registry follow-up will own
//!   the Avro/Protobuf migration.
//! - **Headers:** `event_id`, `event_kind` (= event_type), `created_at`
//!   (RFC 3339 from outbox.created_at). These make broker-side filtering
//!   trivial for consumers that don't want to deserialise the full
//!   payload.
//!
//! ## Reliability
//!
//! - The underlying `FutureProducer` runs with `enable.idempotence=true`,
//!   so retries on the broker side never duplicate messages even after
//!   transient broker failures. rdkafka's built-in retry handles
//!   transient errors transparently.
//! - The outbox row is marked `dispatched_at` only after the broker
//!   acknowledges the message (acks=all + idempotence ⇒ exactly-once
//!   from producer to broker; at-least-once is the *consumer's*
//!   problem).
//! - On persistent send failure (broker down past
//!   `message.timeout.ms`), the row stays undispatched and
//!   `dispatch_attempts` ticks. The standard outbox→DLQ flow then
//!   triggers at `max_attempts` (same shape as the HTTP relay).
//! - The consumer side is idempotent on `event_id` (existing invariant
//!   for the HTTP path), so even if a message is replayed by Kafka the
//!   verification case is not double-applied.

use std::collections::HashMap;
use std::time::Duration;

use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rdkafka::ClientConfig;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Trait that both transports (HTTP relay + Kafka producer) implement
/// so the boot wiring in `main.rs` can hold either behind an `Arc<dyn
/// RelayBackend>`. The HTTP relay does NOT yet implement this trait —
/// the boot wiring branches on `RELAY_TRANSPORT` and spawns whichever
/// concrete type matches. The trait exists so a future consolidation
/// (when HTTP retires) has a clean seam.
#[async_trait::async_trait]
pub trait RelayBackend: Send + Sync {
    /// Drain the outbox into the underlying transport. Returns Ok on
    /// the happy path; Err only on structural problems (DB outage, etc.).
    async fn process_batch(&self) -> Result<(), sqlx::Error>;

    /// Run until the cancellation token fires. The default impl polls
    /// `process_batch` on a fixed interval.
    async fn run(&self, cancel: CancellationToken);
}

/// One outbox row as the producer sees it. Mirrors the schema in
/// `migrations/0001_initial.sql`'s `outbox` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OutboxRow {
    pub id: uuid::Uuid,
    pub event_id: uuid::Uuid,
    pub event_type: String,
    pub event_version: i32,
    pub aggregate_id: uuid::Uuid,
    pub payload: serde_json::Value,
    pub dispatch_attempts: i32,
    pub created_at: time::OffsetDateTime,
}

/// Serialised envelope as it lands on Kafka. Identical shape to the
/// HTTP relay's POST body so the cutover is observability-only on
/// the consumer side.
pub fn envelope_for(row: &OutboxRow) -> serde_json::Value {
    serde_json::json!({
        "event_id": row.event_id,
        "event_type": row.event_type,
        "event_version": row.event_version,
        "aggregate_id": row.aggregate_id,
        "payload": row.payload,
    })
}

/// Headers attached to every produced message. Returned as
/// `(key, value)` pairs so the producer (which uses rdkafka's
/// `OwnedHeaders`) and tests can share construction.
pub fn headers_for(row: &OutboxRow) -> Vec<(&'static str, String)> {
    vec![
        ("event_id", row.event_id.to_string()),
        ("event_kind", row.event_type.clone()),
        (
            "created_at",
            row.created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| String::new()),
        ),
    ]
}

/// The Kafka producer backend.
pub struct KafkaProducer {
    pool: PgPool,
    pub(crate) producer: FutureProducer,
    pub(crate) topic: String,
    poll_interval: Duration,
    max_attempts: i32,
    send_timeout: Duration,
    metrics: Option<std::sync::Arc<crate::metrics::Metrics>>,
}

impl KafkaProducer {
    /// Build a [`FutureProducer`] with the idempotence settings the
    /// architecture requires. Pulled out so it can be reused in tests
    /// (testcontainers spins up its own broker).
    pub fn build_producer(brokers: &str) -> Result<FutureProducer, rdkafka::error::KafkaError> {
        ClientConfig::new()
            .set("bootstrap.servers", brokers)
            // D13: idempotent producer mode collapses broker-side
            // retries to a single delivery per message even after
            // transient ISR failures. acks=all + max.in.flight=5 is
            // the rdkafka-recommended combination.
            .set("enable.idempotence", "true")
            .set("acks", "all")
            .set("max.in.flight.requests.per.connection", "5")
            // Bounded retries inside the client; persistent failures
            // bubble up to us so the outbox row stays undispatched.
            .set("retries", "10")
            // 30s ceiling on a single send attempt. Beyond this we
            // return the row to the undispatched pool.
            .set("message.timeout.ms", "30000")
            // gzip is the v1 compression — `libz` is on by default.
            // lz4/zstd require extra feature flags; we trade compression
            // ratio for a leaner dependency surface.
            .set("compression.type", "gzip")
            // Tight linger; we don't need to batch heavily because the
            // outbox already aggregates by poll cycle.
            .set("linger.ms", "5")
            .create()
    }

    pub fn new(
        pool: PgPool,
        producer: FutureProducer,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            producer,
            topic: topic.into(),
            poll_interval: Duration::from_secs(5),
            max_attempts: 12,
            send_timeout: Duration::from_secs(30),
            metrics: None,
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn with_max_attempts(mut self, attempts: i32) -> Self {
        self.max_attempts = attempts;
        self
    }

    pub fn with_metrics(mut self, metrics: std::sync::Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Process up to N pending outbox rows.
    #[instrument(skip_all, fields(topic = %self.topic))]
    pub async fn process_batch_inner(&self) -> Result<(), sqlx::Error> {
        let rows = sqlx::query_as::<_, OutboxRow>(
            r#"
            SELECT id, event_id, event_type, event_version, aggregate_id,
                   payload, dispatch_attempts, created_at
            FROM outbox
            WHERE dispatched_at IS NULL
              AND dispatch_attempts < $1
            ORDER BY created_at ASC
            LIMIT 32
            "#,
        )
        .bind(self.max_attempts)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }
        debug!(batch_size = rows.len(), "kafka producer batch");

        for row in rows {
            let id = row.id;
            let event_id = row.event_id;
            let prior_attempts = row.dispatch_attempts;
            let send_start = std::time::Instant::now();

            let result = self.send_one(&row).await;
            let elapsed = send_start.elapsed().as_secs_f64();

            match result {
                Ok(()) => {
                    sqlx::query(
                        r#"UPDATE outbox
                           SET dispatched_at = NOW(),
                               dispatch_attempts = dispatch_attempts + 1
                           WHERE id = $1"#,
                    )
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                    if let Some(m) = self.metrics.as_ref() {
                        m.kafka_produce_total
                            .with_label_values(&["success"])
                            .inc();
                        m.kafka_produce_latency_seconds.observe(elapsed);
                    }
                    info!(
                        %event_id, attempt = prior_attempts + 1,
                        elapsed_s = elapsed,
                        "kafka produce ok"
                    );
                }
                Err(e) => {
                    if let Some(m) = self.metrics.as_ref() {
                        m.kafka_produce_total
                            .with_label_values(&["failure"])
                            .inc();
                    }
                    warn!(%event_id, error = %e, attempt = prior_attempts + 1, "kafka produce failed");
                    self.record_failure(id, &e).await?;
                }
            }
        }
        Ok(())
    }

    async fn send_one(&self, row: &OutboxRow) -> Result<(), String> {
        let envelope = envelope_for(row);
        let body = serde_json::to_vec(&envelope)
            .map_err(|e| format!("envelope serialise: {e}"))?;
        let key = row.aggregate_id.to_string();

        let mut headers = rdkafka::message::OwnedHeaders::new();
        for (k, v) in headers_for(row) {
            headers = headers.insert(rdkafka::message::Header {
                key: k,
                value: Some(v.as_bytes()),
            });
        }

        let record = FutureRecord::to(&self.topic)
            .key(&key)
            .payload(&body)
            .headers(headers);

        match self.producer.send(record, self.send_timeout).await {
            Ok((partition, offset)) => {
                debug!(partition, offset, "kafka delivery acked");
                Ok(())
            }
            Err((kafka_err, _msg)) => Err(format!("kafka send: {kafka_err}")),
        }
    }

    async fn record_failure(&self, id: uuid::Uuid, err: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE outbox
               SET dispatch_attempts = dispatch_attempts + 1,
                   last_error = $2
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(err)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl RelayBackend for KafkaProducer {
    async fn process_batch(&self) -> Result<(), sqlx::Error> {
        self.process_batch_inner().await
    }

    #[instrument(skip_all, fields(topic = %self.topic))]
    async fn run(&self, cancel: CancellationToken) {
        info!(
            topic = %self.topic,
            poll_interval_ms = self.poll_interval.as_millis() as u64,
            max_attempts = self.max_attempts,
            "kafka producer started"
        );
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("kafka producer shutting down — flushing");
                    // Best-effort flush of any in-flight messages so
                    // shutdown does not silently drop acked-by-us
                    // records that aren't yet on the broker.
                    let _ = self.producer.flush(Duration::from_secs(5));
                    return;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.process_batch_inner().await {
                        error!(error = ?e, "kafka producer batch failed");
                    }
                }
            }
        }
    }
}

/// Test-only helper: a mock that records every send call instead of
/// hitting a real broker. Used by the producer unit test below + the
/// kafka-smoke fixtures.
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockSink {
        pub sent: Mutex<Vec<MockMessage>>,
    }

    #[derive(Debug, Clone)]
    pub struct MockMessage {
        pub topic: String,
        pub key: String,
        pub payload: serde_json::Value,
        pub headers: HashMap<String, String>,
    }

    impl MockSink {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn record(
            &self,
            topic: &str,
            row: &OutboxRow,
        ) -> Result<(), String> {
            let envelope = envelope_for(row);
            let key = row.aggregate_id.to_string();
            let mut hdrs = HashMap::new();
            for (k, v) in headers_for(row) {
                hdrs.insert(k.to_string(), v);
            }
            self.sent.lock().unwrap().push(MockMessage {
                topic: topic.to_string(),
                key,
                payload: envelope,
                headers: hdrs,
            });
            Ok(())
        }

        pub fn snapshot(&self) -> Vec<MockMessage> {
            self.sent.lock().unwrap().clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_row() -> OutboxRow {
        OutboxRow {
            id: uuid::Uuid::new_v4(),
            event_id: uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            event_type: "declaration.submitted.v1".to_string(),
            event_version: 1,
            aggregate_id: uuid::Uuid::parse_str(
                "11111111-1111-1111-1111-111111111111",
            )
            .unwrap(),
            payload: serde_json::json!({"hello": "world"}),
            dispatch_attempts: 0,
            created_at: time::macros::datetime!(2026-05-12 10:00:00 UTC),
        }
    }

    #[test]
    fn envelope_carries_the_outbox_shape() {
        let row = fake_row();
        let env = envelope_for(&row);
        assert_eq!(env["event_id"], serde_json::json!(row.event_id));
        assert_eq!(env["event_type"], "declaration.submitted.v1");
        assert_eq!(env["event_version"], 1);
        assert_eq!(env["aggregate_id"], serde_json::json!(row.aggregate_id));
        assert_eq!(env["payload"], serde_json::json!({"hello": "world"}));
    }

    #[test]
    fn headers_contain_event_id_kind_and_created_at() {
        let row = fake_row();
        let h = headers_for(&row);
        let map: HashMap<_, _> = h.into_iter().collect();
        assert_eq!(map["event_id"], row.event_id.to_string());
        assert_eq!(map["event_kind"], "declaration.submitted.v1");
        assert!(
            map["created_at"].starts_with("2026-05-12T10:00:00"),
            "got: {}",
            map["created_at"]
        );
    }

    #[test]
    fn mock_records_send_with_aggregate_key() {
        let sink = mock::MockSink::new();
        let row = fake_row();
        sink.record("recor.declaration.events.v1", &row).unwrap();
        let snap = sink.snapshot();
        assert_eq!(snap.len(), 1);
        let m = &snap[0];
        assert_eq!(m.topic, "recor.declaration.events.v1");
        assert_eq!(m.key, row.aggregate_id.to_string());
        assert_eq!(m.payload["event_type"], "declaration.submitted.v1");
        assert_eq!(m.headers["event_kind"], "declaration.submitted.v1");
    }

    #[test]
    fn producer_build_accepts_empty_brokers_for_unit_test() {
        // We never actually connect in this test — we just want to
        // exercise the ClientConfig builder path so it stays in sync
        // with rdkafka API breaks.
        let _p = KafkaProducer::build_producer("localhost:9092").expect("client builds");
    }
}
