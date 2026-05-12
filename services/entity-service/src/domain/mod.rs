//! Domain layer for the Entity service.
//!
//! Pure types and logic — no I/O, no async, no Tokio. Higher layers
//! depend on this module; this module depends only on `std`, `serde`,
//! `time`, and `uuid`.
//!
//! The `Entity` aggregate is event-sourced: commands are validated and
//! produce events; events are applied to mutate aggregate state. The
//! aggregate is the source of truth for state transitions; the
//! Postgres projection is a derived read model.

pub mod aggregate;
pub mod command;
pub mod error;
pub mod event;
pub mod serde_helpers;
pub mod value_object;

pub use aggregate::EntityAggregate;
pub use command::{Command, DissolveEntity, RegisterEntity, UpdateEntity};
pub use error::DomainError;
pub use event::{EntityDissolvedV1, EntityEvent, EntityRegisteredV1, EntityUpdatedV1};
pub use value_object::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegistrationNumber, UpdatableFields,
};
