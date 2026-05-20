//! Domain value objects — newtype-wrapped primitives that carry domain
//! meaning the underlying primitive does not.
//!
//! `ToSchema` lives on every type that crosses the public wire so the
//! OpenAPI spec (DOC-1) can describe them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Stable identifier for a declaration. UUIDv7 — time-sortable, so
/// natural ordering matches submission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, format = "uuid", example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d")]
pub struct DeclarationId(pub Uuid);

impl DeclarationId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for DeclarationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DeclarationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Stable identifier for a legal entity. UUIDv7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, format = "uuid", example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d")]
pub struct EntityId(pub Uuid);

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Stable identifier for a natural person. UUIDv7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, format = "uuid", example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d")]
pub struct PersonId(pub Uuid);

impl std::fmt::Display for PersonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Ownership percentage expressed in basis points (1/100 of a percent).
/// Range: 0..=10_000. We store basis points rather than floats to
/// preserve exact arithmetic across additions and avoid rounding
/// surprises at the 99.99%/100.00% boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(
    description = "Ownership percentage in basis points (1/100 of a percent). \
        Range 0..=10_000 — i.e. 10_000 == 100.00%. Stored as an integer to \
        preserve exact arithmetic and avoid the 99.99/100.00 boundary surprise.",
    example = 5_000
)]
pub struct OwnershipBasisPoints(pub u32);

impl OwnershipBasisPoints {
    pub const MAX: Self = Self(10_000);
    pub const ZERO: Self = Self(0);

    /// Validate the basis points are in range [0, 10_000].
    pub fn try_from_basis_points(bp: u32) -> Result<Self, ValueObjectError> {
        if bp > Self::MAX.0 {
            return Err(ValueObjectError::OwnershipOutOfRange(bp));
        }
        Ok(Self(bp))
    }

    pub fn as_basis_points(self) -> u32 {
        self.0
    }

    /// As a fraction of 1.0. Use sparingly — basis points are the
    /// canonical form throughout the domain.
    pub fn as_fraction(self) -> f64 {
        f64::from(self.0) / 10_000.0
    }
}

/// Role under which a declarant is submitting on behalf of the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum DeclarantRole {
    /// The declarant is themselves the beneficial owner declaring directly.
    #[serde(rename = "self")]
    SelfDeclaration,
    /// The declarant is an authorised agent (e.g. corporate secretary, notary).
    #[serde(rename = "authorised_agent")]
    AuthorisedAgent,
    /// The declaration was filed with operator assistance (call-centre,
    /// front-desk-assisted intake). Lower-trust path that the verification
    /// engine may weight differently.
    #[serde(rename = "operator_assisted")]
    OperatorAssisted,
}

impl DeclarantRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfDeclaration => "self",
            Self::AuthorisedAgent => "authorised_agent",
            Self::OperatorAssisted => "operator_assisted",
        }
    }
}

/// The reason a declaration is being submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationKind {
    Incorporation,
    AnnualRenewal,
    ChangeOfControl,
    Correction,
    Amendment,
}

impl DeclarationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incorporation => "incorporation",
            Self::AnnualRenewal => "annual_renewal",
            Self::ChangeOfControl => "change_of_control",
            Self::Correction => "correction",
            Self::Amendment => "amendment",
        }
    }
}

/// Lane decision returned by the Verification Engine for a declaration.
/// Matches the verification engine's LaneDecision enum byte-for-byte over
/// the wire (snake_case serialisation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationLane {
    /// Auto-accept; verification confidence is high.
    Green,
    /// Hold for human review; verification is inconclusive.
    Yellow,
    /// Reject; verification found high risk or low authenticity.
    Red,
}

impl VerificationLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }

    /// The DeclarationState the aggregate transitions to upon receiving
    /// this lane decision. Green → Accepted, Yellow → InVerification
    /// (still pending analyst review), Red → Rejected.
    pub fn to_declaration_state(self) -> DeclarationState {
        match self {
            Self::Green => DeclarationState::Accepted,
            Self::Yellow => DeclarationState::InVerification,
            Self::Red => DeclarationState::Rejected,
        }
    }

    /// The verification_state projection column value.
    pub fn as_verification_state_str(self) -> &'static str {
        match self {
            Self::Green => "accepted",
            Self::Yellow => "in_verification",
            Self::Red => "rejected",
        }
    }
}

/// Lifecycle state of a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationState {
    /// Persisted but not yet sent to the verification engine.
    Draft,
    /// Sent to the verification engine; awaiting its decision.
    Submitted,
    /// Verification engine is processing; intermediate state.
    InVerification,
    /// Verification accepted; this declaration is the current truth.
    Accepted,
    /// Verification rejected; the declaration is in red-lane review.
    Rejected,
    /// Replaced by a subsequent declaration; retained for history.
    Superseded,
}

