//! Outbox-relay background task.
//!
//! Polls the `outbox` table every `poll_interval` for rows where
//! `dispatched_at IS NULL`, posts each to a configured webhook URL
//! with an HMAC-SHA256 signature, marks `dispatched_at` on 2xx, or
//! increments `dispatch_attempts` and records `last_error` on failure.
//!
//! This is the v1 transport for declaration events leaving the
//! service. It will be replaced by a Kafka producer when F-003
//! (Kafka) lands (`R-DECL-2` follow-up). The semantics — at-least-once
//! delivery, HMAC for service-to-service authentication, retry with
//! backoff — survive the transport swap.
//!
//! HMAC signing: the relay computes `HMAC-SHA256(payload, secret)`
//! over the raw POST body and includes it as the `X-RECOR-Signature`
//! header. The verifier (verification engine) re-computes the HMAC
//! over the body it received and rejects on mismatch. The shared
//! secret lives in env and never appears in logs.

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

type HmacSha256 = Hmac<Sha256>;

/// Configuration for one relay subscriber.
#[derive(Debug, Clone)]
pub struct RelaySubscriber {
    pub name: String,
    pub webhook_url: String,
    /// Shared HMAC secret. The relay never prints this.
    pub hmac_secret: String,
}

/// Background relay task. Single-subscriber in v1; multi-subscriber is
/// a follow-up when more consumers exist.
pub struct OutboxRelay {
    pool: PgPool,
    subscriber: RelaySubscriber,
    http: reqwest::Client,
    poll_interval: Duration,
    max_attempts: i32,
    /// OBS-1: optional metrics handle. The relay records delivery
    /// latency + dispatches a gauge sample (undispatched count, DLQ
    /// size) on every poll. Optional so tests can run without
    /// constructing a full registry.
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
            max_attempts: 12, // 12 × 5s ≈ 1 min before dead-letter
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

