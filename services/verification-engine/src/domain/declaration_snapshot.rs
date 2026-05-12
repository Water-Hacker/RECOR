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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerSnapshot {
    pub person_id: Uuid,
    pub ownership_basis_points: u32,
    pub interest_kind: String,
}