impl DeclarationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::InVerification => "in_verification",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }
}

/// One declared beneficial owner with their interest in the entity.
///
/// FATF cascade (TODO-001 closure): R.24 §c.24.6 fn 25 requires every
/// BO to be identified under the explicit cascade ownership → control →
/// senior managing official. The platform records the cascade tier
/// (`cascade_tier`) and, for the Control tier, the specific control
/// basis (`control_basis`). Tier (c) — Senior Managing Official — is
/// only admissible when tier (b) has been searched-for-and-ruled-out
/// (enforced at the aggregate; declarant submits the ruled-out
/// declaration in `cascade_tier_b_ruled_out_evidence`).
///
/// Nominee disclosure (TODO-010 closure): R.24 §c.24.12 requires
/// nominee arrangements to disclose the nominator. `is_nominee = true`
/// requires `nominator_person_id` to resolve to a separately-registered
/// person who themselves appears at the appropriate cascade tier.
///
/// Backwards-compatibility: every FATF field is `Option<T>` and uses
/// `#[serde(default)]` so historical Submitted/Amended/Corrected events
/// that pre-date this migration replay without loss. NEW declarations
/// MUST present `cascade_tier` (validated at the API DTO ⇒ command
/// boundary; missing → 400). Historical projections report the legacy
/// `LegacyPreCascade` tier value on read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BeneficialOwnerClaim {
    /// Canonical person identifier. The declarant supplies this; the
    /// Person service is the source of truth that the verification engine
    /// validates against in a later pipeline stage.
    pub person_id: PersonId,
    /// Percentage of equity (or equivalent control proxy) expressed as
    /// basis points.
    pub ownership_basis_points: OwnershipBasisPoints,
    /// Nature of the interest — equity, voting-rights, control-without-equity, etc.
    pub interest_kind: InterestKind,
    /// FATF cascade tier. New declarations MUST set this; historical
    /// declarations (pre-FATF-cascade migration) deserialise with `None`
    /// and are reported as `LegacyPreCascade` at the projection layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade_tier: Option<BoCascadeTier>,
    /// For tier (b) Control, the specific control basis. Required when
    /// `cascade_tier == Some(Control)`; refused for other tiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_basis: Option<BoControlBasis>,
    /// For tier (c) Senior Managing Official, free-text evidence that
    /// tier (b) was searched-for-and-ruled-out. Required when
    /// `cascade_tier == Some(SeniorManagingOfficial)`. The aggregate
    /// only validates presence + length; semantic verification is the
    /// back-office reviewer's responsibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade_tier_b_ruled_out_evidence: Option<String>,
    /// TODO-010: is this BO acting on behalf of a nominator?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_nominee: Option<bool>,
    /// TODO-010: when `is_nominee = Some(true)`, the person_id of the
    /// nominator. The nominator MUST appear as a separately-registered
    /// person, and the aggregate refuses self-nomination (person_id ==
    /// nominator_person_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nominator_person_id: Option<PersonId>,
}

/// FATF R.24 §c.24.6 cascade tiers. The cascade resolves the question
/// "who is the beneficial owner" in this order:
///
///   (a) Ownership — natural persons who hold ≥ 25% of equity or
///       equivalent direct/indirect interest.
///   (b) Control — natural persons exercising control through other
///       means: voting rights, board-appointment power, contractual
///       arrangements, family aggregation.
///   (c) Senior Managing Official — the residual fallback when (a)
///       and (b) yield no identified BO.
///
/// `LegacyPreCascade` is a read-time sentinel for projection rows that
/// were submitted before this migration shipped. It MUST never be the
/// target of a new declaration; the API DTO ⇒ command path refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoCascadeTier {
    /// Direct ownership ≥ 25%.
    OwnershipDirect,
    /// Indirect ownership ≥ 25% via intermediate legal persons.
    OwnershipIndirect,
    /// Control-without-ownership (voting / board / contractual).
    Control,
    /// Residual fallback per FATF cascade.
    SeniorManagingOfficial,
    /// Read-time sentinel for historical (pre-cascade-migration) rows.
    /// Never set on new declarations.
    LegacyPreCascade,
}

impl BoCascadeTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OwnershipDirect => "ownership_direct",
            Self::OwnershipIndirect => "ownership_indirect",
            Self::Control => "control",
            Self::SeniorManagingOfficial => "senior_managing_official",
            Self::LegacyPreCascade => "legacy_pre_cascade",
        }
    }
}

/// FATF R.24 §c.24.6(b) control bases. Required when
/// `BoCascadeTier == Control`; refused otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoControlBasis {
    /// Holds voting rights without equivalent equity.
    VotingRights,
    /// Power to appoint or remove a majority of the board.
    BoardAppointment,
    /// Contractual arrangement (shareholder agreement, management
    /// agreement, etc.) granting effective control.
    ContractualControl,
    /// Family-of-controllers aggregation (FATF Guidance §2.4).
    FamilyAggregation,
    /// Other documented control basis; supporting documents accompany
    /// the declaration.
    OtherDocumented,
}

