//! Commands accepted by the Person aggregate.
//!
//! A command is an intent that has not yet been validated against the
//! aggregate's state. The aggregate's `handle_*` methods validate the
//! command and either produce an event or reject with a domain error.
//!
//! Authorisation: every command carries an `actor_principal` field
//! that the API layer sources from the authenticated principal
//! (D17 zero trust). The aggregate stores it on the emitted event so
//! the audit chain attributes every state change.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::value_object::{PersonAttributes, PersonId};

/// The set of commands the aggregate accepts. Register creates the
/// aggregate; Update mutates it in place; Merge collapses a duplicate
/// into a surviving canonical person.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command_type", rename_all = "snake_case")]
pub enum Command {
    Register(RegisterPerson),
    Update(UpdatePerson),
    Merge(MergePersons),
}

/// Register a new natural person.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPerson {
    pub person_id: PersonId,
    pub attributes: PersonAttributes,
    /// Principal that issued the registration (operator or
    /// declarant-self-registration). Sourced from auth; never from the
    /// request body. Stored on the emitted event as provenance.
    pub actor_principal: String,
    /// Time the API received the request.
    pub registered_at: OffsetDateTime,
    /// Correlation token for tracing across services.
    pub correlation_id: uuid::Uuid,
}

/// Update an existing person in place.
///
/// Update is allowed only on persons that have not been merged into
/// another. Attempts to update a merged-out shell return
/// `DomainError::AlreadyMerged`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePerson {
    pub person_id: PersonId,
    pub attributes: PersonAttributes,
    pub actor_principal: String,
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

/// Merge `from_person_id` INTO `into_person_id`. The "from" aggregate
/// is the duplicate being collapsed; the "into" aggregate is the
/// surviving canonical record. After the merge:
///   - the "from" projection carries `merged_into = Some(into)` and
///     is no longer eligible for Update.
///   - declarations referencing the "from" id continue to resolve
///     through the merge pointer (the API's GET handler follows the
///     chain).
///
/// Admin-only — the API layer gates this against the
/// `admin_principals` allow-list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePersons {
    pub from_person_id: PersonId,
    pub into_person_id: PersonId,
    pub actor_principal: String,
    pub merged_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
