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

    /// Load every current-state projection belonging to the given
    /// principal. Used by the data-subject-access endpoint (COMP-1):
    /// the authenticated declarant asks "show me everything RÉCOR
    /// holds about me" and receives the list of declarations where
    /// `declarant_principal == principal`.
    ///
    /// Ordering: most-recently submitted first — the order a declarant
    /// expects to see their own history in. The match is strictly by
    /// `declarant_principal`; future identity-linkage (mapping the
    /// authenticated principal to one or more `person_id` values so
    /// that beneficial-owner rows naming the principal are also
    /// returned) is a separate ticket and depends on a person registry
    /// that does not yet exist.
    ///
    /// Authorisation is the caller's job — by contract, the API layer
    /// MUST source the `principal` argument from the authenticated
    /// session (D17), never from request body or query string.
    async fn find_by_principal(
        &self,
        principal: &str,
    ) -> Result<Vec<crate::application::DeclarationProjection>, RepositoryError>;

    /// Atomic supersede: append the NEW declaration's Submitted event +
    /// upsert its projection + write its outbox row, AND append the OLD
    /// aggregate's Superseded event + update its projection + write its
    /// outbox row, in one Postgres transaction. Both `expected_version`
    /// values are asserted for optimistic concurrency; a mismatch on
    /// either aborts the entire transaction.
    async fn save_supersede(
        &self,
        new_event: &DeclarationEvent,
        new_expected_version: u64,
        old_id: DeclarationId,
        old_event: &DeclarationEvent,
        old_expected_version: u64,
    ) -> Result<(), RepositoryError>;
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
