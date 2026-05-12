//! Commands accepted by the Entity aggregate.
//!
//! Commands carry the raw inputs from the API layer; they are validated
//! by the aggregate, which either rejects with `DomainError` or produces
//! an event. Commands are not persisted (events are); the wire-level
//! request DTOs in `crate::api::dto` translate into these commands.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::value_object::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegistrationNumber, UpdatableFields,
};

/// The set of commands the aggregate accepts.
#[derive(Debug, Clone)]
pub enum Command {
    Register(RegisterEntity),
    Update(UpdateEntity),
    Dissolve(DissolveEntity),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterEntity {
    pub entity_id: EntityId,
    pub canonical_name: CanonicalName,
    pub entity_type: EntityType,
    pub jurisdiction: Jurisdiction,
    pub registration_number_in_jurisdiction: RegistrationNumber,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub founded_at: time::Date,
    pub registered_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub registered_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEntity {
    pub entity_id: EntityId,
    pub after: UpdatableFields,
    pub updated_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissolveEntity {
    pub entity_id: EntityId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolved_at: time::Date,
    pub dissolved_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub recorded_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
