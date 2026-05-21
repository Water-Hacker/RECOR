//! Ports for the Arrangement aggregate. Concrete implementations live
//! in `crate::infrastructure`. Tests use the in-memory double defined
//! alongside the use-case test modules.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::domain::{ArrangementEvent, ArrangementId, ArrangementKind, ArrangementUpdatableFields, GoverningLawJurisdiction};

/// Projection shape returned to API consumers + persisted in the
/// `arrangements` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ArrangementProjection {
    pub arrangement_id: ArrangementId,
    pub arrangement_kind: ArrangementKind,
    pub governing_law_jurisdiction: GoverningLawJurisdiction,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2024-06-01")]
    pub constitution_date: time::Date,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date, example = "2026-04-01")]
    pub dissolution_date: Option<time::Date>,
    /// FATF R.25 INR §3.f — five-year-after-cessation deadline. `None`
    /// until the arrangement is dissolved.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date, example = "2031-04-01")]
    pub retention_until: Option<time::Date>,
    pub fields: ArrangementUpdatableFields,
    pub version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// The transactional contract for persisting an Arrangement aggregate.
///
/// `save_event` is atomic: the event MUST be written to the event log
/// AND the current-state projection updated AND the outbox row inserted
/// in a single Postgres transaction.
#[async_trait]
pub trait ArrangementRepository: Send + Sync {
    async fn load_events(
        &self,
        id: ArrangementId,
    ) -> Result<Vec<ArrangementEvent>, ArrangementRepositoryError>;

    /// Persist a new event for an arrangement. The repository asserts
    /// that `expected_version` matches the current persisted aggregate
    /// version (optimistic concurrency); mismatch is `Conflict`.
    async fn save_event(
        &self,
        event: &ArrangementEvent,
        expected_version: u64,
    ) -> Result<(), ArrangementRepositoryError>;

    /// Load the current-state projection. Returns `None` if no events
    /// exist for the id.
    async fn load_projection(
        &self,
        id: ArrangementId,
    ) -> Result<Option<ArrangementProjection>, ArrangementRepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ArrangementRepositoryError {
    #[error("optimistic concurrency conflict: expected version {expected}, found {found}")]
    Conflict { expected: u64, found: u64 },

    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),

    #[error("event serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),

    #[error("invalid stored value: {0}")]
    InvalidStoredValue(String),
}
