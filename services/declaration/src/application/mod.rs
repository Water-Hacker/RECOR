//! Application layer — orchestrates domain operations against
//! infrastructure ports.
//!
//! Use cases are stateless functions over the ports defined in `port`.
//! Ports are traits; infrastructure provides the concrete adapters.
//! This separation is the testability boundary: use cases can be
//! exercised against in-memory adapter doubles in unit tests.

pub mod amend_declaration;
pub mod correct_declaration;
pub mod list_by_principal;
pub mod port;
pub mod submit_declaration;
pub mod get_declaration;
pub mod record_verification_outcome;
pub mod supersede_declaration;

pub use amend_declaration::{AmendDeclarationUseCase, AmendError, AmendReceipt};
pub use correct_declaration::{CorrectDeclarationUseCase, CorrectError, CorrectReceipt};
pub use list_by_principal::{ListByPrincipalError, ListByPrincipalUseCase};
pub use port::{
    DeclarationRepository, OutboxWriter, PersonRegistryDisabled, PersonRegistryError,
    PersonRegistryPort, RepositoryError,
};
pub use submit_declaration::{SubmitDeclarationUseCase, SubmitReceipt, SubmitError};
pub use get_declaration::{DeclarationProjection, GetDeclarationUseCase, GetError};
pub use record_verification_outcome::{
    RecordVerificationError, RecordVerificationOutcomeUseCase, RecordVerificationReceipt,
};
pub use supersede_declaration::{
    SupersedeDeclarationUseCase, SupersedeError, SupersedeReceipt,
};
