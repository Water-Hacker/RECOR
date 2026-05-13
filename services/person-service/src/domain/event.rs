//! Events emitted by the Person aggregate.
//!
//! Events are the source of truth for aggregate state. They are
//! persisted append-only in the `person_events` table. The
//! current-state `persons` projection is rebuilt by replaying events.
//!
//! Event payloads are versioned (`PersonRegisteredV1` etc.) — a
//! schema migration produces a new variant, never a breaking change
//! to an existing one. Old events remain replayable forever; the
//! aggregate's `apply()` method handles every version.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::value_object::{PersonAttributes, PersonId};

/// The set of events the Person aggregate emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum PersonEvent {
    /// Person registered; aggregate transitions from absent to Registered.
    Registered(PersonRegisteredV1),
    /// Person attributes updated in place.
    Updated(PersonUpdatedV1),
    /// Person merged INTO a surviving canonical record. Emitted on the
    /// "from" aggregate (the duplicate being collapsed).
    Merged(PersonMergedV1),
}

impl PersonEvent {
    /// The event type discriminator stored alongside the payload in the
    /// event log. Used by the projection reader for routing.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Registered(_) => "person.registered.v1",
            Self::Updated(_) => "person.updated.v1",
            Self::Merged(_) => "person.merged.v1",
        }
    }

    /// The aggregate identifier the event applies to.
    pub fn person_id(&self) -> PersonId {
        match self {
            Self::Registered(p) => p.person_id,
            Self::Updated(p) => p.person_id,
            Self::Merged(p) => p.person_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonRegisteredV1 {
    pub person_id: PersonId,
    pub attributes: PersonAttributes,
    /// Authenticated principal that issued the registration. Stored on
    /// the event as provenance for the audit chain (D15 cryptographic-
    /// provenance limitation: v1 carries no per-event signature; this
    /// principal + the append-only event log together are the
    /// in-platform provenance surface. See service `CLAUDE.md`).
    pub actor_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub registered_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonUpdatedV1 {
    pub person_id: PersonId,
    /// Snapshot of attributes BEFORE the update was applied. Derived
    /// from the aggregate state at command-handle time so a replay
    /// has both before and after on hand.
    pub before: PersonAttributes,
    /// Snapshot of attributes AFTER the update.
    pub after: PersonAttributes,
    pub actor_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonMergedV1 {
    /// The aggregate being collapsed — the duplicate.
    pub person_id: PersonId,
    /// The surviving canonical record.
    pub into_person_id: PersonId,
    pub actor_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub merged_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
