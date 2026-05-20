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

    #[error("beneficial_owner.person_id {0} does not resolve in the Person registry (R-DECL-4)")]
    BeneficialOwnerNotInPersonRegistry(uuid::Uuid),

    // ─── FATF cascade invariants (TODO-001 closure) ────────────────────
    #[error("beneficial owner {person_id} declares cascade_tier=control but control_basis is missing (FATF c.24.6(b))")]
    ControlTierMissingBasis { person_id: uuid::Uuid },

    #[error("beneficial owner {person_id} declares cascade_tier={tier} but control_basis is set (only the Control tier carries a control_basis)")]
    ControlBasisOnNonControlTier {
        person_id: uuid::Uuid,
        tier: &'static str,
    },

    #[error("beneficial owner {person_id} declares cascade_tier=senior_managing_official but cascade_tier_b_ruled_out_evidence is missing or too short (FATF c.24.6 cascade; required to demonstrate tier-(b) search was exhausted)")]
    SmoTierMissingRuledOutEvidence { person_id: uuid::Uuid },

    #[error("beneficial owner {person_id} declares cascade_tier_b_ruled_out_evidence on a non-SMO tier ({tier}); only the senior_managing_official tier may carry this field")]
    RuledOutEvidenceOnNonSmoTier {
        person_id: uuid::Uuid,
        tier: &'static str,
    },

    #[error("declaration includes a senior_managing_official BO without a separately-declared control-tier BO that was ruled out (the cascade requires the tier-(b) search to be visible alongside the tier-(c) fallback)")]
    SmoTierWithoutVisibleControlSearch,

    #[error("beneficial owner {person_id} has cascade_tier set to the legacy_pre_cascade sentinel; new declarations must specify a real tier")]
    LegacyCascadeTierOnNewDeclaration { person_id: uuid::Uuid },

    #[error("beneficial owner {person_id} is missing the cascade_tier field; new declarations MUST specify the tier per FATF R.24 c.24.6")]
    CascadeTierMissing { person_id: uuid::Uuid },

    // ─── Nominee disclosure (TODO-010 closure) ─────────────────────────
    #[error("beneficial owner {person_id} is marked is_nominee=true but nominator_person_id is missing (FATF c.24.12)")]
    NomineeMissingNominator { person_id: uuid::Uuid },

    #[error("beneficial owner {person_id} declares nominator_person_id but is_nominee is not true; nominator without nominee status is ambiguous")]
    NominatorWithoutNomineeFlag { person_id: uuid::Uuid },

    #[error("beneficial owner {person_id} cannot nominate themselves (nominator_person_id must differ from person_id)")]
    SelfNominationForbidden { person_id: uuid::Uuid },

    #[error("nominator {nominator_id} for beneficial owner {nominee_id} does not appear as a separately-registered BO on this declaration")]
    NominatorNotRegisteredOnDeclaration {
        nominee_id: uuid::Uuid,
        nominator_id: uuid::Uuid,
    },

    // ─── Adequacy claims (TODO-021 closure) ────────────────────────────
    #[error("declaration is missing the adequacy_claims block (FATF R.24 c.24.8 requires explicit adequacy / accuracy / up-to-date assertions on new declarations)")]
    AdequacyClaimsMissing,

    #[error("adequacy_claims.up_to_date_as_of {as_of} is in the future relative to submission time {submitted_at}; up-to-date assertions cannot post-date the submission")]
    AdequacyAsOfInFuture {
        as_of: time::OffsetDateTime,
        submitted_at: time::OffsetDateTime,
    },

    #[error("adequacy_claims.up_to_date_as_of {as_of} is more than 30 days before submission time {submitted_at}; FATF c.24.8 fn 29 benchmark requires updates within one month of change")]
    AdequacyAsOfStale {
        as_of: time::OffsetDateTime,
        submitted_at: time::OffsetDateTime,
    },

    #[error("adequacy_claims.legal_basis is empty; FATF c.24.6(a) declarants must cite the obligation under which they file")]
    AdequacyLegalBasisEmpty,

    #[error("adequacy_claims.legal_basis exceeds 1024 characters; pin a concise citation, attach the full legal text out-of-band if needed")]
    AdequacyLegalBasisTooLong,

    // ─── 30-day update obligation (TODO-005, FATF R.24 c.24.8 fn 29) ──
    #[error("last_event_observed_at {as_of} is in the future relative to submission time {submitted_at}; the declarant asserts an event that has not yet occurred")]
    LastEventObservedAtInFuture {
        as_of: time::OffsetDateTime,
        submitted_at: time::OffsetDateTime,
    },

    #[error("last_event_observed_at {as_of} is more than 5 years before submission time {submitted_at}; out of range (the declarant should re-stamp the date the change actually occurred)")]
    LastEventObservedAtTooOld {
        as_of: time::OffsetDateTime,
        submitted_at: time::OffsetDateTime,
    },
}
