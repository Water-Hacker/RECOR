//! Get-declaration use case. Reads the projection (current-state) for
//! a given declaration id.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, DeclarationState, EntityId,
    VerificationLane,
};
use crate::domain::attestation::CryptographicAttestation;

/// Projection shape returned to API consumers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    /// Downstream verification projection. Populated once the
    /// Verification Engine writeback has recorded an outcome. Distinct
    /// from `state` — which is the aggregate's full lifecycle — so the
    /// projection captures both "what the aggregate became" and "what
    /// the verification said". v1 keeps them aligned but the columns
    /// stay independent so future amendment flows can decouple them.
    pub verification_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_lane: Option<VerificationLane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_case_id: Option<uuid::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<OffsetDateTime>,

    /// If this declaration replaced an earlier one, the earlier id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_declaration_id: Option<DeclarationId>,
    /// If this declaration has been replaced, the successor's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_declaration_id: Option<DeclarationId>,
    /// Time this declaration was superseded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_at: Option<OffsetDateTime>,

    /// Most recent amendment timestamp. `None` if the declaration has
    /// never been amended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amended_at: Option<OffsetDateTime>,
    /// Free-form metadata annotation set by the declarant via a
    /// Correct command. Pre-verification only; `None` until a
    /// correction is applied. See R-DECL-3-CORRECT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_notes: Option<String>,
    /// Most recent correction timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrected_at: Option<OffsetDateTime>,
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
