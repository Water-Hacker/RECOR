//! Infrastructure adapters — concrete implementations of the
//! application-layer ports.

pub mod postgres;

pub use postgres::{IdempotencyStore, PostgresEntityRepository};
