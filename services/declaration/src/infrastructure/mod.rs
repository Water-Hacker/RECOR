//! Infrastructure adapters — concrete implementations of the
//! application-layer ports.

pub mod postgres;
pub mod relay;

pub use postgres::PostgresDeclarationRepository;
pub use relay::{OutboxRelay, RelaySubscriber};
