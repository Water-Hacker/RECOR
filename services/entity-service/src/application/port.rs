//! Ports — abstractions the application layer depends on. Concrete
//! implementations live in `crate::infrastructure`. Tests use in-memory
//! doubles defined alongside the use-case test modules.

use async_trait::async_trait;

use crate::domain::{EntityEvent, EntityId};

/// Search criteria for `find_by_criteria`. Each field is optional;
/// `None` means "do not filter on this dimension". `q` is a free-text
/// substring/ILIKE search against `canonical_name`.
#[derive(Debug, Clone, Default)]
pub struct SearchCriteria {
    pub q: Option<String>,
    pub jurisdiction: Option<String>,
    pub entity_type: Option<String>,
    /// Page size (LIMIT). Repository caps at 200 to prevent runaway
    /// scans.
    pub limit: u32,
}

/// The transactional contract for persisting an Entity aggregate.
///
/// `save_event` is atomic: the event MUST be written to the event log
/// AND the current-state projection updated AND the outbox row inserted
/// in a single Postgres transaction. Failure of any one rolls them all
/// back.
#[async_trait]
pub trait EntityRepository: Send + Sync {
    /// Load all events for an entity, in version order.
    async fn load_events(&self, id: EntityId) -> Result<Vec<EntityEvent>, RepositoryError>;

    /// Persist a new event for an entity. The repository asserts that
    /// `expected_version` matches the current persisted version
    /// (optimistic concurrency); mismatch is `RepositoryError::Conflict`.
    /// The same transaction updates the projection and writes an outbox
    /// row.
    async fn save_event(
        &self,
        event: &EntityEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError>;

    /// Load the current-state projection. Returns `None` if no events
    /// exist for the id.
    async fn load_projection(
        &self,
        id: EntityId,
    ) -> Result<Option<crate::application::EntityProjection>, RepositoryError>;

    /// Search entities by free-text + filters. Returns an ordered list
    /// (most recently founded first) bounded by `limit`. The repository
    /// is responsible for SQL-injection-safe query construction.
    async fn find_by_criteria(
        &self,
        criteria: &SearchCriteria,
    ) -> Result<Vec<crate::application::EntityProjection>, RepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("optimistic concurrency: expected version {expected}, found {found}")]
    Conflict { expected: u64, found: u64 },

    #[error("duplicate identity tuple (jurisdiction, registration_number_in_jurisdiction): {jurisdiction} / {registration_number}")]
    DuplicateIdentityTuple {
        jurisdiction: String,
        registration_number: String,
    },

    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),

    #[error("event serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),

    #[error("invalid stored value: {0}")]
    InvalidStoredValue(String),
}
