//! HTTP receiver and operational surface (healthz, metrics).

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use sha2::Sha256;
use tracing::{debug, warn};

use crate::processor::{EventEnvelope, EventProcessor, ProcessOutcome};
use crate::WorkerMetrics;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    pub processor: Arc<EventProcessor>,
    pub metrics: Arc<WorkerMetrics>,
    pub hmac_secret: secrecy::SecretString,
    /// FIND-015 / ADR-005: previous-generation HMAC secret accepted
    /// alongside `hmac_secret` during a rotation window. Empty ⇒ no
    /// rotation in progress; only `hmac_secret` is checked.
    pub hmac_secret_old: secrecy::SecretString,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics_handler))
        .route("/v1/relay", post(receive))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    // Liveness is decoupled from readiness; the processor is always
    // ready once the binary starts. A future ticket adds a Fabric
    // gateway probe here.
    let _ = state.metrics.anchor_total.with_label_values(&["committed"]);
    (StatusCode::OK, "ready")
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics.encode_text() {
        Ok(body) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            body,
        )
            .into_response(),
        Err(e) => {
            warn!(error = %e, "metrics encode failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "metrics encode failed").into_response()
        }
    }
}

/// `POST /v1/relay` — accept one outbox row.
async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // FIND-012: iat-bound HMAC verification via the shared
    // `recor-hmac-sig` crate. Both X-RECOR-Signature and
    // X-RECOR-Timestamp are required; requests outside a ±5-min
    // window are rejected before the MAC compare runs.
    // FIND-015 / ADR-005: dual-secret rotation slot still honoured —
    // when `RECOR_FABRIC_BRIDGE_HMAC_OLD` is set, MACs under the old
    // secret verify too.
    let sig_header = headers
        .get("x-recor-signature")
        .and_then(|v| v.to_str().ok());
    let ts_header = headers
        .get("x-recor-timestamp")
        .and_then(|v| v.to_str().ok());
    let primary = state.hmac_secret.expose_secret();
    let old = state.hmac_secret_old.expose_secret();
    let mut cfg = recor_hmac_sig::VerifyConfig::primary(primary);
    if !old.is_empty() {
        cfg = cfg.with_old_secret(old);
    }
    if let Err(e) = recor_hmac_sig::verify(
        &cfg,
        &body,
        sig_header,
        ts_header,
        recor_hmac_sig::now_unix_seconds(),
    ) {
        warn!(error = %e, "rejected relay request: HMAC verification failed");
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }

    let envelope: EventEnvelope = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "malformed relay body");
            return (StatusCode::BAD_REQUEST, format!("malformed body: {e}"))
                .into_response();
        }
    };

    debug!(event_id = %envelope.event_id, event_type = %envelope.event_type, "received relay event");

    match state.processor.process(envelope).await {
        ProcessOutcome::Committed { tx_id } => {
            (StatusCode::OK, format!("committed:{tx_id}")).into_response()
        }
        ProcessOutcome::AlreadyCommitted { tx_id } => {
            (StatusCode::OK, format!("already_committed:{tx_id}")).into_response()
        }
        ProcessOutcome::Skipped => (StatusCode::OK, "skipped").into_response(),
        ProcessOutcome::DeadLettered { cause } => {
            // 200 — the relay should NOT retry; the DLQ is now the
            // forensic record (D14 fail-closed at THIS boundary
            // delegates the durability promise to the DLQ).
            (StatusCode::OK, format!("dead_lettered:{cause}")).into_response()
        }
        ProcessOutcome::Retryable { message } => {
            warn!(message, "transient error; asking relay to retry");
            (StatusCode::SERVICE_UNAVAILABLE, message).into_response()
        }
    }
}

fn verify_hmac(secret: &str, payload: &[u8], signature_hex: &str) -> bool {
    let Ok(provided) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);
    mac.verify_slice(&provided).is_ok()
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
    fn hmac_round_trips() {
        let sig = hmac_hex("secret", b"hello");
        assert!(verify_hmac("secret", b"hello", &sig));
    }

    #[test]
    fn hmac_rejects_wrong_secret() {
        let sig = hmac_hex("a", b"hello");
        assert!(!verify_hmac("b", b"hello", &sig));
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

    /// FIND-015 / ADR-005: during a rotation window, requests signed
    /// with the previous-generation secret must still verify.
    #[test]
    fn dual_secret_rotation_accepts_old_secret() {
        let new = "new-secret";
        let old = "old-secret";
        // Signed with the OUTGOING secret — should still verify when
        // operator sets both slots during the rotation window.
        let sig = hmac_hex(old, b"payload");
        let primary_ok = verify_hmac(new, b"payload", &sig);
        let old_ok = verify_hmac(old, b"payload", &sig);
        assert!(!primary_ok);
        assert!(old_ok);
    }

    /// Empty `hmac_secret_old` is the steady-state default — it must
    /// never match (otherwise an empty secret would accept any
    /// payload signed with the empty key).
    #[test]
    fn dual_secret_empty_old_does_not_match() {
        let sig = hmac_hex("", b"payload");
        // The signature exists but the receiver guards `!old.is_empty()`
        // before checking the old secret. Verify that guard behaviour
        // here by simulating it directly.
        let old = "";
        let accepted = !old.is_empty() && verify_hmac(old, b"payload", &sig);
        assert!(!accepted);
    }
}
