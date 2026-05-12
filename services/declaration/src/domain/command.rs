//! Commands accepted by the Declaration aggregate.
//!
//! A command is an intent that has not yet been validated against the
//! aggregate's state. The aggregate's `handle()` method validates the
//! command and either produces an event or rejects with a domain error.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::attestation::CryptographicAttestation;
use super::value_object::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, EntityId,
    VerificationLane,
};

/// The set of commands the aggregate accepts. Submit creates the
/// aggregate; RecordVerificationOutcome transitions it after the
/// Verification Engine returns a lane decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command_type", rename_all = "snake_case")]
pub enum Command {
    Submit(SubmitDeclaration),
    RecordVerificationOutcome(RecordVerificationOutcome),
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
