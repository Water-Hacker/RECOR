//! Domain errors. These represent invariant violations that the
//! aggregate refuses. API-layer translation maps them to HTTP status
//! codes; the aggregate itself doesn't know about HTTP.

use thiserror::Error;

use super::value_object::ValueObjectError;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("entity {0} already exists; duplicate registration rejected (use Update or Dissolve)")]
    AlreadyRegistered(uuid::Uuid),

    #[error("entity {0} not found")]
    EntityNotFound(uuid::Uuid),

    #[error("entity {entity_id} is already dissolved on {dissolved_at}; idempotent dissolution requires the same date")]
    AlreadyDissolved {
        entity_id: uuid::Uuid,
        dissolved_at: time::Date,
    },

    #[error("dissolution date {dissolved_at} is before foundation date {founded_at} for entity {entity_id}")]
    DissolutionBeforeFoundation {
        entity_id: uuid::Uuid,
        founded_at: time::Date,
        dissolved_at: time::Date,
    },

    #[error("update on entity {0} rejected: aggregate has been dissolved; create a successor entity instead")]
    UpdateOnDissolvedEntity(uuid::Uuid),

    #[error("update on entity {0} before any registration event; cannot update a non-existent entity")]
    UpdateBeforeRegistration(uuid::Uuid),

    #[error("dissolution of entity {0} before any registration event; cannot dissolve a non-existent entity")]
    DissolveBeforeRegistration(uuid::Uuid),

    #[error("founded_at date {founded_at} is in the future (after now {now})")]
    FoundedAtInFuture {
        founded_at: time::Date,
        now: time::Date,
    },

    #[error(transparent)]
    ValueObject(#[from] ValueObjectError),
}
