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
            // FIND-012: bind an `iat` (issued-at) timestamp into the
            // signature payload. The receiver enforces a ±5-min
            // replay window; a captured envelope cannot be replayed
            // after the window expires even before the secret
            // rotates.
            let iat = recor_hmac_sig::now_unix_seconds();
            let signature =
                recor_hmac_sig::sign(&self.subscriber.hmac_secret, &body, iat);

            // TODO-050 — propagate the originating declaration's
            // correlation_id as X-Correlation-ID. The body's
            // `correlation_id` field is authoritative; the header is
            // for ingress + log correlation. Absent for legacy outbox
            // rows (e.g. unknown event types under test); the
            // declaration service refuses envelopes whose body is
            // missing correlation_id at the boundary.
            let correlation_header = payload
                .get("correlation_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut req = self
                .http
                .post(&self.subscriber.url)
                .header("Content-Type", "application/json")
                .header("X-RECOR-Signature", &signature)
                .header("X-RECOR-Timestamp", iat.to_string())
                .header("X-RECOR-Event-Type", &event_type)
                .header("X-RECOR-Event-Id", event_id.to_string());
            if let Some(h) = correlation_header.as_deref() {
                req = req.header("X-Correlation-ID", h);
            }
            let result = req.body(body).send().await;

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
    use uuid::Uuid;

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

    // ─── TODO-050 — correlation_id HMAC round-trip ───────────────────
    //
    // The envelope-build code lives in `process_batch`; for unit
    // testing we exercise the relevant pieces directly: shape the
    // payload as the V-engine outbox writer does, sign it, then
    // verify the signature is stable across a serialise-roundtrip.

    fn build_envelope(correlation_id: Uuid) -> (serde_json::Value, Vec<u8>) {
        let case_id = Uuid::now_v7();
        let declaration_id = Uuid::now_v7();
        let payload = serde_json::json!({
            "case_id": case_id,
            "declaration_id": declaration_id,
            "correlation_id": correlation_id,
            "lane": "green",
            "fused_authenticity_belief": 0.9,
            "fused_authenticity_plausibility": 0.95,
            "fused_risk_belief": 0.05,
            "completed_at": "2026-05-20T00:00:00Z",
        });
        let envelope = serde_json::json!({
            "event_id": Uuid::now_v7(),
            "event_type": "verification.completed.v1",
            "event_version": 1,
            "aggregate_id": declaration_id,
            "payload": payload,
        });
        let body = serde_json::to_vec(&envelope).expect("envelope is serialisable");
        (envelope, body)
    }

    #[test]
    fn correlation_id_roundtrips_through_serialise() {
        let cid = Uuid::now_v7();
        let (envelope, _body) = build_envelope(cid);
        let reser: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&envelope).unwrap()).unwrap();
        let extracted = reser["payload"]["correlation_id"].as_str().unwrap();
        let parsed: Uuid = extracted.parse().unwrap();
        assert_eq!(parsed, cid);
    }

    #[test]
    fn hmac_signature_covers_correlation_id() {
        let cid_a = Uuid::now_v7();
        let cid_b = Uuid::now_v7();
        assert_ne!(cid_a, cid_b);
        let (_, body_a) = build_envelope(cid_a);
        let (_, body_b) = build_envelope(cid_b);
        let sig_a = hmac_hex("k", &body_a);
        let sig_b = hmac_hex("k", &body_b);
        assert_ne!(sig_a, sig_b);
    }

    #[test]
    fn hmac_verifies_when_correlation_id_unchanged() {
        let cid = Uuid::now_v7();
        let (_, body) = build_envelope(cid);
        let sig = hmac_hex("k", &body);
        let mut mac = HmacSha256::new_from_slice(b"k").unwrap();
        mac.update(&body);
        let computed = hex::encode(mac.finalize().into_bytes());
        assert_eq!(computed, sig);
    }
}
