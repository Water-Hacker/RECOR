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
use utoipa::ToSchema;
use uuid::Uuid;

use crate::application::SubmitVerificationUseCase;
use crate::domain::declaration_snapshot::{DeclarationSnapshot, OwnerSnapshot};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct InternalAppState {
    pub submit_usecase: Arc<SubmitVerificationUseCase>,
    /// Current HMAC secret for the D→V inbound channel.
    pub hmac_secret: String,
    /// Optional "still-valid old" secret accepted during a rotation
    /// window. Empty means rotation not in progress. See the
    /// declaration service's internal.rs doc comment for the full
    /// rotation procedure.
    pub old_hmac_secret: String,
    /// R-LOOP-3 — whether the inbound endpoint requires the HMAC
    /// header. `true` for `AUTH_TRANSPORT=hmac` and `mtls`; `false`
    /// for `mtls-only`.
    pub hmac_required: bool,
    /// R-LOOP-3 — the SPIFFE ID this endpoint expects from peers at
    /// the TLS layer. Empty when mTLS is disabled.
    pub expected_peer_spiffe_id: String,
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
    /// PR-FATF-2.B — FATF R.24 c.24.6 cascade tier. Optional on the
    /// wire for back-compat with declaration-side events that pre-date
    /// the FATF migration (`#[serde(default)]` deserialises absent as
    /// None). Stage 7 cross-source verification (TODO-013) is the
    /// consumer that reads this field.
    #[serde(default)]
    pub cascade_tier: Option<String>,
    #[serde(default)]
    pub control_basis: Option<String>,
    #[serde(default)]
    pub cascade_tier_b_ruled_out_evidence: Option<String>,
    #[serde(default)]
    pub is_nominee: Option<bool>,
    #[serde(default)]
    pub nominator_person_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AttestationWire {
    pub signed_by: String,
    pub signature_algorithm: String,
    pub signature_hex: String,
    pub public_key_hex: String,
    pub nonce_hex: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InboundResponse {
    #[schema(value_type = String, format = "uuid")]
    pub event_id: Uuid,
    pub event_type: String,
    /// Verification case UUID minted for this declaration event.
    pub case_id: String,
    /// Lane decision (`"green"`, `"yellow"`, `"red"`).
    pub lane: String,
}

/// Handler for POST /v1/internal/declaration-events.
#[utoipa::path(
    post,
    path = "/v1/internal/declaration-events",
    tag = "internal",
    operation_id = "handleDeclarationEvent",
    // The body is the declaration-relay envelope; its schema is owned
    // by the declaration service. We expose it as `serde_json::Value`
    // here — the wire contract lives in declaration's OpenAPI spec.
    request_body(content = serde_json::Value, description = "Outbox envelope from the declaration service's relay (HMAC-signed body)"),
    responses(
        (status = 200, description = "Verification case created from the inbound declaration event", body = InboundResponse),
        (status = 400, description = "Malformed envelope or payload", body = crate::api::rest::ErrorEnvelope),
        (status = 401, description = "HMAC signature missing or invalid (or peer-SPIFFE-ID mismatch under mtls-only)", body = crate::api::rest::ErrorEnvelope),
        (status = 500, description = "Internal failure", body = crate::api::rest::ErrorEnvelope),
        (status = 503, description = "INBOUND_HMAC_SECRET unset under AUTH_TRANSPORT=hmac — endpoint disabled (D14 fail-closed)", body = crate::api::rest::ErrorEnvelope),
    ),
    security(
        ("hmacSignature" = []),
    ),
)]
#[instrument(skip_all)]
pub async fn handle_declaration_event(
    State(state): State<InternalAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<InboundResponse>), (StatusCode, Json<serde_json::Value>)> {
    // 1. HMAC verification — skipped under AUTH_TRANSPORT=mtls-only
    // (the TLS-layer peer-SPIFFE-ID gate is the sole authenticator).
    if state.hmac_required {
        if state.hmac_secret.is_empty() {
            warn!("internal endpoint hit but HMAC secret is unconfigured");
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "inbound_disabled",
                "internal endpoint disabled — INBOUND_HMAC_SECRET unset",
            ));
        }
        // FIND-012: iat-bound replay window. Both the signature
        // header AND the timestamp header are required; the receiver
        // refuses requests whose `iat` is outside a 5-minute window
        // from the local clock. See `recor-hmac-sig` for the wire
        // contract.
        let sig_header = headers
            .get("x-recor-signature")
            .and_then(|v| v.to_str().ok());
        let ts_header = headers
            .get("x-recor-timestamp")
            .and_then(|v| v.to_str().ok());
        let mut cfg = recor_hmac_sig::VerifyConfig::primary(&state.hmac_secret);
        if !state.old_hmac_secret.is_empty() {
            cfg = cfg.with_old_secret(&state.old_hmac_secret);
        }
        if let Err(e) = recor_hmac_sig::verify(
            &cfg,
            &body,
            sig_header,
            ts_header,
            recor_hmac_sig::now_unix_seconds(),
        ) {
            warn!(error = %e, "inbound HMAC verification failed");
            let (kind, message): (&str, &str) = match e {
                recor_hmac_sig::VerifyError::TimestampMissing => {
                    ("missing_timestamp", "X-RECOR-Timestamp header required")
                }
                recor_hmac_sig::VerifyError::TimestampMalformed => (
                    "malformed_timestamp",
                    "X-RECOR-Timestamp must be unix seconds",
                ),
                recor_hmac_sig::VerifyError::OutsideWindow { .. } => (
                    "stale_request",
                    "request timestamp outside the replay window",
                ),
                recor_hmac_sig::VerifyError::SignatureMissing => {
                    ("missing_signature", "X-RECOR-Signature header required")
                }
                recor_hmac_sig::VerifyError::SignatureMalformed => {
                    ("malformed_signature", "X-RECOR-Signature must be hex")
                }
                recor_hmac_sig::VerifyError::BadSignature => {
                    ("bad_signature", "HMAC signature did not verify")
                }
            };
            return Err(error_response(StatusCode::UNAUTHORIZED, kind, message));
        }
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
                // PR-FATF-2.B: propagate FATF cascade fields from the
                // declaration-side wire to the V-engine domain snapshot.
                cascade_tier: o.cascade_tier,
                control_basis: o.control_basis,
                cascade_tier_b_ruled_out_evidence: o.cascade_tier_b_ruled_out_evidence,
                is_nominee: o.is_nominee,
                nominator_person_id: o.nominator_person_id,
            })
            .collect(),
        attestation_signed_by: payload.attestation.signed_by,
        attestation_signature_hex: payload.attestation.signature_hex,
        attestation_public_key_hex: payload.attestation.public_key_hex,
        receipt_hash_hex: payload.receipt_hash_hex,
        correlation_id: payload.correlation_id,
        submitted_at: payload.submitted_at,
        // PR-FATF-2.B: the declaration-side wire doesn't yet carry
        // adequacy_claims through the relay envelope (the relay
        // serialises the projection row; the projection columns are
        // unchanged in this PR). Stage 7 wiring + relay-envelope
        // extension is the follow-up.
        adequacy_claims: None,
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

/// Same dual-secret rotation primitive as the declaration service's
/// internal.rs. See that file for the full rotation procedure doc.
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
            "inbound request verified against OLD HMAC secret — rotation in progress"
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
        assert!(verify_hmac_with_rotation("current", "", b"x", &sig_current));
        assert!(!verify_hmac_with_rotation("current", "", b"x", &sig_old));
    }

    #[test]
    fn rotation_active_both_old_and_current_accepted() {
        let sig_current = hmac_hex("current", b"x");
        let sig_old = hmac_hex("old", b"x");
        assert!(verify_hmac_with_rotation("current", "old", b"x", &sig_current));
        assert!(verify_hmac_with_rotation("current", "old", b"x", &sig_old));
    }

    #[test]
    fn rotation_third_party_signature_still_rejected() {
        let sig_attacker = hmac_hex("attacker", b"x");
        assert!(!verify_hmac_with_rotation("current", "old", b"x", &sig_attacker));
        assert!(!verify_hmac_with_rotation("current", "", b"x", &sig_attacker));
    }
}

// Used by IntoResponse impls upstream.
impl IntoResponse for InboundResponse {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
