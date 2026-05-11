//! Domain errors. These represent invariant violations that the
//! aggregate refuses. API-layer translation maps them to HTTP status
//! codes; the aggregate itself doesn't know about HTTP.

use thiserror::Error;

use super::value_object::ValueObjectError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("declaration must include at least one beneficial owner")]
    NoBeneficialOwners,

    #[error("beneficial-owner ownership basis points sum to {sum} (expected exactly 10_000 for fully-owned entities; declared sum must equal 10_000)")]
    OwnershipSumInvariant { sum: u32 },

    #[error("duplicate beneficial owner person_id within a single declaration: {0}")]
    DuplicateBeneficialOwner(uuid::Uuid),

    #[error("effective_from date {effective_from} is more than 5 years before submission time {submitted_at}; out of range")]
    EffectiveFromTooOld {
        effective_from: time::Date,
        submitted_at: time::Date,
    },

    #[error("effective_from date {effective_from} is in the future (after submission time {submitted_at})")]
    EffectiveFromInFuture {
        effective_from: time::Date,
        submitted_at: time::Date,
    },

    #[error("declaration_id {0} already has a submitted event; duplicate submission rejected (use Amend or Correction)")]
    AlreadySubmitted(uuid::Uuid),

    #[error("attestation principal {actual} does not match command declarant_principal {expected}")]
    AttestationPrincipalMismatch { expected: String, actual: String },

    #[error("declarant_principal must not be empty")]
    EmptyDeclarantPrincipal,

    #[error(transparent)]
    ValueObject(#[from] ValueObjectError),
}
