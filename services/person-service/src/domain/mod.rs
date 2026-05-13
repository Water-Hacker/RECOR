//! Domain layer for the Person service.
//!
//! Pure types and logic — no I/O, no async, no Tokio. Higher layers
//! depend on this module; this module depends only on `std`, `serde`,
//! `time`, and `uuid`.
//!
//! The `Person` aggregate is event-sourced: commands are validated and
//! produce events; events are applied to mutate aggregate state. The
//! Postgres `persons` projection is a derived read model rebuilt by
//! replaying events for a given person id.

pub mod aggregate;
pub mod command;
pub mod error;
pub mod event;
pub mod serde_helpers;
pub mod value_object;

pub use aggregate::PersonAggregate;
pub use command::{Command, MergePersons, RegisterPerson, UpdatePerson};
pub use error::DomainError;
pub use event::{PersonEvent, PersonMergedV1, PersonRegisteredV1, PersonUpdatedV1};
pub use value_object::{CanonicalFullName, IdDocument, Nationality, PersonAttributes, PersonId};
