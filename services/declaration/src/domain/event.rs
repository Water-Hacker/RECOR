//! Events emitted by the Declaration aggregate.
//!
//! Events are the source of truth for aggregate state. They are
//! persisted append-only in the `declaration_events` table. The
//! current-state `declarations` projection is rebuilt by replaying
//! events for an aggregate id.
//!
//! Event payloads are versioned — `DeclarationSubmittedV1` etc. — so
//! that a schema migration produces a new variant rather than a
//! breaking change to an existing one. Old events remain replayable
//! forever; the aggregate's `apply()` method handles every version.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::attestation::CryptographicAttestation;
use super::value_object::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, EntityId,
};

/// The set of events the aggregate emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum DeclarationEvent {
    /// Declaration submitted; aggregate transitions from absent to Submitted.
    Submitted(DeclarationSubmittedV1),
}

impl DeclarationEvent {
    /// The event type discriminator stored alongside the payload in the
    /// event log. Used by the projection reader for routing.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Submitted(_) => "declaration.submitted.v1",
        }
    }

    /// The aggregate identifier the event applies to.
    pub fn declaration_id(&self) -> DeclarationId {
        match self {
            Self::Submitted(p) => p.declaration_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationSubmittedV1 {
    pub declaration_id: DeclarationId,
    pub entity_id: EntityId,
    pub declarant_principal: String,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    pub attestation: CryptographicAttestation,
    pub submitted_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
    /// BLAKE3 hash of the canonical content of this declaration. The
    /// receipt the API returns to the declarant carries this hash; the
    /// declarant can verify their copy of the submission against the
    /// hash.
    pub receipt_hash_hex: String,
}
