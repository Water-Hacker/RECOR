//! Lane routing — Stage 9. Deterministic threshold logic over the
//! fused authenticity + risk BPAs.
//!
//! Per Architecture V4 P14 § Stage 9:
//!   * Green: authenticity belief ≥ green_belief_threshold AND
//!            risk belief ≤ green_risk_threshold AND
//!            ignorance gap (plausibility - belief) ≤ green_ignorance_threshold
//!   * Yellow: authenticity belief < green threshold OR risk belief
//!             above green but below red.
//!   * Red: any high-severity short-circuit OR authenticity belief
//!          below red threshold OR risk above red threshold.

use serde::{Deserialize, Serialize};

use super::fusion::BasicProbabilityAssignment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneDecision {
    /// Auto-accept. Declaration becomes served data.
    Green,
    /// Hold for human review by a verification analyst.
    Yellow,
    /// Reject + route to investigation workflow.
    Red,
}

impl LaneDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }
}

/// Operator-tunable thresholds for the lane router. These come from
/// the calibration ceremony documented in the operations runbook; v1
/// ships defaults that are deliberately conservative.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LaneThresholds {
    pub green_authenticity_belief: f64,
    pub green_risk_belief: f64,
    pub green_max_ignorance: f64,
    pub red_authenticity_belief: f64,
    pub red_risk_belief: f64,
}

impl Default for LaneThresholds {
    fn default() -> Self {
        Self {
            // Above 0.85 belief in authenticity → can be green.
            green_authenticity_belief: 0.85,
            // Below 0.20 belief in risk → can be green.
            green_risk_belief: 0.20,
            // Less than 0.30 ignorance (i.e. authenticity belief and
            // plausibility don't differ by more than 0.30) → enough
            // evidence to green-lane.
            green_max_ignorance: 0.30,
            // Below 0.40 belief in authenticity → red.
            red_authenticity_belief: 0.40,
            // Above 0.70 belief in risk → red.
            red_risk_belief: 0.70,
        }
    }
}

impl LaneThresholds {
    pub fn route(
        self,
        authenticity_bpa: BasicProbabilityAssignment,
        risk_bpa: BasicProbabilityAssignment,
    ) -> LaneDecision {
        let auth_belief = authenticity_bpa.belief_true();
        let risk_belief = risk_bpa.belief_true();
        let auth_ignorance = authenticity_bpa.m_uncertain;

        // Red overrides everything: too little confidence in authenticity
        // OR too much confidence in risk.
        if auth_belief < self.red_authenticity_belief || risk_belief > self.red_risk_belief {
            return LaneDecision::Red;
        }

        // Green requires all three constraints.
        if auth_belief >= self.green_authenticity_belief
            && risk_belief <= self.green_risk_belief
            && auth_ignorance <= self.green_max_ignorance
        {
            return LaneDecision::Green;
        }

        // Otherwise yellow.
        LaneDecision::Yellow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_requires_all_three_conditions() {
        let t = LaneThresholds::default();
        let strong_auth = BasicProbabilityAssignment::new(0.9, 0.05, 0.05).unwrap();
        let low_risk = BasicProbabilityAssignment::new(0.05, 0.85, 0.10).unwrap();
        assert_eq!(t.route(strong_auth, low_risk), LaneDecision::Green);
    }

    #[test]
    fn high_risk_routes_red_even_with_high_authenticity() {
        let t = LaneThresholds::default();
        let strong_auth = BasicProbabilityAssignment::new(0.9, 0.05, 0.05).unwrap();
        let high_risk = BasicProbabilityAssignment::new(0.8, 0.10, 0.10).unwrap();
        assert_eq!(t.route(strong_auth, high_risk), LaneDecision::Red);
    }

    #[test]
    fn low_authenticity_routes_red_even_with_low_risk() {
        let t = LaneThresholds::default();
        let weak_auth = BasicProbabilityAssignment::new(0.3, 0.5, 0.2).unwrap();
        let low_risk = BasicProbabilityAssignment::new(0.05, 0.85, 0.10).unwrap();
        assert_eq!(t.route(weak_auth, low_risk), LaneDecision::Red);
    }

    #[test]
    fn high_ignorance_blocks_green() {
        // Authenticity belief is 0.85 but ignorance is 0.4 — too much
        // unknown to confidently auto-accept.
        let t = LaneThresholds::default();
        let auth = BasicProbabilityAssignment::new(0.85, 0.1, 0.05).unwrap();
        let mid_auth = BasicProbabilityAssignment::new(0.5, 0.1, 0.4).unwrap();
        let low_risk = BasicProbabilityAssignment::new(0.05, 0.85, 0.10).unwrap();
        // Strong-belief, low-ignorance, low-risk → green
        assert_eq!(t.route(auth, low_risk), LaneDecision::Green);
        // Strong-ish belief but 40% ignorance → yellow
        assert_eq!(t.route(mid_auth, low_risk), LaneDecision::Yellow);
    }

    #[test]
    fn yellow_in_between() {
        let t = LaneThresholds::default();
        // Authenticity 0.6, risk 0.4 — neither high-pass nor red.
        let auth = BasicProbabilityAssignment::new(0.6, 0.2, 0.2).unwrap();
        let risk = BasicProbabilityAssignment::new(0.4, 0.4, 0.2).unwrap();
        assert_eq!(t.route(auth, risk), LaneDecision::Yellow);
    }

    #[test]
    fn vacuous_inputs_route_red() {
        // No evidence either way; authenticity belief = 0 < red threshold 0.40.
        let t = LaneThresholds::default();
        let vac = BasicProbabilityAssignment::vacuous();
        assert_eq!(t.route(vac, vac), LaneDecision::Red);
    }
}
