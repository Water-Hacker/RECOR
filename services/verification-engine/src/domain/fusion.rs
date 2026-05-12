//! Dempster-Shafer belief-function fusion.
//!
//! The architecture commits (Build Spec, Architecture V4 P14 § Stage 8)
//! to Dempster-Shafer over Bayesian probability. The choice is
//! deliberate: in adversarial verification the dominant epistemic state
//! is several stages producing weak positive evidence and no stage
//! producing direct negative evidence, with the question of how much
//! weight to give to the *absence* of negative evidence. Bayesian
//! probability forces a prior on that absence; Dempster-Shafer lets us
//! represent ignorance explicitly as mass on the universal set.
//!
//! This module implements the binary-frame version of the theory —
//! frame of discernment Θ = {True, False}, power set 2^Θ =
//! {∅, {True}, {False}, {True, False}}. A Basic Probability Assignment
//! (BPA) assigns non-negative mass to each non-empty subset of 2^Θ,
//! with the total summing to 1.
//!
//! Operations:
//!   * `belief(A)`        = Σ_{B ⊆ A} m(B)
//!   * `plausibility(A)`  = Σ_{B ∩ A ≠ ∅} m(B)
//!   * `m₁ ⊕ m₂` (Dempster's rule of combination), with the conflict
//!     normalisation factor K. Yager's fallback engages when K → 1.

use serde::{Deserialize, Serialize};

/// Numerical tolerance used in mass-sum sanity checks.
pub const MASS_EPSILON: f64 = 1e-9;

/// Binary frame of discernment Θ = {True, False}. For authenticity
/// reasoning, `True` means "the declared structure is true" and `False`
/// means "the declared structure is false". For risk reasoning, `True`
/// means "high risk" and `False` means "low risk".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryFrame {
    True,
    False,
}

/// Basic Probability Assignment over the binary frame.
/// All three mass components are non-negative and sum to 1 (within
/// `MASS_EPSILON`). `m_uncertain` is the mass on the universal set
/// {True, False} — the explicit-ignorance component that distinguishes
/// Dempster-Shafer from Bayesian.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BasicProbabilityAssignment {
    pub m_true: f64,
    pub m_false: f64,
    pub m_uncertain: f64,
}

