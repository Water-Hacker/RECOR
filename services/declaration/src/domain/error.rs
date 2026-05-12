//! Domain errors. These represent invariant violations that the
//! aggregate refuses. API-layer translation maps them to HTTP status
//! codes; the aggregate itself doesn't know about HTTP.

use thiserror::Error;

use super::value_object::ValueObjectError;

#[derive(Debug, Error, PartialEq)]
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

    #[error("verification outcome received for declaration {0} which has no Submitted event")]
    VerificationOutcomeBeforeSubmit(uuid::Uuid),

    #[error("verification outcome received for declaration {declaration_id} but a different case ({existing_case_id}) is already recorded as the new case ({new_case_id})")]
    VerificationCaseMismatch {
        declaration_id: uuid::Uuid,
        existing_case_id: uuid::Uuid,
        new_case_id: uuid::Uuid,
    },

    #[error("fused {field} {value} out of range [0.0, 1.0]")]
    FusedBeliefOutOfRange { field: &'static str, value: f64 },

    #[error("cannot supersede declaration {0}: it has no Submitted event")]
    SupersedeBeforeSubmit(uuid::Uuid),

    #[error("declaration {0} is already superseded; supersede chains are strictly linear")]
    AlreadySuperseded(uuid::Uuid),

    #[error("declaration {0} cannot supersede itself")]
    SelfSupersedeForbidden(uuid::Uuid),

    #[error("declaration {declaration_id} cannot be superseded from state {state}; only Accepted or InVerification declarations can be superseded")]
    SupersedeFromInvalidState {
        declaration_id: uuid::Uuid,
        state: &'static str,
    },

    #[error("supersede authorisation failed: declarant {actual} does not own declaration {declaration_id} (owned by {expected})")]
    SupersedeNotOwner {
        declaration_id: uuid::Uuid,
        expected: String,
        actual: String,
    },

    #[error("supersede entity mismatch: new declaration is for entity {new_entity_id} but supersedes declaration for entity {old_entity_id}")]
    SupersedeEntityMismatch {
        old_entity_id: uuid::Uuid,
        new_entity_id: uuid::Uuid,
    },

    #[error("cannot amend declaration {0}: it has no Submitted event")]
    AmendBeforeSubmit(uuid::Uuid),

    #[error("declaration {declaration_id} cannot be amended from state {state}; only Submitted or InVerification declarations can be amended (Accepted → use Supersede; Rejected → re-submit)")]
    AmendFromInvalidState {
        declaration_id: uuid::Uuid,
        state: &'static str,
    },

    #[error("amend authorisation failed: declarant {actual} does not own declaration {declaration_id} (owned by {expected})")]
    AmendNotOwner {
        declaration_id: uuid::Uuid,
        expected: String,
        actual: String,
    },

    #[error("cannot correct declaration {0}: it has no Submitted event")]
    CorrectBeforeSubmit(uuid::Uuid),

    #[error("declaration {declaration_id} cannot be corrected from state {state}; corrections are allowed only from Submitted (use Amend or Supersede instead)")]
    CorrectFromInvalidState {
        declaration_id: uuid::Uuid,
        state: &'static str,
    },

    #[error("correct authorisation failed: declarant {actual} does not own declaration {declaration_id} (owned by {expected})")]
    CorrectNotOwner {
        declaration_id: uuid::Uuid,
        expected: String,
        actual: String,
    },

    #[error(transparent)]
    ValueObject(#[from] ValueObjectError),
}
