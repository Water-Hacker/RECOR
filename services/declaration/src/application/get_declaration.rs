//! Get-declaration use case. Reads the projection (current-state) for
//! a given declaration id.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, DeclarationState, EntityId,
};
use crate::domain::attestation::CryptographicAttestation;

/// Projection shape returned to API consumers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclarationProjection {
    pub declaration_id: DeclarationId,
    pub entity_id: EntityId,
    pub declarant_principal: String,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    pub attestation: CryptographicAttestation,
    pub state: DeclarationState,
    pub version: u64,
    pub submitted_at: OffsetDateTime,
    pub receipt_hash_hex: String,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Error)]
pub enum GetError {
    #[error("declaration {0} not found")]
    NotFound(DeclarationId),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct GetDeclarationUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl GetDeclarationUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), fields(declaration_id = %id))]
    pub async fn execute(
        &self,
        id: DeclarationId,
    ) -> Result<DeclarationProjection, GetError> {
        match self.repository.load_projection(id).await? {
            Some(p) => Ok(p),
            None => Err(GetError::NotFound(id)),
        }
    }
}