/// The triple of belief, plausibility, and ignorance for a binary
/// hypothesis under a given BPA.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeliefMass {
    /// Lower bound on confidence in the hypothesis being True.
    pub belief: f64,
    /// Upper bound on confidence (1 - belief in not-True).
    pub plausibility: f64,
    /// Ignorance = plausibility - belief = mass on the universal set.
    pub ignorance: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum FusionError {
    #[error("BPA mass sum {sum} differs from 1.0 by more than {MASS_EPSILON} epsilon")]
    MassSumInvalid { sum: f64 },
    #[error("BPA mass component {label} is negative ({value})")]
    NegativeMass { label: &'static str, value: f64 },
    #[error("Dempster combination produced total conflict (K=1); cannot normalise")]
    TotalConflict,
}

impl BasicProbabilityAssignment {
    /// Construct a BPA with explicit validation.
    pub fn new(m_true: f64, m_false: f64, m_uncertain: f64) -> Result<Self, FusionError> {
        if m_true < 0.0 {
            return Err(FusionError::NegativeMass { label: "m_true", value: m_true });
        }
        if m_false < 0.0 {
            return Err(FusionError::NegativeMass { label: "m_false", value: m_false });
        }
        if m_uncertain < 0.0 {
            return Err(FusionError::NegativeMass { label: "m_uncertain", value: m_uncertain });
        }
        let sum = m_true + m_false + m_uncertain;
        if (sum - 1.0).abs() > MASS_EPSILON {
            return Err(FusionError::MassSumInvalid { sum });
        }
        Ok(Self { m_true, m_false, m_uncertain })
    }

    /// "I know nothing" — total ignorance. Identity element for
    /// Dempster combination.
    pub fn vacuous() -> Self {
        Self { m_true: 0.0, m_false: 0.0, m_uncertain: 1.0 }
    }

    /// Certain True.
    pub fn certain_true() -> Self {
        Self { m_true: 1.0, m_false: 0.0, m_uncertain: 0.0 }
    }

    /// Certain False.
    pub fn certain_false() -> Self {
        Self { m_true: 0.0, m_false: 1.0, m_uncertain: 0.0 }
    }

    /// Belief in the hypothesis being True. For the binary frame, this
    /// is just `m_true` (subsets of {True}: ∅ has m=0, {True} has m_true).
    pub fn belief_true(self) -> f64 {
        self.m_true
    }

    /// Plausibility of the hypothesis being True.
    /// pl({True}) = Σ_{B ∩ {True} ≠ ∅} m(B) = m_true + m_uncertain
    pub fn plausibility_true(self) -> f64 {
        self.m_true + self.m_uncertain
    }

    /// Belief in the hypothesis being False.
    pub fn belief_false(self) -> f64 {
        self.m_false
    }

    /// Plausibility of the hypothesis being False.
    pub fn plausibility_false(self) -> f64 {
        self.m_false + self.m_uncertain
    }

    /// Full belief-mass summary for the True hypothesis.
    pub fn belief_mass_true(self) -> BeliefMass {
        BeliefMass {
            belief: self.belief_true(),
            plausibility: self.plausibility_true(),
            ignorance: self.m_uncertain,
        }
    }

    /// Dempster's rule of combination, also written m₁ ⊕ m₂.
    /// Returns the combined BPA, or `TotalConflict` when K → 1.
    ///
    /// K (conflict mass) for the binary frame:
    ///   K = m₁({True}) · m₂({False}) + m₁({False}) · m₂({True})
    ///
    /// Combined masses (before normalisation):
    ///   m₁⊕₂({True})         = m₁({True})·m₂({True})
    ///                          + m₁({True})·m₂(Θ)
    ///                          + m₁(Θ)·m₂({True})
    ///   m₁⊕₂({False})        = m₁({False})·m₂({False})
    ///                          + m₁({False})·m₂(Θ)
    ///                          + m₁(Θ)·m₂({False})
    ///   m₁⊕₂(Θ)              = m₁(Θ)·m₂(Θ)
    ///
    /// Normalised by 1/(1-K).
    pub fn combine(self, other: Self) -> Result<Self, FusionError> {
        let conflict = self.m_true * other.m_false + self.m_false * other.m_true;
        if (1.0 - conflict).abs() < MASS_EPSILON {
            return Err(FusionError::TotalConflict);
        }
        let denom = 1.0 - conflict;
        let m_true_raw = self.m_true * other.m_true
            + self.m_true * other.m_uncertain
            + self.m_uncertain * other.m_true;
        let m_false_raw = self.m_false * other.m_false
            + self.m_false * other.m_uncertain
            + self.m_uncertain * other.m_false;
        let m_uncertain_raw = self.m_uncertain * other.m_uncertain;
        Self::new(m_true_raw / denom, m_false_raw / denom, m_uncertain_raw / denom)
    }

    /// Yager's modification: instead of normalising by (1-K), the
    /// conflict mass is allocated to the universal set (the ignorance
    /// term). Useful when sources are highly conflicting and the
    /// classical Dempster normalisation produces counter-intuitive
    /// "more conflict → more confidence" results.
    pub fn combine_yager(self, other: Self) -> Self {
        let conflict = self.m_true * other.m_false + self.m_false * other.m_true;
        let m_true = self.m_true * other.m_true
            + self.m_true * other.m_uncertain
            + self.m_uncertain * other.m_true;
        let m_false = self.m_false * other.m_false
            + self.m_false * other.m_uncertain
            + self.m_uncertain * other.m_false;
        let m_uncertain = self.m_uncertain * other.m_uncertain + conflict;
        // Construction here cannot fail: by definition the three sum to 1.
        Self { m_true, m_false, m_uncertain }
    }

    /// Fuse a sequence of BPAs via Dempster's rule. Returns the vacuous
    /// BPA on empty input (the identity).
    pub fn fuse_all<I>(bpas: I) -> Result<Self, FusionError>
    where
        I: IntoIterator<Item = Self>,
    {
        let mut acc = Self::vacuous();
        for bpa in bpas {
            acc = acc.combine(bpa)?;
        }
        Ok(acc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn vacuous_is_total_ignorance() {
        let v = BasicProbabilityAssignment::vacuous();
        assert_eq!(v.belief_true(), 0.0);
        assert_eq!(v.plausibility_true(), 1.0);
        assert_eq!(v.belief_false(), 0.0);
        assert_eq!(v.plausibility_false(), 1.0);
    }

    #[test]
    fn certain_true_is_total_certainty() {
        let c = BasicProbabilityAssignment::certain_true();
        assert_eq!(c.belief_true(), 1.0);
        assert_eq!(c.plausibility_true(), 1.0);
        assert_eq!(c.belief_false(), 0.0);
        assert_eq!(c.plausibility_false(), 0.0);
    }

    #[test]
    fn new_rejects_negative_mass() {
        assert!(BasicProbabilityAssignment::new(-0.1, 0.5, 0.6).is_err());
        assert!(BasicProbabilityAssignment::new(0.5, -0.1, 0.6).is_err());
        assert!(BasicProbabilityAssignment::new(0.5, 0.5, -0.1).is_err());
    }

    #[test]
    fn new_rejects_invalid_sum() {
        assert!(BasicProbabilityAssignment::new(0.5, 0.4, 0.0).is_err());
        assert!(BasicProbabilityAssignment::new(0.5, 0.6, 0.0).is_err());
    }

    #[test]
    fn new_accepts_valid_sum_within_epsilon() {
        // 0.3 + 0.3 + 0.4 = 1.0 (modulo float error)
        BasicProbabilityAssignment::new(0.3, 0.3, 0.4).expect("valid BPA");
    }

    #[test]
    fn vacuous_is_identity_under_combine() {
        let v = BasicProbabilityAssignment::vacuous();
        let bpa = BasicProbabilityAssignment::new(0.6, 0.2, 0.2).unwrap();
        let combined = v.combine(bpa).unwrap();
        assert!(approx_eq(combined.m_true, bpa.m_true));
        assert!(approx_eq(combined.m_false, bpa.m_false));
        assert!(approx_eq(combined.m_uncertain, bpa.m_uncertain));
    }

    #[test]
    fn combine_is_commutative() {
        let a = BasicProbabilityAssignment::new(0.6, 0.2, 0.2).unwrap();
        let b = BasicProbabilityAssignment::new(0.4, 0.4, 0.2).unwrap();
        let ab = a.combine(b).unwrap();
        let ba = b.combine(a).unwrap();
        assert!(approx_eq(ab.m_true, ba.m_true));
        assert!(approx_eq(ab.m_false, ba.m_false));
        assert!(approx_eq(ab.m_uncertain, ba.m_uncertain));
    }

    #[test]
    fn two_supporting_sources_strengthen_belief() {
        // Two independent sources each give 60% to True, 40% ignorance.
        // After combination, belief in True should be > 60%.
        let s1 = BasicProbabilityAssignment::new(0.6, 0.0, 0.4).unwrap();
        let s2 = BasicProbabilityAssignment::new(0.6, 0.0, 0.4).unwrap();
        let fused = s1.combine(s2).unwrap();
        assert!(fused.belief_true() > 0.6);
        assert!(fused.belief_true() < 1.0);
    }

    #[test]
    fn conflicting_sources_produce_finite_belief() {
        // s1 says 80% True; s2 says 80% False. K = 0.64, not 1.
        let s1 = BasicProbabilityAssignment::new(0.8, 0.0, 0.2).unwrap();
        let s2 = BasicProbabilityAssignment::new(0.0, 0.8, 0.2).unwrap();
        let fused = s1.combine(s2).unwrap();
        // Normalised; both belief and counter-belief get some mass.
        assert!(fused.belief_true() > 0.0);
        assert!(fused.belief_false() > 0.0);
        // Both finite and ≤ 1.
        assert!(fused.belief_true() <= 1.0);
        assert!(fused.belief_false() <= 1.0);
    }

    #[test]
    fn total_conflict_returns_error() {
        // Each side is certain of opposite. K = 1.
        let s1 = BasicProbabilityAssignment::certain_true();
        let s2 = BasicProbabilityAssignment::certain_false();
        assert!(matches!(s1.combine(s2), Err(FusionError::TotalConflict)));
    }

    #[test]
    fn yager_handles_total_conflict() {
        // Same total-conflict scenario; Yager allocates to ignorance.
        let s1 = BasicProbabilityAssignment::certain_true();
        let s2 = BasicProbabilityAssignment::certain_false();
        let yag = s1.combine_yager(s2);
        // All mass goes to ignorance (Θ).
        assert!(approx_eq(yag.m_true, 0.0));
        assert!(approx_eq(yag.m_false, 0.0));
        assert!(approx_eq(yag.m_uncertain, 1.0));
    }

    #[test]
    fn fuse_all_empty_is_vacuous() {
        let bpas: Vec<BasicProbabilityAssignment> = vec![];
        let f = BasicProbabilityAssignment::fuse_all(bpas).unwrap();
        assert!(approx_eq(f.m_uncertain, 1.0));
    }

    #[test]
    fn fuse_all_three_sources() {
        let s1 = BasicProbabilityAssignment::new(0.5, 0.0, 0.5).unwrap();
        let s2 = BasicProbabilityAssignment::new(0.5, 0.0, 0.5).unwrap();
        let s3 = BasicProbabilityAssignment::new(0.5, 0.0, 0.5).unwrap();
        let f = BasicProbabilityAssignment::fuse_all([s1, s2, s3]).unwrap();
        // Three independent 50%-True sources combine to belief > 50%.
        assert!(f.belief_true() > 0.5);
    }

    proptest! {
        #[test]
        fn fuzz_combine_preserves_mass_unity(
            t1 in 0f64..1.0,
            f1 in 0f64..1.0,
            t2 in 0f64..1.0,
            f2 in 0f64..1.0,
        ) {
            // Reject sums > 1.0; otherwise place residue in uncertain.
            prop_assume!(t1 + f1 <= 1.0);
            prop_assume!(t2 + f2 <= 1.0);
            let u1 = 1.0 - t1 - f1;
            let u2 = 1.0 - t2 - f2;
            // Skip near-total-conflict configurations where K ≈ 1.
            let k = t1 * f2 + f1 * t2;
            prop_assume!((1.0 - k).abs() > 1e-3);

            let bpa1 = BasicProbabilityAssignment::new(t1, f1, u1).unwrap();
            let bpa2 = BasicProbabilityAssignment::new(t2, f2, u2).unwrap();
            let fused = bpa1.combine(bpa2).unwrap();
            let sum = fused.m_true + fused.m_false + fused.m_uncertain;
            prop_assert!((sum - 1.0).abs() < 1e-6);
            prop_assert!(fused.m_true >= 0.0);
            prop_assert!(fused.m_false >= 0.0);
            prop_assert!(fused.m_uncertain >= 0.0);
        }
    }
}
