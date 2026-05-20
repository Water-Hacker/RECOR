//! Domain layer for the Declaration service.
//!
//! Pure types and logic — no I/O, no async, no Tokio. Higher layers
//! depend on this module; this module depends only on `std`, `serde`,
//! `time`, and `uuid`.
//!
//! The `Declaration` aggregate is event-sourced: commands are validated
//! and produce events; events are applied to mutate aggregate state.
//! The aggregate is the source of truth for state transitions; the
//! Postgres projection is a derived read model.

pub mod aggregate;
pub mod attestation;
pub mod command;
pub mod error;
pub mod event;
pub mod serde_helpers;
pub mod value_object;

pub use aggregate::DeclarationAggregate;
pub use attestation::CryptographicAttestation;
pub use command::{
    AmendDeclaration, Command, CorrectDeclaration, RecordVerificationOutcome, SubmitDeclaration,
    SupersedeDeclaration,
};
pub use error::DomainError;
pub use event::{
    DeclarationAmendedV1, DeclarationCorrectedV1, DeclarationEvent, DeclarationSubmittedV1,
    DeclarationSupersededV1, DeclarationVerifiedV1,
};
pub use value_object::{
    AmendmentSet, BeneficialOwnerClaim, BoCascadeTier, BoControlBasis, CorrectionSet,
    DeclarantRole, DeclarationId, DeclarationKind, DeclarationState, EntityId,
    OwnershipBasisPoints, PersonId, VerificationLane,
};
