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
    VerificationLane,
};

/// The set of events the aggregate emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum DeclarationEvent {
    /// Declaration submitted; aggregate transitions from absent to Submitted.
    Submitted(DeclarationSubmittedV1),
    /// Verification engine returned a lane decision. Aggregate
    /// transitions to Accepted | InVerification | Rejected per lane.
    Verified(DeclarationVerifiedV1),
    /// This declaration was superseded by a successor declaration.
    /// Emitted against THIS aggregate (the one being replaced); the
    /// successor aggregate carries its own `DeclarationSubmittedV1`.
    /// Both events are written in the same DB transaction by the
    /// `SupersedeDeclaration` use case.
    Superseded(DeclarationSupersededV1),
}

impl DeclarationEvent {
    /// The event type discriminator stored alongside the payload in the
    /// event log. Used by the projection reader for routing.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Submitted(_) => "declaration.submitted.v1",
            Self::Verified(_) => "declaration.verified.v1",
            Self::Superseded(_) => "declaration.superseded.v1",
        }
    }

    /// The aggregate identifier the event applies to.
    pub fn declaration_id(&self) -> DeclarationId {
        match self {
            Self::Submitted(p) => p.declaration_id,
            Self::Verified(p) => p.declaration_id,
            Self::Superseded(p) => p.declaration_id,
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
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
    /// BLAKE3 hash of the canonical content of this declaration. The
    /// receipt the API returns to the declarant carries this hash; the
    /// declarant can verify their copy of the submission against the
    /// hash.
    pub receipt_hash_hex: String,
}

/// Verification outcome event — emitted when the Verification Engine
/// returns a lane decision through the writeback channel. The aggregate
/// transitions state per the lane (green → Accepted, yellow →
/// InVerification, red → Rejected).
///
/// The triplet (case_id, declaration_id) is unique: each declaration
/// has exactly one verification case at a time. Replays of the same
/// case_id MUST be idempotent at the use-case layer (no second event
/// written, no projection drift).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeclarationVerifiedV1 {
    pub declaration_id: DeclarationId,
    /// The verification engine's case identifier. The Declaration
    /// service persists this so consumers of the projection can join
    /// back to the verification case detail.
    pub verification_case_id: uuid::Uuid,
    /// Lane decision produced by the verification engine's lane router.
    pub lane: VerificationLane,
    /// Fused authenticity belief (Dempster-Shafer, m({True})) — stored
    /// for audit and downstream consumers. Range [0.0, 1.0].
    pub fused_authenticity_belief: f64,
    /// Fused authenticity plausibility (1 - m({False})). Range [0.0, 1.0].
    pub fused_authenticity_plausibility: f64,
    /// Fused risk belief (m({True}) of the risk frame). Range [0.0, 1.0].
    pub fused_risk_belief: f64,
    /// Time the verification engine completed the case. Reported by the
    /// engine, NOT the wall clock at writeback receipt time.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub completed_at: OffsetDateTime,
    /// Time the declaration service recorded the outcome.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub recorded_at: OffsetDateTime,
}

/// Marks a declaration as superseded by a successor. The successor
/// carries its own `DeclarationSubmittedV1` (referencing this id via
/// the `supersedes_declaration_id` field on the submit command).
///
/// Aggregate invariants:
///   - This aggregate must already exist (have at least one event).
///   - This aggregate must not have been superseded before
///     (idempotency anchor — supersede chains are strictly linear).
///   - This aggregate's state must be `Accepted` or `InVerification`.
///     Rejected and Draft declarations cannot be superseded — they
///     should be re-submitted instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationSupersededV1 {
    /// The declaration being superseded (the OLD one).
    pub declaration_id: DeclarationId,
    /// The new declaration that replaces it.
    pub superseded_by_declaration_id: DeclarationId,
    /// Time the supersede event was recorded.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub superseded_at: OffsetDateTime,
    /// Correlation token for tracing the supersede transaction across
    /// the two aggregates' event logs and the two outbox rows.
    pub correlation_id: uuid::Uuid,
}
