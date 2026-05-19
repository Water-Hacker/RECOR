//! Ports — abstractions the application layer depends on. Concrete
//! implementations live in `crate::infrastructure`. Tests use in-memory
//! doubles defined alongside the use-case test modules.

use async_trait::async_trait;

use crate::domain::{PersonEvent, PersonId};

/// The transactional contract for persisting a Person aggregate.
///
/// `save_event` is atomic: the event MUST be written to the event log
/// AND the current-state projection updated AND the outbox row inserted
/// in a single Postgres transaction. Failure of any one rolls them all
/// back.
#[async_trait]
pub trait PersonRepository: Send + Sync {
    /// Load all events for a person, in version order.
    async fn load_events(&self, id: PersonId) -> Result<Vec<PersonEvent>, RepositoryError>;

    /// Persist a new event for a person. The repository asserts
    /// `expected_version` matches the current persisted version
    /// (optimistic concurrency); mismatch is `RepositoryError::Conflict`.
    /// The same transaction updates the projection and writes an outbox
    /// row.
    async fn save_event(
        &self,
        event: &PersonEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError>;

    /// Atomic merge: append the source aggregate's `Merged` event +
    /// upsert its projection (setting `merged_into`) + write its
    /// outbox row, all in one Postgres transaction. The target
    /// aggregate is NOT mutated — the merge pointer is one-way.
    async fn save_merge(
        &self,
        event: &PersonEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError>;

    /// Load the current-state projection. Returns `None` if no events
    /// exist for the id.
    async fn load_projection(
        &self,
        id: PersonId,
    ) -> Result<Option<crate::application::PersonProjection>, RepositoryError>;

    /// Fuzzy-ish search: ILIKE on the canonical_full_name, optionally
    /// filtered by nationality. v1 is intentionally crude (no
    /// pg_trgm) — see `search_persons::SearchPersonsUseCase` docs and
    /// the TODO marker in the Postgres adapter for the fuzzy upgrade.
    /// Caller supplies the page size via `limit`.
    ///
    /// FIND-005 RBAC scope: when `created_by_filter` is `Some(principal)`
    /// the repository restricts the result set to rows that principal
    /// registered (an extra `AND created_by_principal = $N` predicate).
    /// `None` is the admin path — every row is in scope. The HTTP
    /// handler decides which path applies based on the admin allowlist;
    /// the repository does not consult the allowlist itself.
    async fn search(
        &self,
        query: &str,
        nationality_filter: Option<&str>,
        created_by_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<crate::application::PersonProjection>, RepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("optimistic concurrency: expected version {expected}, found {found}")]
    Conflict { expected: u64, found: u64 },

    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),

    #[error("event serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),
}
