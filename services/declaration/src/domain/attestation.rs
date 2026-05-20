//! Cryptographic attestation by the declarant. Every submitted
//! declaration carries an Ed25519 signature over the canonical form of
//! the declared content. The signature is verified by the API layer
//! before the command reaches the aggregate.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Algorithm identifier for the signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureAlgorithm {
    /// Ed25519 — the platform's canonical signature scheme per
    /// Architecture V4 P11.
    Ed25519,
}

impl SignatureAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
        }
    }
}

/// A signed attestation submitted with a declaration.
///
/// The signature is over the canonical JSON form of the declaration
/// payload (entity_id, declarant_principal, declarant_role,
/// declaration_kind, effective_from, beneficial_owners,
/// adequacy_claims, nonce_hex), serialised with sorted keys, no
/// whitespace, UTF-8 — i.e. JCS (RFC 8785). The nonce protects
/// against replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CryptographicAttestation {
    /// Principal identifier the signature is bound to. Matches the
    /// authenticated principal at the API boundary.
    pub signed_by: String,
    /// Signature algorithm. Always Ed25519 in this version.
    pub signature_algorithm: SignatureAlgorithm,
    /// The signature, hex-encoded.
    pub signature_hex: String,
    /// The declarant's public key, hex-encoded. The verifier resolves
    /// this against the registered keys for the principal; mismatch
    /// rejects the attestation.
    pub public_key_hex: String,
    /// Random nonce, hex-encoded. The verifier records nonces per
    /// principal to prevent replay.
    pub nonce_hex: String,
}

/// TODO-021 closure — explicit FATF claims block.
///
/// FATF R.24 c.24.8 requires BO data to be "adequate, accurate, and
/// up-to-date". The cryptographic attestation by itself only proves
/// authorship of the bytes; the explicit claims block proves the
/// declarant *asserts* the three properties, which is the surface a
/// sanctions workflow (TODO-004) needs to demonstrate perjury when
/// a claim is later shown false.
///
/// The block is included in the canonical payload bytes (the bytes
/// signed by Ed25519) so any tampering with claims invalidates the
/// signature. Historical declarations that pre-date this migration
/// deserialise with `None` via `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdequacyClaims {
    /// The declarant asserts the BO data is *adequate* —
    /// sufficient to identify each natural person per c.24.8 fn 27
    /// (full name, all nationalities, full DOB + place, residential
    /// address, national ID, TIN or equivalent). Required for new
    /// declarations; missing on legacy projections.
    pub adequate: bool,
    /// The declarant asserts the BO data is *accurate* — verified by
    /// reliable, independently sourced documents per c.24.8 fn 28.
    pub accurate: bool,
    /// The declarant asserts the BO data is *up-to-date as of* the
    /// given timestamp (c.24.8 fn 29; FATF benchmark: within 1 month
    /// of any change).
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub up_to_date_as_of: time::OffsetDateTime,
    /// Free-text legal basis — typically a citation to the CEMAC
    /// Règlement, Cameroon AML law, or the obligation under which the
    /// declarant files. The aggregate validates length only; semantic
    /// review is the back-office responsibility.
    pub legal_basis: String,
}

impl CryptographicAttestation {
    /// Verify the attestation signature against the canonical payload.
    /// The caller computes the canonical bytes; this function does not
    /// canonicalise — separation of concerns keeps the verifier
    /// algorithm-pure.
    pub fn verify_against(&self, canonical_payload: &[u8]) -> Result<(), AttestationError> {
        if self.signature_algorithm != SignatureAlgorithm::Ed25519 {
            return Err(AttestationError::UnsupportedAlgorithm(
                self.signature_algorithm,
            ));
        }
        let public_key_bytes =
            hex::decode(&self.public_key_hex).map_err(|_| AttestationError::MalformedPublicKey)?;
        let public_key_bytes: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| AttestationError::MalformedPublicKey)?;
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|_| AttestationError::MalformedPublicKey)?;
        let signature_bytes =
            hex::decode(&self.signature_hex).map_err(|_| AttestationError::MalformedSignature)?;
        let signature_bytes: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| AttestationError::MalformedSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify_strict(canonical_payload, &signature)
            .map_err(|_| AttestationError::SignatureVerificationFailed)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AttestationError {
    #[error("unsupported signature algorithm: {0:?}")]
    UnsupportedAlgorithm(SignatureAlgorithm),
    #[error("malformed public key")]
    MalformedPublicKey,
    #[error("malformed signature")]
    MalformedSignature,
    #[error("signature verification failed")]
    SignatureVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fresh_keypair() -> SigningKey {
        // Deterministic for test reproducibility.
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn build_attestation(signing_key: &SigningKey, payload: &[u8]) -> CryptographicAttestation {
        let verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(payload);
        CryptographicAttestation {
            signed_by: "spiffe://recor.cm/test-declarant".into(),
            signature_algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: hex::encode(signature.to_bytes()),
            public_key_hex: hex::encode(verifying_key.to_bytes()),
            nonce_hex: hex::encode([0u8; 16]),
        }
    }

    #[test]
    fn valid_attestation_verifies() {
        let key = fresh_keypair();
        let payload = b"the canonical payload bytes";
        let att = build_attestation(&key, payload);
        assert!(att.verify_against(payload).is_ok());
    }

    #[test]
    fn tampered_payload_rejects() {
        let key = fresh_keypair();
        let payload = b"the canonical payload bytes";
        let att = build_attestation(&key, payload);
        assert_eq!(
            att.verify_against(b"a different payload"),
            Err(AttestationError::SignatureVerificationFailed)
        );
    }

    #[test]
    fn malformed_signature_rejects() {
        let key = fresh_keypair();
        let payload = b"x";
        let mut att = build_attestation(&key, payload);
        att.signature_hex = "deadbeef".into(); // too short
        assert_eq!(
            att.verify_against(payload),
            Err(AttestationError::MalformedSignature)
        );
    }

    #[test]
    fn malformed_public_key_rejects() {
        let key = fresh_keypair();
        let payload = b"x";
        let mut att = build_attestation(&key, payload);
        att.public_key_hex = "00".into();
        assert_eq!(
            att.verify_against(payload),
            Err(AttestationError::MalformedPublicKey)
        );
    }

    #[test]
    fn substituted_public_key_rejects() {
        // Sign with key A, but present key B's public key.
        let key_a = fresh_keypair();
        let payload = b"x";
        let mut att = build_attestation(&key_a, payload);
        let key_b = SigningKey::from_bytes(&[42u8; 32]);
        att.public_key_hex = hex::encode(key_b.verifying_key().to_bytes());
        assert_eq!(
            att.verify_against(payload),
            Err(AttestationError::SignatureVerificationFailed)
        );
    }
}
