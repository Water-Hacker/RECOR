//! Application layer — use cases + ports + pipeline orchestrator +
//! concrete stage implementations.

pub mod orchestrator;
pub mod port;
pub mod stages;
pub mod submit_verification;
pub mod get_verification;

pub use orchestrator::PipelineOrchestrator;
pub use port::{
    AdapterError, BunecAdapter, BunecLookup, BunecLookupError, IcijAdapter, IcijCandidate,
    PepAdapter, PepMatch, PersonQuery, RepositoryError, SanctionMatch, SanctionsAdapter,
    VerificationRepository,
};
pub use submit_verification::{SubmitVerificationUseCase, SubmitError};
pub use get_verification::{GetVerificationUseCase, GetError};
