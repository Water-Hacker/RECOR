//! Application layer — orchestrates domain operations against
//! infrastructure ports.
//!
//! Use cases are stateless functions over the ports defined in `port`.
//! Ports are traits; infrastructure provides the concrete adapters.
//! Tests exercise use cases against in-memory doubles (see each use
//! case's `mod tests`).

pub mod get_person;
pub mod merge_persons;
pub mod port;
pub mod register_person;
pub mod search_persons;

pub use get_person::{GetError, GetPersonUseCase, PersonProjection};
pub use merge_persons::{MergeError, MergePersonsUseCase, MergeReceipt};
pub use port::{PersonRepository, RepositoryError};
pub use register_person::{
    RegisterError, RegisterPersonUseCase, RegisterReceipt,
};
pub use search_persons::{SearchError, SearchPersonsUseCase, SearchQuery};
