//! Ports — abstractions the application layer depends on. Concrete
//! implementations live in `crate::infrastructure`. Tests use in-memory
//! doubles defined alongside the use-case test modules.

use async_trait::async_trait;

use crate::domain::{DeclarationEvent, DeclarationId};

/// The transactional contract for persisting a Declaration aggregate.
///
/// `save_event` is atomic: the event MUST be written to the event log
/// AND the current-state projection updated AND the outbox row inserted
/// in a single Postgres transaction. Failure of any one rolls them all
/// back.
#[async_trait]
pub trait DeclarationRepository: Send + Sync {
    /// Load all events for a declaration, in version order.
    async fn load_events(&self, id: DeclarationId)
        -> Result<Vec<DeclarationEvent>, RepositoryError>;

    /// Persist a new event for a declaration. The repository asserts
    /// `expected_version` matches the current persisted version
    /// (optimistic concurrency); mismatch is `RepositoryError::Conflict`.
    /// The same transaction updates the projection and writes an outbox
    /// row.
    async fn save_event(
        &self,
        event: &DeclarationEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError>;

    /// Load the current-state projection. Returns `None` if no events
    /// exist for the id.
    async fn load_projection(
        &self,
        id: DeclarationId,
    ) -> Result<Option<crate::application::DeclarationProjection>, RepositoryError>;
}

/// Outbox writer — abstracted because some adapters (in-memory tests)
/// may not need a real outbox. Production wires this to the same
/// Postgres adapter as the repository.
#[async_trait]
pub trait OutboxWriter: Send + Sync {
    /// Mark an event as dispatched (called by the outbox-relay background
    /// task once it has actually published the event to Kafka).
    async fn mark_dispatched(&self, event_id: uuid::Uuid) -> Result<(), RepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("optimistic concurrency: expected version {expected}, found {found}")]
    Conflict { expected: u64, found: u64 },

    #[error("declaration {0} already has a recorded event with same idempotency key")]
    DuplicateIdempotencyKey(String),

    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),

    #[error("event serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),
}
