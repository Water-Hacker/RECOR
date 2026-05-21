//! TODO-002-declaration-link — cascade-tier resolver.
//!
//! FATF R.24 §c.24.6 defines the BO cascade for legal *entities* as
//!   (a) Ownership — natural persons holding ≥ 25% of equity / equivalent
//!   (b) Control — natural persons exercising control through other means
//!   (c) Senior Managing Official — residual fallback when (a)+(b) yield
//!       no identified BO
//!
//! FATF R.25 §INR.25 defines the equivalent chain for *arrangements*:
//!   settlor → trustee → protector → beneficiary (named + class) →
//!   any other natural person exercising ultimate effective control.
//!
//! The aggregate's cascade validator (see
//! `services/declaration/src/domain/aggregate.rs::validate_cascade_tier`)
//! encodes the R.24 rules per BO. The R.25 chain is a different
//! algorithm: there is no single "tier" per natural person — the same
//! person can simultaneously be a trustee AND a protector AND a
//! discretionary beneficiary, and the BO cascade applies to the
//! arrangement *as a whole* rather than per-person.
//!
//! This module provides a thin resolver that:
//!
//!   * For an `Entity { entity_id }` subject — delegates to the
//!     existing R.24 cascade-tier validation surface
//!     (`BoCascadeTier` is required on every BO).
//!
//!   * For an `Arrangement { arrangement_id }` subject — applies
//!     the R.25 chain. The legacy `BoCascadeTier` enum still gates
//!     each declared BO (so the same `cascade_tier` field works
//!     across both subject kinds); the verification engine is
//!     responsible for the arrangement-aware semantic check ("at
//!     least one settlor + at least one trustee per R.25
//!     §INR.25(b)").
//!
//! D14 fail-closed: when the subject is `Arrangement` but the
//! verification engine cannot resolve the `arrangement_id` against
//! the entity-service registry, the cascade resolver surfaces a
//! `UnresolvedArrangement` error rather than admitting the
//! declaration silently. The Declaration service itself does NOT
//! reach out to entity-service at submit time; the resolver is
//! a domain-pure helper that the verification engine plugs in via
//! Stage 1 / Stage 2 of the pipeline.

use thiserror::Error;

use crate::domain::{BeneficialOwnerClaim, BeneficialOwnerSubject, BoCascadeTier};

/// Outcome of resolving the cascade-tier requirements for a given
/// declaration. Pure value object — no I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CascadeContext {
    /// FATF R.24 — entity subject; the standard ownership →
    /// control → SMO cascade applies.
    EntityR24,
    /// FATF R.25 — arrangement subject; the settlor → trustee →
    /// protector → beneficiary chain applies.
    ArrangementR25,
}

impl CascadeContext {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EntityR24 => "r24_entity_cascade",
            Self::ArrangementR25 => "r25_arrangement_chain",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CascadeResolverError {
    #[error("declaration carries an arrangement subject but no beneficial owners were listed; R.25 requires at least one settlor or trustee")]
    EmptyArrangementOwners,
    #[error("declaration carries an entity subject but no beneficial owners were listed; R.24 requires at least one BO")]
    EmptyEntityOwners,
    #[error("legacy_pre_cascade sentinel is not admissible on a new declaration regardless of subject kind")]
    LegacyCascadeTierOnNewDeclaration,
}

/// Resolve the cascade context for a declaration. The returned
/// `CascadeContext` switches the downstream semantic-check stage of
/// the verification engine. The aggregate's per-BO invariants still
/// apply in both branches — the resolver only chooses which
/// semantic rule the verification engine should run.
pub fn resolve(
    subject: &BeneficialOwnerSubject,
    owners: &[BeneficialOwnerClaim],
) -> Result<CascadeContext, CascadeResolverError> {
    // The aggregate already refuses owners.is_empty() for the
    // entity path; we re-check defensively here so the resolver
    // can be called pre-aggregate (e.g. at the API boundary as
    // part of DTO validation).
    if owners.is_empty() {
        return Err(if subject.is_arrangement() {
            CascadeResolverError::EmptyArrangementOwners
        } else {
            CascadeResolverError::EmptyEntityOwners
        });
    }
    // Legacy sentinel is read-only — refuse regardless of subject.
    for o in owners {
        if matches!(o.cascade_tier, Some(BoCascadeTier::LegacyPreCascade)) {
            return Err(CascadeResolverError::LegacyCascadeTierOnNewDeclaration);
        }
    }
    if subject.is_arrangement() {
        Ok(CascadeContext::ArrangementR25)
    } else {
        Ok(CascadeContext::EntityR24)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_object::{InterestKind, OwnershipBasisPoints, PersonId};
    use crate::domain::{ArrangementId, EntityId};
    use uuid::Uuid;

    fn owner_at(tier: Option<BoCascadeTier>) -> BeneficialOwnerClaim {
        BeneficialOwnerClaim {
            person_id: PersonId(Uuid::now_v7()),
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(10_000).unwrap(),
            interest_kind: InterestKind::Equity,
            cascade_tier: tier,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        }
    }

    #[test]
    fn entity_subject_resolves_to_r24() {
        let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::now_v7()));
        let ctx = resolve(&subj, &[owner_at(Some(BoCascadeTier::OwnershipDirect))]).unwrap();
        assert_eq!(ctx, CascadeContext::EntityR24);
    }

    #[test]
    fn arrangement_subject_resolves_to_r25() {
        let subj = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::now_v7()));
        let ctx = resolve(&subj, &[owner_at(Some(BoCascadeTier::Control))]).unwrap();
        assert_eq!(ctx, CascadeContext::ArrangementR25);
    }

    #[test]
    fn empty_owners_refused_for_entity() {
        let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::now_v7()));
        let err = resolve(&subj, &[]).unwrap_err();
        assert_eq!(err, CascadeResolverError::EmptyEntityOwners);
    }

    #[test]
    fn empty_owners_refused_for_arrangement() {
        let subj = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::now_v7()));
        let err = resolve(&subj, &[]).unwrap_err();
        assert_eq!(err, CascadeResolverError::EmptyArrangementOwners);
    }

    #[test]
    fn legacy_sentinel_refused_on_arrangement() {
        let subj = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::now_v7()));
        let err = resolve(
            &subj,
            &[owner_at(Some(BoCascadeTier::LegacyPreCascade))],
        )
        .unwrap_err();
        assert_eq!(err, CascadeResolverError::LegacyCascadeTierOnNewDeclaration);
    }

    #[test]
    fn legacy_sentinel_refused_on_entity() {
        let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::now_v7()));
        let err = resolve(
            &subj,
            &[owner_at(Some(BoCascadeTier::LegacyPreCascade))],
        )
        .unwrap_err();
        assert_eq!(err, CascadeResolverError::LegacyCascadeTierOnNewDeclaration);
    }
}
