//! Domain errors. These represent invariant violations that the
//! aggregate refuses. API-layer translation maps them to HTTP status
//! codes; the aggregate itself doesn't know about HTTP.

use thiserror::Error;

use super::value_object::ValueObjectError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("person {0} not found")]
    PersonNotFound(uuid::Uuid),

    #[error("person {0} already registered; duplicate registration rejected (use Update or Merge)")]
    AlreadyRegistered(uuid::Uuid),

    #[error("cannot update person {0}: it has no Registered event")]
    UpdateBeforeRegister(uuid::Uuid),

    #[error("cannot merge person {0}: it has no Registered event")]
    MergeBeforeRegister(uuid::Uuid),

    #[error("person {person_id} is already merged into {into}; merges are strictly linear")]
    AlreadyMerged {
        person_id: uuid::Uuid,
        into: uuid::Uuid,
    },

    #[error("person {0} cannot be merged into itself")]
    MergeIntoSelf(uuid::Uuid),

    #[error("merge target {0} cannot be a person that has itself been merged into another record")]
    MergeTargetIsMerged(uuid::Uuid),

    #[error("actor_principal must not be empty")]
    EmptyActorPrincipal,

    #[error(transparent)]
    ValueObject(#[from] ValueObjectError),
}
