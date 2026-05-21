//! TODO-040 — outbox-relay background task for the person service.
//!
//! Mirrors `services/declaration/src/infrastructure/relay.rs` (semantics)
//! and `services/entity-service/src/infrastructure/relay.rs` (the
//! runtime-`sqlx::query` form). The differences vs declaration:
//!
//!   * runtime `sqlx::query` API (not the compile-time `query!` macro)
//!     because the person-service postgres adapter is also runtime
//!     (R-PERSON-SQLX-CACHE follow-up to flip both at once).
//!   * targets the person-service's own `outbox` / `outbox_dlq` tables
//!     (see migrations 0001 + 0004).
//!   * emits person-service-shaped metrics
//!     (`recor_person_outbox_undispatched`,
//!     `recor_person_outbox_dlq_size`,
//!     `recor_person_relay_delivery_latency_seconds`).
//!
//! Semantics — at-least-once delivery, FIND-012-aware HMAC signing,
//! retry with backoff, atomic move to `outbox_dlq` once
//! `dispatch_attempts` exhausts the configured maximum — match
//! declaration so a single operator runbook covers every service.
//!
//! ## Doctrine compliance
//!
//! - **D14 fail-closed** — a non-2xx response or transport error
//!   increments `dispatch_attempts` rather than marking the row
//!   delivered. The atomic move to DLQ happens INSIDE a transaction
//!   so a row is in EXACTLY one of outbox / outbox_dlq at any time.
//! - **D15 cryptographic provenance** — the HMAC sign path uses
//!   `recor_hmac_sig::sign` (iat-bound) so a captured envelope
//!   cannot be replayed once the receiver's ±5min window has
//!   elapsed.
//! - **D17 zero trust** — the HMAC secret is the cross-service
//!   authenticator; the receiver verifies it before accepting the
//!   delivery.
//! - **D18 no secrets** — the secret arrives via [`RelaySubscriber`]
//!   constructed from `SecretString::expose_secret()`; this module
//!   never logs the raw value.

use std::time::Duration;

use sqlx::{PgPool, Row};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RelaySubscriber {
    pub name: String,
    pub webhook_url: String,
    pub hmac_secret: String,
}

pub struct OutboxRelay {
    pool: PgPool,
    subscriber: RelaySubscriber,
    http: reqwest::Client,
    poll_interval: Duration,
    max_attempts: i32,
    batch_size: i64,
    metrics: Option<std::sync::Arc<crate::metrics::Metrics>>,
}

