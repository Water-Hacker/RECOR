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
    AmendmentSet, BeneficialOwnerClaim, CorrectionSet, DeclarantRole, DeclarationId,
    DeclarationKind, EntityId, VerificationLane,
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
    /// A field on the declaration was amended in-place. Allowed only
    /// from Submitted or InVerification states; carries a fresh
    /// attestation over the AMENDED canonical form and records
    /// before/after snapshots for audit replay. Distinct from
    /// `Superseded`: the aggregate identity does not change, the row
    /// is updated in place. See R-DECL-3-AMEND.
    Amended(DeclarationAmendedV1),
    /// A pre-verification metadata correction was applied. Allowed
    /// only from Submitted state; before/after snapshots capture the
    /// changed metadata fields. The canonical declaration payload is
    /// unchanged — corrections cover display-only / metadata fields
    /// that would not warrant a re-signature of the declaration body
    /// itself, but the operation IS attested by a fresh signature
    /// over the corrected metadata bytes (D15: every consequential
    /// event carries cryptographic provenance). See R-DECL-3-CORRECT.
    Corrected(DeclarationCorrectedV1),
}

impl DeclarationEvent {
    /// The event type discriminator stored alongside the payload in the
    /// event log. Used by the projection reader for routing.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Submitted(_) => "declaration.submitted.v1",
            Self::Verified(_) => "declaration.verified.v1",
            Self::Superseded(_) => "declaration.superseded.v1",
            Self::Amended(_) => "declaration.amended.v1",
            Self::Corrected(_) => "declaration.corrected.v1",
        }
    }

    /// The aggregate identifier the event applies to.
    pub fn declaration_id(&self) -> DeclarationId {
        match self {
            Self::Submitted(p) => p.declaration_id,
            Self::Verified(p) => p.declaration_id,
            Self::Superseded(p) => p.declaration_id,
            Self::Amended(p) => p.declaration_id,
            Self::Corrected(p) => p.declaration_id,
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
    /// TODO-021 closure — explicit FATF R.24 c.24.8 adequacy claims.
    /// Required for new declarations; absent on historical events that
    /// pre-date this migration (`#[serde(default)]` deserialises older
    /// payloads with `None` so replay is forward-compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adequacy_claims: Option<super::attestation::AdequacyClaims>,
    /// PR-FATF-4 / TODO-005 — FATF R.24 c.24.8 fn 29: declarant-asserted
    /// timestamp of the underlying BO control event. Optional on the
    /// event for back-compat with historical events.
    #[serde(default, with = "crate::domain::serde_helpers::iso_datetime_option")]
    pub last_event_observed_at: Option<OffsetDateTime>,
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

/// Records an in-place amendment of declaration fields. Carries both
/// the BEFORE and AFTER snapshots of every amendable field so the
/// event log is sufficient to replay the projection state. The fresh
/// attestation in `attestation` is signed over the AMENDED canonical
/// form by the declarant — proof the declarant stands behind the new
/// values (D15: cryptographic provenance on every consequential event).
///
/// Aggregate invariants (validated by `handle_amend`):
///   - The aggregate must have a prior Submitted event.
///   - The aggregate must NOT be Superseded.
///   - The current state must be `Submitted` or `InVerification`.
///     Accepted declarations require Supersede (more transparency
///     for downstream consumers); Rejected declarations require
///     re-submission.
///   - `after.beneficial_owners` must still sum to 10_000 basis points.
///   - The attestation principal must match the declarant principal
///     stored on the aggregate (only the owner can amend).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationAmendedV1 {
    pub declaration_id: DeclarationId,
    /// Snapshot of the amendable fields BEFORE the amendment was
    /// applied — derived from the aggregate state at the time the
    /// command was handled.
    pub before: AmendmentSet,
    /// Snapshot of the amendable fields AFTER the amendment is applied.
    pub after: AmendmentSet,
    /// Fresh Ed25519 attestation by the declarant over the AMENDED
    /// canonical form. Verified at the API boundary before the
    /// command reaches the aggregate; recorded in the event log so a
    /// replay can re-verify provenance.
    pub attestation: CryptographicAttestation,
    /// Time the amendment was recorded.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub amended_at: OffsetDateTime,
    /// Correlation token for tracing the amendment across the event
    /// log and the outbox row.
    pub correlation_id: uuid::Uuid,
}

/// Records a pre-verification metadata correction. Smaller-scope than
/// `Amended` (the declaration payload is unchanged); the correction
/// only touches the projection's metadata columns. Allowed only from
/// `Submitted` state — any later state must use Amend or Supersede.
///
/// Aggregate invariants (validated by `handle_correct`):
///   - The aggregate must have a prior Submitted event.
///   - The aggregate must NOT be Superseded.
///   - The current state MUST be `Submitted` (strictly).
///   - The attestation principal must match the declarant principal
///     stored on the aggregate (only the owner can correct).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationCorrectedV1 {
    pub declaration_id: DeclarationId,
    /// Snapshot of correctable fields BEFORE.
    pub before: CorrectionSet,
    /// Snapshot of correctable fields AFTER.
    pub after: CorrectionSet,
    /// Fresh attestation by the declarant over the corrected metadata
    /// bytes — D15 holds even when the canonical declaration body is
    /// unchanged.
    pub attestation: CryptographicAttestation,
    /// Time the correction was recorded.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub corrected_at: OffsetDateTime,
    /// Correlation token for tracing.
    pub correlation_id: uuid::Uuid,
}
