//! Infrastructure adapters — concrete implementations of the
//! application-layer ports.

pub mod outbox_admin;
pub mod postgres;
pub mod relay;
pub mod retention;

pub use outbox_admin::{DlqRow, OutboxAdminError, OutboxAdminStore};
pub use postgres::PostgresDeclarationRepository;
pub use relay::{OutboxRelay, RelaySubscriber};
pub use retention::{OutboxRetention, PruneOutcome};
