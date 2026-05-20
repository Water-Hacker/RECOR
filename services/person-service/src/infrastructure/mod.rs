//! Infrastructure adapters — concrete implementations of the
//! application-layer ports.

pub mod postgres;
pub mod retention;

pub use postgres::{IdempotencyStore, PostgresPersonRepository};
pub use retention::{warn_if_misconfigured, OutboxRetention, PruneOutcome};
