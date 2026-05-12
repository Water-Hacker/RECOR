//! Wire DTOs for the REST API. Mirrors the shape of the canonical
//! domain types but stays decoupled so the public contract can evolve
//! independently of the internal aggregate representation.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::application::{EntityProjection, RegisterReceipt};
use crate::domain::{
    CanonicalName, DissolveEntity, DomainError, EntityId, EntityType, Jurisdiction, RegisterEntity,
    RegistrationNumber, UpdatableFields, UpdateEntity,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterEntityRequest {
    pub canonical_name: String,
    /// Entity type. One of: `sa`, `sarl`, `partnership`, `trust`, `other`.
    /// When `kind` is `other`, `label` is required.
    pub entity_type: EntityTypeDto,
    /// ISO-3166-1 alpha-2 jurisdiction code (e.g. "CM").
    #[schema(value_type = String, pattern = "^[A-Z]{2}$")]
    pub jurisdiction: String,
    pub registration_number_in_jurisdiction: String,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub founded_at: time::Date,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "label", rename_all = "snake_case")]
pub enum EntityTypeDto {
    Sa,
    Sarl,
    Partnership,
    Trust,
    Other(String),
}

impl EntityTypeDto {
    pub fn into_domain(self) -> Result<EntityType, DomainError> {
        match self {
            Self::Sa => Ok(EntityType::Sa),
            Self::Sarl => Ok(EntityType::Sarl),
            Self::Partnership => Ok(EntityType::Partnership),
            Self::Trust => Ok(EntityType::Trust),
            Self::Other(l) => EntityType::try_from_wire("other", Some(&l))
                .map_err(DomainError::ValueObject),
        }
    }

    pub fn from_domain(t: &EntityType) -> Self {
        match t {
            EntityType::Sa => Self::Sa,
            EntityType::Sarl => Self::Sarl,
            EntityType::Partnership => Self::Partnership,
            EntityType::Trust => Self::Trust,
            EntityType::Other(l) => Self::Other(l.clone()),
        }
    }
}

impl RegisterEntityRequest {
    pub fn into_command(
        self,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> Result<RegisterEntity, DomainError> {
        let canonical_name = CanonicalName::try_from_str(&self.canonical_name)
            .map_err(DomainError::ValueObject)?;
        let jurisdiction = Jurisdiction::try_from_str(&self.jurisdiction)
            .map_err(DomainError::ValueObject)?;
        let registration_number =
            RegistrationNumber::try_from_str(&self.registration_number_in_jurisdiction)
                .map_err(DomainError::ValueObject)?;
        let entity_type = self.entity_type.into_domain()?;
        Ok(RegisterEntity {
            entity_id: EntityId::new(),
            canonical_name,
            entity_type,
            jurisdiction,
            registration_number_in_jurisdiction: registration_number,
            founded_at: self.founded_at,
            registered_by_principal: actor_principal,
            registered_at: OffsetDateTime::now_utc(),
            correlation_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterEntityResponse {
    pub entity_id: EntityId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub registered_at: OffsetDateTime,
    /// Self-link to the projection.
    pub self_url: String,
}

impl RegisterEntityResponse {
    pub fn from_receipt(receipt: RegisterReceipt, base_url: &str) -> Self {
        let self_url = format!("{}/v1/entities/{}", base_url, receipt.entity_id);
        Self {
            entity_id: receipt.entity_id,
            registered_at: receipt.registered_at,
            self_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateEntityRequest {
    pub canonical_name: String,
    pub entity_type: EntityTypeDto,
}

impl UpdateEntityRequest {
    pub fn into_command(
        self,
        entity_id: EntityId,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> Result<UpdateEntity, DomainError> {
        let canonical_name = CanonicalName::try_from_str(&self.canonical_name)
            .map_err(DomainError::ValueObject)?;
        let entity_type = self.entity_type.into_domain()?;
        Ok(UpdateEntity {
            entity_id,
            after: UpdatableFields {
                canonical_name,
                entity_type,
            },
            updated_by_principal: actor_principal,
            updated_at: OffsetDateTime::now_utc(),
            correlation_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DissolveEntityRequest {
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub dissolved_at: time::Date,
}

impl DissolveEntityRequest {
    pub fn into_command(
        self,
        entity_id: EntityId,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> DissolveEntity {
        DissolveEntity {
            entity_id,
            dissolved_at: self.dissolved_at,
            dissolved_by_principal: actor_principal,
            recorded_at: OffsetDateTime::now_utc(),
            correlation_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetEntityResponse {
    pub entity_id: EntityId,
    pub canonical_name: String,
    pub entity_type: EntityTypeDto,
    pub jurisdiction: Jurisdiction,
    pub registration_number_in_jurisdiction: RegistrationNumber,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub founded_at: time::Date,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date)]
    pub dissolved_at: Option<time::Date>,
    pub version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

impl From<EntityProjection> for GetEntityResponse {
    fn from(p: EntityProjection) -> Self {
        Self {
            entity_id: p.entity_id,
            canonical_name: p.canonical_name,
            entity_type: EntityTypeDto::from_domain(&p.entity_type),
            jurisdiction: p.jurisdiction,
            registration_number_in_jurisdiction: p.registration_number_in_jurisdiction,
            founded_at: p.founded_at,
            dissolved_at: p.dissolved_at,
            version: p.version,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SearchEntitiesResponse {
    pub items: Vec<GetEntityResponse>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DissolveResponse {
    pub entity_id: EntityId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub dissolved_at: time::Date,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateResponse {
    pub entity_id: EntityId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthzResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReadyzResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
}
