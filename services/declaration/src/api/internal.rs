//! Internal-only endpoint that consumes verification outcomes from the
//! Verification Engine's writeback relay.
//!
//! Authentication is HMAC-SHA256 over the raw request body, using the
//! shared secret configured at startup. The signature arrives in the
//! `X-RECOR-Signature` header. Verification is constant-time.
//!
//! The Verification Engine's outbox writes a slim envelope:
//!
//! ```json
//! {
//!   "event_id": "...",
//!   "event_type": "verification.completed.v1",
//!   "event_version": 1,
//!   "aggregate_id": "<declaration_id>",
//!   "payload": {
//!     "case_id": "...",
//!     "declaration_id": "...",
//!     "lane": "green|yellow|red",
//!     "fused_authenticity_belief": 0.0..1.0,
//!     "fused_authenticity_plausibility": 0.0..1.0,
//!     "fused_risk_belief": 0.0..1.0,
//!     "completed_at": "RFC3339"
//!   }
//! }
//! ```
//!
//! Idempotency: the Verification Engine's relay retries on non-2xx.
//! Our use case treats a replay of the same case_id as a successful
//! no-op (returns 200 with `recorded_new_event: false`).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::api::dto::{VerificationOutcomeRequest, VerificationOutcomeResponse};
use crate::application::RecordVerificationOutcomeUseCase;
use crate::error::ServiceError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct InternalAppState {
    pub record_verification_usecase: Arc<RecordVerificationOutcomeUseCase>,
    /// Current HMAC secret. Used for both verify (incoming requests)
    /// and rejected when empty.
    pub hmac_secret: String,
    /// Optional "still-valid old" secret accepted during a rotation
    /// window. Empty string means rotation not in progress.
    ///
    /// Rotation procedure (zero-downtime):
    ///   1. Operator generates a new secret, sets it as `old_hmac_secret`
    ///      on the verifier side AND as `hmac_secret` on the signer
    ///      side. Both signers continue to use the OLD secret;
    ///      verifiers accept both.
    ///   2. Operator updates signer-side `hmac_secret` to the new
    ///      value. New signatures use the new secret; in-flight
    ///      requests with the old signature still verify against
    ///      `old_hmac_secret`.
    ///   3. After all in-flight requests have drained (poll interval
    ///      + retry budget), operator clears `old_hmac_secret`. Only
    ///      the new secret is accepted from now on.
    pub old_hmac_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct InboundEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    #[serde(rename = "event_version")]
    pub _event_version: i32,
    #[serde(rename = "aggregate_id")]
    pub _aggregate_id: Uuid,
    pub payload: serde_json::Value,
}

#[utoipa::path(
    post,
    path = "/v1/internal/verification-outcomes",
    tag = "internal",
    operation_id = "handleVerificationOutcome",
    request_body(
        description = "Slim outbox envelope from the Verification Engine; payload conforms to VerificationOutcomeRequest.",
        content_type = "application/json",
        content = crate::api::dto::VerificationOutcomeRequest,
    ),
    responses(
        (status = 201, description = "New verification outcome recorded", body = crate::api::dto::VerificationOutcomeResponse),
        (status = 200, description = "Replay of an already-recorded outcome (no-op)", body = crate::api::dto::VerificationOutcomeResponse),
        (status = 202, description = "Accepted but ignored (unknown event_type)", body = crate::api::dto::VerificationOutcomeResponse),
        (status = 400, description = "Malformed envelope or payload", body = crate::api::dto::ErrorEnvelope),
        (status = 401, description = "Missing or bad HMAC signature", body = crate::api::dto::ErrorEnvelope),
        (status = 404, description = "Referenced declaration does not exist", body = crate::api::dto::ErrorEnvelope),
        (status = 409, description = "Conflicting outcome for the same case_id", body = crate::api::dto::ErrorEnvelope),
        (status = 500, description = "Backend failure", body = crate::api::dto::ErrorEnvelope),
        (status = 503, description = "Writeback endpoint disabled (no HMAC secret configured)", body = crate::api::dto::ErrorEnvelope),
    ),
    security(
        ("hmacSignature" = []),
    ),
)]
#[instrument(skip_all)]
pub async fn handle_verification_outcome(
    State(state): State<InternalAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<VerificationOutcomeResponse>), (StatusCode, Json<serde_json::Value>)>
{
    if state.hmac_secret.is_empty() {
        warn!("writeback endpoint hit but HMAC secret is unconfigured");
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "writeback_disabled",
            "writeback endpoint disabled — WRITEBACK_HMAC_SECRET unset",
        ));
    }

    let Some(provided_hex) = headers
        .get("x-recor-signature")
        .and_then(|v| v.to_str().ok())
    else {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "missing_signature",
            "X-RECOR-Signature header required",
        ));
    };
    if !verify_hmac_with_rotation(
        &state.hmac_secret,
        &state.old_hmac_secret,
        &body,
        provided_hex,
    ) {
        warn!("writeback HMAC verification failed");
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "HMAC signature did not verify",
        ));
    }

    let envelope: InboundEnvelope = serde_json::from_slice(&body).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "malformed_envelope",
            &format!("envelope parse: {e}"),
        )
    })?;

    if envelope.event_type != "verification.completed.v1" {
        // Unknown event type — return 202 (accepted, ignored) so the
        // relay marks dispatched and stops retrying.
        info!(event_type = %envelope.event_type, "ignoring non-verification event type");
        return Ok((
            StatusCode::ACCEPTED,
            Json(VerificationOutcomeResponse {
                declaration_id: crate::domain::DeclarationId(envelope._aggregate_id),
                verification_case_id: Uuid::nil(),
                lane: crate::domain::VerificationLane::Yellow,
                recorded_new_event: false,
            }),
        ));
    }

    let outcome: VerificationOutcomeRequest =
        serde_json::from_value(envelope.payload.clone()).map_err(|e| {
            error_response(
                StatusCode::BAD_REQUEST,
                "malformed_payload",
                &format!("payload parse: {e}"),
            )
        })?;

    let cmd = outcome.into_command();
    let receipt = state
        .record_verification_usecase
        .execute(cmd)
        .await
        .map_err(service_error_to_writeback)?;

    let status = if receipt.recorded_new_event {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    info!(
        event_id = %envelope.event_id,
        declaration_id = %receipt.declaration_id,
        case_id = %receipt.verification_case_id,
        lane = receipt.lane.as_str(),
        new = receipt.recorded_new_event,
        "verification outcome recorded"
    );

    Ok((
        status,
        Json(VerificationOutcomeResponse {
            declaration_id: receipt.declaration_id,
            verification_case_id: receipt.verification_case_id,
            lane: receipt.lane,
            recorded_new_event: receipt.recorded_new_event,
        }),
    ))
}

