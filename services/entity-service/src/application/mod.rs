//! Application layer — orchestrates domain operations against
//! infrastructure ports.
//!
//! Use cases are stateless functions over the ports defined in `port`.
//! Ports are traits; infrastructure provides the concrete adapters.
//! This separation is the testability boundary: use cases can be
//! exercised against in-memory adapter doubles in unit tests.

pub mod dissolve_entity;
pub mod get_entity;
pub mod port;
pub mod register_entity;
pub mod search_entities;
pub mod update_entity;

pub use dissolve_entity::{DissolveEntityUseCase, DissolveError, DissolveReceipt};
pub use get_entity::{EntityProjection, GetEntityUseCase, GetError};
pub use port::{EntityRepository, RepositoryError, SearchCriteria};
pub use register_entity::{RegisterEntityUseCase, RegisterError, RegisterReceipt};
pub use search_entities::{SearchEntitiesUseCase, SearchError};
pub use update_entity::{UpdateEntityUseCase, UpdateError, UpdateReceipt};
