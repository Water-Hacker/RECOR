//! Get-entity use case. Reads the projection (current-state) for a
//! given entity id.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::application::port::{EntityRepository, RepositoryError};
use crate::domain::{EntityId, EntityType, Jurisdiction, RegistrationNumber};

/// Projection shape returned to API consumers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct EntityProjection {
    pub entity_id: EntityId,
    pub canonical_name: String,
    pub entity_type: EntityType,
    pub jurisdiction: Jurisdiction,
    pub registration_number_in_jurisdiction: RegistrationNumber,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2024-01-15")]
    pub founded_at: time::Date,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date, example = "2026-04-01")]
    pub dissolved_at: Option<time::Date>,
    pub version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum GetError {
    #[error("entity {0} not found")]
    NotFound(EntityId),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct GetEntityUseCase {
    repository: Arc<dyn EntityRepository>,
}

impl GetEntityUseCase {
    pub fn new(repository: Arc<dyn EntityRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), fields(entity_id = %id))]
    pub async fn execute(&self, id: EntityId) -> Result<EntityProjection, GetError> {
        match self.repository.load_projection(id).await? {
            Some(p) => Ok(p),
            None => Err(GetError::NotFound(id)),
        }
    }
}
