//! Internal-only endpoint that consumes declaration events from the
//! Declaration service's outbox relay.
//!
//! Authentication is HMAC-SHA256 over the raw request body, using the
//! shared secret configured at startup. The signature arrives in the
//! `X-RECOR-Signature` header. Constant-time verification.
//!
//! Schema: the relay envelope wraps the declaration_submitted_v1
//! payload. We extract the payload, materialise a DeclarationSnapshot,
//! and feed it through the existing pipeline.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::application::SubmitVerificationUseCase;
use crate::domain::declaration_snapshot::{DeclarationSnapshot, OwnerSnapshot};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct InternalAppState {
    pub submit_usecase: Arc<SubmitVerificationUseCase>,
    pub hmac_secret: String,
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

/// Declaration-side `DeclarationSubmittedV1` payload as received in the
/// relay envelope. Field names mirror the wire format produced by
/// services/declaration's outbox writer.
#[derive(Debug, Deserialize)]
pub struct DeclarationSubmittedV1Wire {
    pub declaration_id: Uuid,
    pub entity_id: Uuid,
    pub declarant_principal: String,
    pub declarant_role: String,
    pub kind: String,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerWire>,
    pub attestation: AttestationWire,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: time::OffsetDateTime,
    pub correlation_id: Uuid,
    pub receipt_hash_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct BeneficialOwnerWire {
    pub person_id: Uuid,
    pub ownership_basis_points: u32,
    pub interest_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct AttestationWire {
    pub signed_by: String,
    pub signature_algorithm: String,
    pub signature_hex: String,
    pub public_key_hex: String,
    pub nonce_hex: String,
}

#[derive(Debug, Serialize)]
pub struct InboundResponse {
    pub event_id: Uuid,
    pub event_type: String,
    pub case_id: String,
    pub lane: String,
}

/// Handler for POST /v1/internal/declaration-events.
#[instrument(skip_all)]
pub async fn handle_declaration_event(
    State(state): State<InternalAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<InboundResponse>), (StatusCode, Json<serde_json::Value>)> {
    // 1. HMAC verification.
    if state.hmac_secret.is_empty() {
        warn!("internal endpoint hit but HMAC secret is unconfigured");
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "inbound_disabled",
            "internal endpoint disabled — INBOUND_HMAC_SECRET unset",
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
    if !verify_hmac(&state.hmac_secret, &body, provided_hex) {
        warn!("inbound HMAC verification failed");
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "HMAC signature did not verify",
        ));
    }

    // 2. Deserialise the envelope.
    let envelope: InboundEnvelope = serde_json::from_slice(&body).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "malformed_envelope",
            &format!("envelope parse: {e}"),
        )
    })?;

    if envelope.event_type != "declaration.submitted.v1" {
        // Unknown event type — return 202 (accepted, ignored) so the
        // relay marks it dispatched and doesn't retry.
        info!(event_type = %envelope.event_type, "ignoring non-declaration event type");
        return Ok((
            StatusCode::ACCEPTED,
            Json(InboundResponse {
                event_id: envelope.event_id,
                event_type: envelope.event_type,
                case_id: "skipped".into(),
                lane: "n/a".into(),
            }),
        ));
    }

    let payload: DeclarationSubmittedV1Wire =
        serde_json::from_value(envelope.payload.clone()).map_err(|e| {
            error_response(
                StatusCode::BAD_REQUEST,
                "malformed_payload",
                &format!("payload parse: {e}"),
            )
        })?;

    // 3. Materialise a DeclarationSnapshot for the pipeline.
    let snapshot = DeclarationSnapshot {
        declaration_id: payload.declaration_id,
        entity_id: payload.entity_id,
        declarant_principal: payload.declarant_principal,
        declarant_role: payload.declarant_role,
        kind: payload.kind,
        effective_from: payload.effective_from,
        beneficial_owners: payload
            .beneficial_owners
            .into_iter()
            .map(|o| OwnerSnapshot {
                person_id: o.person_id,
                ownership_basis_points: o.ownership_basis_points,
                interest_kind: o.interest_kind,
            })
            .collect(),
        attestation_signed_by: payload.attestation.signed_by,
        attestation_signature_hex: payload.attestation.signature_hex,
        attestation_public_key_hex: payload.attestation.public_key_hex,
        receipt_hash_hex: payload.receipt_hash_hex,
        correlation_id: payload.correlation_id,
        submitted_at: payload.submitted_at,
    };

    // 4. Run the pipeline.
    let case = state.submit_usecase.execute(snapshot).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "pipeline_failed",
            &format!("{e}"),
        )
    })?;

    info!(
        event_id = %envelope.event_id,
        case_id = %case.case_id,
        lane = case.lane.as_str(),
        "inbound declaration verified"
    );

    Ok((
        StatusCode::CREATED,
        Json(InboundResponse {
            event_id: envelope.event_id,
            event_type: envelope.event_type,
            case_id: case.case_id.to_string(),
            lane: case.lane.as_str().to_string(),
        }),
    ))
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
}

// Used by IntoResponse impls upstream.
impl IntoResponse for InboundResponse {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
