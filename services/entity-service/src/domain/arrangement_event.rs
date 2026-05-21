//! Events emitted by the `Arrangement` aggregate.
//!
//! Events are the source of truth: `arrangement_events` is the append-only
//! log; the `arrangements` table is a derived projection.
//!
//! Event payloads are versioned — `ArrangementRegisteredV1` etc. — so a
//! schema migration produces a new variant rather than a breaking change
//! to an existing one. The aggregate's `apply()` handles every version.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::arrangement_value_object::{
    ArrangementId, ArrangementKind, ArrangementUpdatableFields, GoverningLawJurisdiction,
};

/// The closed set of events the Arrangement aggregate emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum ArrangementEvent {
    Registered(ArrangementRegisteredV1),
    Updated(ArrangementUpdatedV1),
    Dissolved(ArrangementDissolvedV1),
}

impl ArrangementEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Registered(_) => "arrangement.registered.v1",
            Self::Updated(_) => "arrangement.updated.v1",
            Self::Dissolved(_) => "arrangement.dissolved.v1",
        }
    }

    pub fn arrangement_id(&self) -> ArrangementId {
        match self {
            Self::Registered(p) => p.arrangement_id,
            Self::Updated(p) => p.arrangement_id,
            Self::Dissolved(p) => p.arrangement_id,
        }
    }

    /// Time the event was recorded. Used by the staleness watcher to
    /// surface arrangements whose last observed event predates the
    /// freshness threshold.
    pub fn recorded_at(&self) -> OffsetDateTime {
        match self {
            Self::Registered(p) => p.registered_at,
            Self::Updated(p) => p.updated_at,
            Self::Dissolved(p) => p.recorded_at,
        }
    }
}

/// Payload of `arrangement.registered.v1` — emitted on first creation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrangementRegisteredV1 {
    pub arrangement_id: ArrangementId,
    pub arrangement_kind: ArrangementKind,
    pub governing_law_jurisdiction: GoverningLawJurisdiction,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub constitution_date: time::Date,
    pub fields: ArrangementUpdatableFields,
    /// Authenticated principal that registered the arrangement.
    pub registered_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub registered_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

/// Payload of `arrangement.updated.v1`. Mirrors the entity event shape:
/// both `before` and `after` snapshots so the event log is fully
/// self-describing without consulting the projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrangementUpdatedV1 {
    pub arrangement_id: ArrangementId,
    pub before: ArrangementUpdatableFields,
    pub after: ArrangementUpdatableFields,
    pub updated_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}

/// Payload of `arrangement.dissolved.v1`. Terminal event; subsequent
/// updates / dissolves are refused at the aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrangementDissolvedV1 {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolution_date: time::Date,
    /// R.25 INR §3.f: records must be retained for at least five years
    /// after cessation. The aggregate computes this automatically from
    /// the dissolution date when the command lands; operators may
    /// dashboard against the column to surface arrangements eligible
    /// for post-cessation pruning.
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub retention_until: time::Date,
    /// Admin principal that recorded the dissolution. Admin-allowlist
    /// gated at the API layer (D17).
    pub dissolved_by_principal: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub recorded_at: OffsetDateTime,
    pub correlation_id: uuid::Uuid,
}