    /// OBS-1: wire the shared Prometheus registry handle. The relay
    /// emits `recor_outbox_undispatched`, `recor_outbox_dlq_size`,
    /// and `recor_relay_delivery_latency_seconds` samples when set.
    /// When `metrics` is `None` (the test/legacy path), the relay
    /// behaves identically but does not emit samples.
    pub fn with_metrics(mut self, metrics: std::sync::Arc<crate::metrics::Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Run the relay until the cancellation token fires.
    #[instrument(skip_all, fields(subscriber = %self.subscriber.name, webhook = %self.subscriber.webhook_url))]
    pub async fn run(&self, cancel: CancellationToken) {
        info!(
            poll_interval_ms = self.poll_interval.as_millis() as u64,
            max_attempts = self.max_attempts,
            "outbox relay started"
        );
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("outbox relay shutting down");
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

    /// Process up to N pending outbox rows. Returns Ok even if individual
    /// rows failed (errors logged); Err only on a structural problem like
    /// a DB outage.
    async fn process_batch(&self) -> Result<(), sqlx::Error> {
        // OBS-1: sample the outbox + DLQ gauges every poll. Using
        // `sqlx::query_scalar` (non-macro form) so this read does not
        // require an entry in the prepared-query cache. The gauges are
        // best-effort — failing to sample logs at debug! and proceeds.
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

        let rows = sqlx::query!(
            r#"
            SELECT id, event_id, event_type, event_version, aggregate_id, payload, dispatch_attempts
            FROM outbox
            WHERE dispatched_at IS NULL
              AND dispatch_attempts < $1
            ORDER BY created_at ASC
            LIMIT 32
            "#,
            self.max_attempts,
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }
        debug!(batch_size = rows.len(), "relay batch");

        for row in rows {
            let id = row.id;
            let event_id = row.event_id;
            let event_type = row.event_type;
            let event_version = row.event_version;
            let aggregate_id = row.aggregate_id;
            let payload = row.payload;
            let prior_attempts = row.dispatch_attempts;

            let envelope = serde_json::json!({
                "event_id": event_id,
                "event_type": event_type,
                "event_version": event_version,
                "aggregate_id": aggregate_id,
                "payload": payload,
            });
            let body = serde_json::to_vec(&envelope).expect("envelope is always serialisable");
            // FIND-012: bind iat into the signature. The receiver
            // enforces a ±5-min replay window; captured envelopes
            // cannot be replayed once the window expires.
            let iat = recor_hmac_sig::now_unix_seconds();
            let signature =
                recor_hmac_sig::sign(&self.subscriber.hmac_secret, &body, iat);

            // OBS-1: time the POST itself so we can record subscriber
            // delivery latency. This is the per-attempt round-trip; on
            // success we observe it into the latency histogram.
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
                    sqlx::query!(
                        r#"UPDATE outbox
                           SET dispatched_at = NOW(),
                               dispatch_attempts = dispatch_attempts + 1
                           WHERE id = $1"#,
                        id,
                    )
                    .execute(&self.pool)
                    .await?;
                    if let Some(m) = self.metrics.as_ref() {
                        m.relay_delivery_latency_seconds
                            .with_label_values(&[self.subscriber.name.as_str()])
                            .observe(elapsed);
                    }
                    info!(
                        %event_id, event_type, attempt = prior_attempts + 1,
                        "relay delivered"
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

    /// Routes a failed dispatch attempt to either `record_failure`
    /// (still has retries left) or `move_to_dlq` (exhausted). The
    /// `new_attempts` arg is the count INCLUDING the one that just
    /// failed (i.e., prior + 1).
    async fn handle_failure(
        &self,
        id: uuid::Uuid,
        event_id: uuid::Uuid,
        new_attempts: i32,
        message: &str,
    ) -> Result<(), sqlx::Error> {
        if new_attempts >= self.max_attempts {
            warn!(
                %id, %event_id, attempts = new_attempts, max_attempts = self.max_attempts,
                "outbox row dead-lettered (max_attempts exhausted)"
            );
            self.move_to_dlq(id, new_attempts, message).await
        } else {
            self.record_failure(id, message).await
        }
    }

    async fn record_failure(&self, id: uuid::Uuid, message: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE outbox
               SET dispatch_attempts = dispatch_attempts + 1,
                   last_error = $2
               WHERE id = $1"#,
            id,
            message,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Atomic move: copy the outbox row into outbox_dlq with the
    /// final `dispatch_attempts` + `last_error`, then delete the
    /// original. Both statements run in one transaction so a row is
    /// in EXACTLY one of the two tables at any time.
    async fn move_to_dlq(
        &self,
        id: uuid::Uuid,
        attempts: i32,
        last_error: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO outbox_dlq (
                id, event_id, event_type, event_version,
                aggregate_type, aggregate_id, partition_key,
                payload, headers, created_at,
                dispatch_attempts, last_error
            )
            SELECT
                id, event_id, event_type, event_version,
                aggregate_type, aggregate_id, partition_key,
                payload, headers, created_at,
                $2, $3
            FROM outbox
            WHERE id = $1
            "#,
            id,
            attempts,
            last_error,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(r#"DELETE FROM outbox WHERE id = $1"#, id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Compute HMAC-SHA256(payload, secret), hex-encoded.
pub fn hmac_hex(secret: &str, payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time HMAC verification.
pub fn verify_hmac(secret: &str, payload: &[u8], signature_hex: &str) -> bool {
    let Ok(provided) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);
    mac.verify_slice(&provided).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_round_trips() {
        let sig = hmac_hex("secret", b"hello");
        assert!(verify_hmac("secret", b"hello", &sig));
    }

    #[test]
    fn hmac_rejects_wrong_secret() {
        let sig = hmac_hex("secret-a", b"hello");
        assert!(!verify_hmac("secret-b", b"hello", &sig));
    }

    #[test]
    fn hmac_rejects_tampered_payload() {
        let sig = hmac_hex("secret", b"hello");
        assert!(!verify_hmac("secret", b"goodbye", &sig));
    }

    #[test]
    fn hmac_rejects_malformed_hex() {
        assert!(!verify_hmac("secret", b"hello", "zzzz"));
    }

    #[test]
    fn hmac_rejects_truncated_signature() {
        let sig = hmac_hex("secret", b"hello");
        let truncated = &sig[..sig.len() - 2];
        assert!(!verify_hmac("secret", b"hello", truncated));
    }
}
