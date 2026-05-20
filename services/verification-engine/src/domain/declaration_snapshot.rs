//! Decoupled snapshot of a Declaration. The Verification Engine
//! receives this shape from the Declaration service via the API (or, in
//! the future, via Kafka outbox events). The snapshot is intentionally
//! a structural duplicate of the Declaration service's
//! `DeclarationSubmittedV1` event — service boundaries dictate
//! independent types even when they happen to match field-for-field.
//!
//! When the Declaration service updates its event schema, this service
//! updates its snapshot type and the consumer (this file) handles both
//! versions during the rollover window.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationSnapshot {
    pub declaration_id: Uuid,
    pub entity_id: Uuid,
    pub declarant_principal: String,
    pub declarant_role: String,
    pub kind: String,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<OwnerSnapshot>,
    pub attestation_signed_by: String,
    pub attestation_signature_hex: String,
    pub attestation_public_key_hex: String,
    pub receipt_hash_hex: String,
    pub correlation_id: Uuid,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: time::OffsetDateTime,
    /// PR-FATF-2.B — FATF R.24 c.24.8 adequacy claims block carried
    /// over from the declaration. Optional for back-compat with
    /// snapshots produced by pre-FATF-cascade declaration deploys.
    /// Stage 7 cross-source verification (TODO-013) reads this to
    /// cross-reference perjury claims against later contradictions.
    #[serde(default)]
    pub adequacy_claims: Option<AdequacyClaimsSnapshot>,
}

/// Verification-engine-side mirror of the declaration service's
/// `AdequacyClaims`. Separate type per service boundary discipline —
/// the V-engine doesn't depend on the declaration crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdequacyClaimsSnapshot {
    pub adequate: bool,
    pub accurate: bool,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub up_to_date_as_of: time::OffsetDateTime,
    pub legal_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerSnapshot {
    pub person_id: Uuid,
    pub ownership_basis_points: u32,
    pub interest_kind: String,
    /// PR-FATF-2.B — FATF R.24 c.24.6 cascade tier (string-typed at
    /// the V-engine boundary; Stage 7 parses + validates).
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
