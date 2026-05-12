//! Commands accepted by the Declaration aggregate.
//!
//! A command is an intent that has not yet been validated against the
//! aggregate's state. The aggregate's `handle()` method validates the
//! command and either produces an event or rejects with a domain error.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::attestation::CryptographicAttestation;
use super::value_object::{
    AmendmentSet, BeneficialOwnerClaim, CorrectionSet, DeclarantRole, DeclarationId,
    DeclarationKind, EntityId, VerificationLane,
};

/// The set of commands the aggregate accepts. Submit creates the
/// aggregate; RecordVerificationOutcome transitions it after the
/// Verification Engine returns a lane decision; SupersedeDeclaration
/// closes a declaration's lifecycle when a successor replaces it.
/// AmendDeclaration updates the aggregate in place (still-mutable
/// states only). CorrectDeclaration is the narrower pre-verification
/// metadata sibling of Amend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command_type", rename_all = "snake_case")]
pub enum Command {
    Submit(SubmitDeclaration),
    RecordVerificationOutcome(RecordVerificationOutcome),
    Supersede(SupersedeDeclaration),
    Amend(AmendDeclaration),
    Correct(CorrectDeclaration),
}

/// Submit a new beneficial ownership declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitDeclaration {
    pub declaration_id: DeclarationId,
    pub entity_id: EntityId,
    pub declarant_principal: String,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    pub attestation: CryptographicAttestation,
    /// Time the API received the request, set by the API layer.
    pub submitted_at: OffsetDateTime,
    /// Correlation token for tracing across services.
    pub correlation_id: uuid::Uuid,
}

/// Record the Verification Engine's lane decision against a declaration.
/// Issued by the internal /v1/internal/verification-outcomes endpoint
/// after the Declaration service's writeback receiver authenticates the
/// HMAC envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordVerificationOutcome {
    pub declaration_id: DeclarationId,
    pub verification_case_id: uuid::Uuid,
    pub lane: VerificationLane,
    pub fused_authenticity_belief: f64,
    pub fused_authenticity_plausibility: f64,
    pub fused_risk_belief: f64,
    pub completed_at: OffsetDateTime,
}

/// Supersede an existing declaration with a new one. The new
/// declaration's payload (entity_id, owners, attestation, etc.) is the
/// same shape as `SubmitDeclaration`; it gets a fresh declaration_id.
/// The OLD declaration referenced here transitions to `Superseded`
/// state atomically with the new declaration's `Submitted`.
///
/// Authorisation: handled at the API layer — the declarant principal
/// must own the OLD declaration AND be authorised to declare for the
/// same entity. The domain aggregate does not re-check authz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersedeDeclaration {
    /// The declaration_id being superseded.
    pub supersedes_declaration_id: DeclarationId,
    /// The new declaration's full submit payload.
    pub new_declaration: SubmitDeclaration,
}

/// Amend a field in place on an existing declaration. Only valid from
/// `Submitted` or `InVerification`; later states require `Supersede`.
///
/// `declarant_principal` carries the authenticated identity (D17 zero
/// trust) so the aggregate can refuse amendments by anyone but the
/// owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendDeclaration {
    pub declaration_id: DeclarationId,
    pub declarant_principal: String,
    pub amendments: AmendmentSet,
    /// Fresh Ed25519 attestation over the amended canonical form,
    /// produced by the declarant. Re-verified at the API boundary
    /// before the command reaches the aggregate; the aggregate stores
    /// it on the emitted event.
    pub attestation: CryptographicAttestation,
    pub submitted_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

/// Apply a pre-verification metadata correction. Only valid from
/// `Submitted`; everywhere else use `Amend` or `Supersede`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectDeclaration {
    pub declaration_id: DeclarationId,
    pub declarant_principal: String,
    pub corrections: CorrectionSet,
    /// Fresh attestation over the corrected metadata bytes — D15
    /// applies to every consequential event.
    pub attestation: CryptographicAttestation,
    pub submitted_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
