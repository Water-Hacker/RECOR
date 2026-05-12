//! Outbox-relay background task for the Verification engine.
//!
//! Polls `verification_outbox` for undispatched rows, HMAC-SHA256-signs
//! the envelope, POSTs to a configured writeback URL (the Declaration
//! service's `/v1/internal/verification-outcomes` endpoint), marks
//! dispatched_at on 2xx.
//!
//! Mirror of the relay in services/declaration. The two services will
//! share an `recor-outbox-relay` crate when the monorepo workspace is
//! wired (R-DECL-6).

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct WritebackSubscriber {
    pub name: String,
    pub url: String,
    pub hmac_secret: String,
}

pub struct VerificationOutboxRelay {
    pool: PgPool,
    subscriber: WritebackSubscriber,
    http: reqwest::Client,
    poll_interval: Duration,
    max_attempts: i32,
}

impl VerificationOutboxRelay {
    pub fn new(pool: PgPool, subscriber: WritebackSubscriber) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client builds");
        Self {
            pool,
            subscriber,
            http,
            poll_interval: Duration::from_secs(5),
            max_attempts: 12,
        }
    }

    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    pub fn with_max_attempts(mut self, a: i32) -> Self {
        self.max_attempts = a;
        self
    }

    #[instrument(skip_all, fields(subscriber = %self.subscriber.name, url = %self.subscriber.url))]
    pub async fn run(&self, cancel: CancellationToken) {
        info!(
            poll_interval_ms = self.poll_interval.as_millis() as u64,
            max_attempts = self.max_attempts,
            "verification outbox relay started"
        );
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("verification outbox relay shutting down");
                    return;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.process_batch().await {
                        error!(error = ?e, "verification relay batch failed");
                    }
                }
            }
        }
    }

    async fn process_batch(&self) -> Result<(), sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT id, event_id, event_type, event_version, aggregate_id, payload, dispatch_attempts
            FROM verification_outbox
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
        debug!(batch_size = rows.len(), "verification relay batch");

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
            let body =
                serde_json::to_vec(&envelope).expect("envelope is always serialisable");
            let signature = hmac_hex(&self.subscriber.hmac_secret, &body);

            let result = self
                .http
                .post(&self.subscriber.url)
                .header("Content-Type", "application/json")
                .header("X-RECOR-Signature", &signature)
                .header("X-RECOR-Event-Type", &event_type)
                .header("X-RECOR-Event-Id", event_id.to_string())
                .body(body)
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    sqlx::query!(
                        r#"UPDATE verification_outbox
                           SET dispatched_at = NOW(),
                               dispatch_attempts = dispatch_attempts + 1
                           WHERE id = $1"#,
                        id,
                    )
                    .execute(&self.pool)
                    .await?;
                    info!(%event_id, event_type, attempt = prior_attempts + 1, "writeback delivered");
                }
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    warn!(
                        %event_id, %status, attempt = prior_attempts + 1,
                        "writeback subscriber returned non-2xx: {text}"
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
                    warn!(
                        %event_id, error = %e, attempt = prior_attempts + 1,
                        "writeback transport error"
                    );
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
    /// (still has retries left) or `move_to_dlq` (exhausted).
    /// `new_attempts` is the count INCLUDING the just-failed try.
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
                "verification_outbox row dead-lettered (max_attempts exhausted)"
            );
            self.move_to_dlq(id, new_attempts, message).await
        } else {
            self.record_failure(id, message).await
        }
    }

    async fn record_failure(&self, id: uuid::Uuid, message: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE verification_outbox
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

    /// Atomic move: copy the verification_outbox row into
    /// verification_outbox_dlq with final dispatch_attempts +
    /// last_error, then delete the original. Both run in one
    /// transaction so a row exists in exactly one table at any time.
    async fn move_to_dlq(
        &self,
        id: uuid::Uuid,
        attempts: i32,
        last_error: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO verification_outbox_dlq (
                id, event_id, event_type, event_version,
                aggregate_id, partition_key, payload, created_at,
                dispatch_attempts, last_error
            )
            SELECT
                id, event_id, event_type, event_version,
                aggregate_id, partition_key, payload, created_at,
                $2, $3
            FROM verification_outbox
            WHERE id = $1
            "#,
            id,
            attempts,
            last_error,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(r#"DELETE FROM verification_outbox WHERE id = $1"#, id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub fn hmac_hex(secret: &str, payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_is_deterministic_for_same_input() {
        let a = hmac_hex("secret", b"payload");
        let b = hmac_hex("secret", b"payload");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // HMAC-SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn hmac_differs_by_secret() {
        assert_ne!(hmac_hex("s1", b"x"), hmac_hex("s2", b"x"));
    }

    #[test]
    fn hmac_differs_by_payload() {
        assert_ne!(hmac_hex("s", b"x"), hmac_hex("s", b"y"));
    }
}
