//! Application layer — orchestrates domain operations against
//! infrastructure ports.
//!
//! Use cases are stateless functions over the ports defined in `port`.
//! Ports are traits; infrastructure provides the concrete adapters.
//! This separation is the testability boundary: use cases can be
//! exercised against in-memory adapter doubles in unit tests.

pub mod port;
pub mod submit_declaration;
pub mod get_declaration;

pub use port::{DeclarationRepository, OutboxWriter, RepositoryError};
pub use submit_declaration::{SubmitDeclarationUseCase, SubmitReceipt, SubmitError};
pub use get_declaration::{DeclarationProjection, GetDeclarationUseCase, GetError};