fn service_error_to_writeback(
    err: crate::application::RecordVerificationError,
) -> (StatusCode, Json<serde_json::Value>) {
    let svc: ServiceError = err.into();
    use axum::response::IntoResponse;
    let response = svc.into_response();
    let status = response.status();
    // Best-effort: respond with a compact body; the IntoResponse impl
    // already serialised a JSON body but we have to rebuild it here
    // to keep the handler's static return type.
    (
        status,
        Json(json!({
            "error": {
                "kind": classify_status(status),
                "message": status.canonical_reason().unwrap_or("error"),
            }
        })),
    )
}

fn classify_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        404 => "not_found",
        409 => "conflict",
        401 => "unauthorized",
        400 => "bad_request",
        _ => "error",
    }
}

fn error_response(
    status: StatusCode,
    kind: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(json!({ "error": { "kind": kind, "message": message } })),
    )
}

/// Verify an HMAC signature against the current secret, and if a
/// non-empty `old_secret` is configured (rotation in progress), also
/// against the old secret. Either match passes. This is the
/// zero-downtime rotation primitive: during a rotation window the
/// signer can switch over without dropping any in-flight requests.
///
/// An empty `old_secret` is treated as "rotation not active" and
/// only the current secret is consulted.
fn verify_hmac_with_rotation(
    current_secret: &str,
    old_secret: &str,
    payload: &[u8],
    signature_hex: &str,
) -> bool {
    if verify_hmac(current_secret, payload, signature_hex) {
        return true;
    }
    if !old_secret.is_empty() && verify_hmac(old_secret, payload, signature_hex) {
        tracing::warn!(
            "inbound request verified against OLD HMAC secret — rotation in progress, expected for the duration of the rollout"
        );
        return true;
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hmac_hex(secret: &str, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn hmac_verifies_roundtrip() {
        let sig = hmac_hex("k", b"hello");
        assert!(verify_hmac("k", b"hello", &sig));
        assert!(!verify_hmac("wrong", b"hello", &sig));
        assert!(!verify_hmac("k", b"tampered", &sig));
    }

    #[test]
    fn rotation_off_only_current_secret_works() {
        let sig_current = hmac_hex("current", b"x");
        let sig_old = hmac_hex("old", b"x");

        // No rotation: empty old_secret means only `current` is accepted.
        assert!(verify_hmac_with_rotation("current", "", b"x", &sig_current));
        assert!(!verify_hmac_with_rotation("current", "", b"x", &sig_old));
    }

    #[test]
    fn rotation_active_both_old_and_current_accepted() {
        let sig_current = hmac_hex("current", b"x");
        let sig_old = hmac_hex("old", b"x");

        // Rotation window: both secrets valid.
        assert!(verify_hmac_with_rotation("current", "old", b"x", &sig_current));
        assert!(verify_hmac_with_rotation("current", "old", b"x", &sig_old));
    }

    #[test]
    fn rotation_third_party_signature_still_rejected() {
        let sig_attacker = hmac_hex("attacker", b"x");
        // Even during rotation, any signature NOT matching either of
        // the two valid secrets is rejected.
        assert!(!verify_hmac_with_rotation("current", "old", b"x", &sig_attacker));
        assert!(!verify_hmac_with_rotation("current", "", b"x", &sig_attacker));
    }

    #[test]
    fn rotation_tampered_payload_still_rejected() {
        // Sign the original payload with the current secret, then try
        // to verify a tampered payload — should fail under both
        // rotation-on and rotation-off configs.
        let sig = hmac_hex("current", b"original");
        assert!(!verify_hmac_with_rotation("current", "", b"tampered", &sig));
        assert!(!verify_hmac_with_rotation("current", "old", b"tampered", &sig));
    }
}
