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
};

/// The set of commands the aggregate accepts. Today, only Submit. Future:
/// Amend, Withdraw, Supersede.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command_type", rename_all = "snake_case")]
pub enum Command {
    Submit(SubmitDeclaration),
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
