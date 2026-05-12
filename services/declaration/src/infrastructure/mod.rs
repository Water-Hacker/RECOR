//! Infrastructure adapters — concrete implementations of the
//! application-layer ports.

pub mod outbox_admin;
pub mod person_registry;
pub mod postgres;
pub mod relay;
pub mod retention;

pub use outbox_admin::{DlqRow, OutboxAdminError, OutboxAdminStore};
pub use person_registry::PersonRegistryHttpAdapter;
pub use postgres::PostgresDeclarationRepository;
pub use relay::{OutboxRelay, RelaySubscriber};
pub use retention::{OutboxRetention, PruneOutcome};
