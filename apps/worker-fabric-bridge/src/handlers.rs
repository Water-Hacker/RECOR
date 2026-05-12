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
    let signature = headers
        .get("x-recor-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if !verify_hmac(state.hmac_secret.expose_secret(), &body, signature) {
        warn!("rejected relay request: HMAC mismatch");
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
}