impl BoControlBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VotingRights => "voting_rights",
            Self::BoardAppointment => "board_appointment",
            Self::ContractualControl => "contractual_control",
            Self::FamilyAggregation => "family_aggregation",
            Self::OtherDocumented => "other_documented",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterestKind {
    /// Direct equity ownership.
    Equity,
    /// Voting rights without equity.
    Voting,
    /// Family-proxy control.
    FamilyProxy,
    /// Contractual control without equity (e.g. management agreement).
    Contractual,
    /// Other; declarant explains in the supporting documents.
    Other,
}

impl InterestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equity => "equity",
            Self::Voting => "voting",
            Self::FamilyProxy => "family_proxy",
            Self::Contractual => "contractual",
            Self::Other => "other",
        }
    }
}

/// The set of fields amendable in-place on an existing declaration.
///
/// Used by `AmendDeclaration` commands and stored in
/// `DeclarationAmendedV1` events (both as the `before` snapshot — what
/// the aggregate held — and the `after` snapshot — what the declarant
/// is replacing them with). Fields NOT in this set cannot be amended:
///   - `entity_id` (a different entity is `Supersede`, not `Amend`)
///   - `declarant_principal` (auth-bound; an "amend by someone else"
///     would be a separate authorisation flow)
///   - `kind` (changing the declaration's purpose changes its meaning;
///     out of scope for v1)
///   - `attestation` itself (the attestation IS the amendment's signature)
///   - `correlation_id` / `submitted_at` (lifecycle metadata)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AmendmentSet {
    /// Replacement beneficial-owner roster. Must still satisfy the
    /// aggregate's owner-sum-invariant (basis points sum to 10_000)
    /// and the no-duplicate-person-id rule.
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    /// Replacement effective-from date. Validated by the same
    /// `validate_effective_from` rule the Submit path uses.
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2026-05-01")]
    pub effective_from: time::Date,
    /// Replacement declarant role. Most amendments leave this
    /// unchanged; the field is still recorded explicitly in both
    /// before/after snapshots so a replay sees the full intent.
    pub declarant_role: DeclarantRole,
    /// TODO-021: replacement adequacy_claims block. The declarant
    /// re-asserts adequate / accurate / up-to-date for the amended
    /// values. Optional on the type for back-compat with legacy
    /// Amended events; required at the API DTO boundary for new
    /// amendments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adequacy_claims: Option<crate::domain::attestation::AdequacyClaims>,
}

/// The set of pre-verification corrections the API supports.
///
/// In v1 the only field is `metadata_notes` — a free-form operator-
/// facing annotation that lets the declarant attach context the
/// canonical declaration body doesn't carry (typo explanations,
/// supporting-document references, etc.). The canonical declaration
/// payload is untouched by a correction, which is why corrections
/// are restricted to the `Submitted` state: the verification engine
/// has not yet processed the declaration, so changing metadata does
/// not perturb a downstream consumer's view of the aggregate.
///
/// Future correctable fields land here as additive `Option<...>` so
/// `CorrectionSet` remains backward-compatible across versions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CorrectionSet {
    /// Free-form metadata annotation. `None` represents "no annotation";
    /// `Some("")` is normalised to `None` at the API boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_notes: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValueObjectError {
    #[error("ownership basis points {0} exceeds maximum of 10_000 (100%)")]
    OwnershipOutOfRange(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_points_within_range_constructs() {
        assert_eq!(
            OwnershipBasisPoints::try_from_basis_points(0).unwrap(),
            OwnershipBasisPoints::ZERO
        );
        assert_eq!(
            OwnershipBasisPoints::try_from_basis_points(10_000).unwrap(),
            OwnershipBasisPoints::MAX
        );
        assert_eq!(
            OwnershipBasisPoints::try_from_basis_points(5_000)
                .unwrap()
                .as_fraction(),
            0.5
        );
    }

    #[test]
    fn basis_points_out_of_range_rejects() {
        assert!(OwnershipBasisPoints::try_from_basis_points(10_001).is_err());
        assert!(OwnershipBasisPoints::try_from_basis_points(u32::MAX).is_err());
    }

    #[test]
    fn declaration_id_new_is_time_sortable() {
        let a = DeclarationId::new();
        // Sleep below uuid v7 timestamp granularity is unnecessary; v7
        // includes a monotonic counter component. Generate a second id
        // and assert it sorts strictly greater.
        let b = DeclarationId::new();
        assert!(b.0 > a.0, "uuidv7 should produce sortable identifiers");
    }
}
