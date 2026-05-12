//! Domain value objects — newtype-wrapped primitives that carry domain
//! meaning the underlying primitive does not.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a declaration. UUIDv7 — time-sortable, so
/// natural ordering matches submission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(pub Uuid);

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Stable identifier for a natural person. UUIDv7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
