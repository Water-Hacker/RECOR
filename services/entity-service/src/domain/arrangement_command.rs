//! Commands accepted by the `Arrangement` aggregate.
//!
//! Commands carry the raw inputs from the API layer; they are validated
//! by the aggregate, which either rejects with `ArrangementDomainError`
//! or produces an event. Commands are not persisted (events are).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::arrangement_value_object::{
    ArrangementId, ArrangementKind, ArrangementUpdatableFields, GoverningLawJurisdiction,
};

#[derive(Debug, Clone)]
pub enum ArrangementCommand {
    Register(RegisterArrangement),
    Update(UpdateArrangement),
    Dissolve(DissolveArrangement),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterArrangement {
    pub arrangement_id: ArrangementId,
    pub arrangement_kind: ArrangementKind,
    pub governing_law_jurisdiction: GoverningLawJurisdiction,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub constitution_date: time::Date,
    pub fields: ArrangementUpdatableFields,
    pub registered_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub registered_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateArrangement {
    pub arrangement_id: ArrangementId,
    pub after: ArrangementUpdatableFields,
    pub updated_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissolveArrangement {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolution_date: time::Date,
    pub dissolved_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub recorded_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
