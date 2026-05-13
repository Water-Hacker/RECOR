//! Events emitted by the Entity aggregate.
//!
//! Events are the source of truth for aggregate state. They are
//! persisted append-only in the `entity_events` table. The
//! current-state `entities` projection is rebuilt by replaying events
//! for an aggregate id.
//!
//! Event payloads are versioned — `EntityRegisteredV1` etc. — so a
//! schema migration produces a new variant rather than a breaking
//! change to an existing one. Old events remain replayable forever;
//! the aggregate's `apply()` method handles every version.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::value_object::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegistrationNumber, UpdatableFields,
};

/// The set of events the aggregate emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum EntityEvent {
    /// Entity registered; aggregate transitions from absent to Active.
    Registered(EntityRegisteredV1),
    /// Mutable fields on the entity were updated in place. Identity
    /// fields (jurisdiction, registration_number_in_jurisdiction,
    /// founded_at) cannot change via Update.
    Updated(EntityUpdatedV1),
    /// Entity has been dissolved. Terminal state (no further Updates).
    Dissolved(EntityDissolvedV1),
}

impl EntityEvent {
    /// The event-type discriminator stored alongside the payload in the
    /// event log. Used by the projection reader for routing.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Registered(_) => "entity.registered.v1",
            Self::Updated(_) => "entity.updated.v1",
            Self::Dissolved(_) => "entity.dissolved.v1",
        }
    }

    /// The aggregate identifier the event applies to.
    pub fn entity_id(&self) -> EntityId {
        match self {
            Self::Registered(p) => p.entity_id,
            Self::Updated(p) => p.entity_id,
            Self::Dissolved(p) => p.entity_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityRegisteredV1 {
    pub entity_id: EntityId,
    pub canonical_name: CanonicalName,
    pub entity_type: EntityType,
    pub jurisdiction: Jurisdiction,
    pub registration_number_in_jurisdiction: RegistrationNumber,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub founded_at: time::Date,
    /// Authenticated principal that registered the entity.
    pub registered_by_principal: String,
    /// Time the entity was recorded.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub registered_at: OffsetDateTime,
    /// Correlation token for tracing the registration across the event
    /// log and the outbox row.
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityUpdatedV1 {
    pub entity_id: EntityId,
    /// Snapshot of the updatable fields BEFORE the update was applied —
    /// derived from the aggregate state at the time the command was
    /// handled. The event log is sufficient to replay the projection.
    pub before: UpdatableFields,
    /// Snapshot of the updatable fields AFTER the update is applied.
    pub after: UpdatableFields,
    /// Authenticated principal that updated the entity.
    pub updated_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityDissolvedV1 {
    pub entity_id: EntityId,
    /// Date the entity was dissolved (gazette/registry-issued date).
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolved_at: time::Date,
    /// Authenticated administrative principal that recorded the
    /// dissolution. Dissolutions are admin-allowlisted at the API layer.
    pub dissolved_by_principal: String,
    /// Time the dissolution was recorded by the service.
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub recorded_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