impl OutboxRelay {
    pub fn new(pool: PgPool, subscriber: RelaySubscriber) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client should build");
        Self {
            pool,
            subscriber,
            http,
            poll_interval: Duration::from_secs(5),
            max_attempts: 12,
            batch_size: 50,
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

    pub fn with_batch_size(mut self, batch_size: i64) -> Self {
        self.batch_size = batch_size.clamp(1, 500);
        self
    }

    pub fn with_metrics(mut self, metrics: std::sync::Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    #[instrument(skip_all, fields(
        subscriber = %self.subscriber.name,
        webhook = %self.subscriber.webhook_url,
    ))]
    pub async fn run(&self, cancel: CancellationToken) {
        info!(
            poll_interval_ms = self.poll_interval.as_millis() as u64,
            max_attempts = self.max_attempts,
            batch_size = self.batch_size,
            "person outbox relay started"
        );
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("person outbox relay shutting down");
                    return;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.process_batch().await {
                        error!(error = ?e, "relay batch failed");
                    }
                }
            }
        }
    }

    pub async fn process_batch(&self) -> Result<(), sqlx::Error> {
        if let Some(m) = self.metrics.as_ref() {
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM outbox WHERE dispatched_at IS NULL",
            )
            .fetch_one(&self.pool)
            .await
            {
                Ok(n) => m.outbox_undispatched.set(n),
                Err(e) => debug!(error = ?e, "outbox-undispatched gauge sample failed"),
            }
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox_dlq")
                .fetch_one(&self.pool)
                .await
            {
                Ok(n) => m.outbox_dlq_size.set(n),
                Err(e) => debug!(error = ?e, "outbox-dlq-size gauge sample failed"),
            }
        }

        let rows = sqlx::query(
            "SELECT id, event_id, event_type, event_version, aggregate_id, \
                    payload, dispatch_attempts \
             FROM outbox \
             WHERE dispatched_at IS NULL \
               AND dispatch_attempts < $1 \
             ORDER BY created_at ASC \
             LIMIT $2",
        )
        .bind(self.max_attempts)
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }
        debug!(batch_size = rows.len(), "person relay batch");

        for row in rows {
            let id: Uuid = row.try_get("id")?;
            let event_id: Uuid = row.try_get("event_id")?;
            let event_type: String = row.try_get("event_type")?;
            let event_version: i32 = row.try_get("event_version")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let payload: serde_json::Value = row.try_get("payload")?;
            let prior_attempts: i32 = row.try_get("dispatch_attempts")?;

            let envelope = serde_json::json!({
                "event_id": event_id,
                "event_type": event_type,
                "event_version": event_version,
                "aggregate_id": aggregate_id,
                "payload": payload,
            });
            let body = serde_json::to_vec(&envelope).expect("envelope is always serialisable");
            let iat = recor_hmac_sig::now_unix_seconds();
            let signature =
                recor_hmac_sig::sign(&self.subscriber.hmac_secret, &body, iat);

            let send_start = std::time::Instant::now();
            let result = self
                .http
                .post(&self.subscriber.webhook_url)
                .header("Content-Type", "application/json")
                .header("X-RECOR-Signature", &signature)
                .header("X-RECOR-Timestamp", iat.to_string())
                .header("X-RECOR-Event-Type", &event_type)
                .header("X-RECOR-Event-Id", event_id.to_string())
                .body(body)
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    let elapsed = send_start.elapsed().as_secs_f64();
                    sqlx::query(
                        "UPDATE outbox \
                         SET dispatched_at = NOW(), \
                             dispatch_attempts = dispatch_attempts + 1 \
                         WHERE id = $1",
                    )
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                    if let Some(m) = self.metrics.as_ref() {
                        m.relay_delivery_latency_seconds
                            .with_label_values(&[self.subscriber.name.as_str()])
                            .observe(elapsed);
                    }
                    info!(
                        %event_id, event_type, attempt = prior_attempts + 1,
                        "person relay delivered"
                    );
                }
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    warn!(
                        %event_id, %status, attempt = prior_attempts + 1,
                        "subscriber returned non-2xx: {text}"
                    );
                    self.handle_failure(
                        id,
                        event_id,
                        prior_attempts + 1,
                        &format!("http {status}: {text}"),
                    )
                    .await?;
                }
                Err(e) => {
                    warn!(%event_id, error = %e, attempt = prior_attempts + 1, "relay transport error");
                    self.handle_failure(
                        id,
                        event_id,
                        prior_attempts + 1,
                        &format!("transport: {e}"),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_failure(
        &self,
        id: Uuid,
        event_id: Uuid,
        new_attempts: i32,
        message: &str,
    ) -> Result<(), sqlx::Error> {
        if new_attempts >= self.max_attempts {
            warn!(
                %id, %event_id, attempts = new_attempts, max_attempts = self.max_attempts,
                "person outbox row dead-lettered (max_attempts exhausted)"
            );
            self.move_to_dlq(id, new_attempts, message).await
        } else {
            self.record_failure(id, message).await
        }
    }

    async fn record_failure(&self, id: Uuid, message: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE outbox \
             SET dispatch_attempts = dispatch_attempts + 1, \
                 last_error = $2 \
             WHERE id = $1",
        )
        .bind(id)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn move_to_dlq(
        &self,
        id: Uuid,
        attempts: i32,
        last_error: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO outbox_dlq ( \
                 id, event_id, event_type, event_version, \
                 aggregate_type, aggregate_id, partition_key, \
                 payload, headers, created_at, \
                 dispatch_attempts, last_error \
             ) \
             SELECT \
                 id, event_id, event_type, event_version, \
                 aggregate_type, aggregate_id, partition_key, \
                 payload, headers, created_at, \
                 $2, $3 \
             FROM outbox \
             WHERE id = $1",
        )
        .bind(id)
        .bind(attempts)
        .bind(last_error)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM outbox WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscriber() -> RelaySubscriber {
        RelaySubscriber {
            name: "verification-engine".to_string(),
            webhook_url: "http://localhost:0/sink".to_string(),
            hmac_secret: "test-secret".to_string(),
        }
    }

    fn lazy_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://does-not-matter:5432/x")
            .expect("connect_lazy cannot fail without a network call")
    }

    #[tokio::test]
    async fn relay_constructor_pins_doctrine_default_max_attempts() {
        let r = OutboxRelay::new(lazy_pool(), subscriber());
        assert_eq!(r.max_attempts, 12);
        assert_eq!(r.batch_size, 50);
        assert_eq!(r.poll_interval, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn relay_builder_methods_override_defaults() {
        let r = OutboxRelay::new(lazy_pool(), subscriber())
            .with_poll_interval(Duration::from_secs(30))
            .with_max_attempts(5)
            .with_batch_size(100);
        assert_eq!(r.max_attempts, 5);
        assert_eq!(r.batch_size, 100);
        assert_eq!(r.poll_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn relay_batch_size_clamps_to_safe_ceiling() {
        let big = OutboxRelay::new(lazy_pool(), subscriber()).with_batch_size(10_000);
        assert!(big.batch_size <= 500);
        let small = OutboxRelay::new(lazy_pool(), subscriber()).with_batch_size(0);
        assert!(small.batch_size >= 1);
    }

    #[tokio::test]
    async fn relay_shuts_down_on_pre_cancelled_token() {
        let r = OutboxRelay::new(lazy_pool(), subscriber())
            .with_poll_interval(Duration::from_secs(60));
        let cancel = CancellationToken::new();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(2), r.run(cancel))
            .await
            .expect("pre-cancelled relay must return within 2s");
    }
}
