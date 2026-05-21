//! TODO-049 — Per-decision explainability event (procedural-fairness gap).
//!
//! Every adjudicated verification case carries an immutable
//! `DecisionRationale` record alongside the case payload. The rationale
//! captures the structure of the decision so analysts, declarants, and
//! oversight bodies can understand WHY a lane was assigned — not just
//! the outcome.
//!
//! Composition is performed by the pipeline orchestrator AFTER fusion
//! and lane routing, BEFORE persistence. The repository persists the
//! rationale in the SAME transaction as the case (COMP-2 immutability
//! mirrors `verification_cases`).
//!
//! Wire contract:
//!   * `case_id` + `declaration_id` — joinable to the case row.
//!   * `stage_rationales` — one entry per stage, in pipeline order.
//!     The `one_line_reason` is extracted from the stage's evidence
//!     JSON (preferring `evidence.rule`, falling back to
//!     `evidence.rationale`, finally to the stage_id string).
//!   * `fusion_steps` — the running BPA after each evidence stage was
//!     combined into the accumulator. Enough to re-derive the fused
//!     belief for an external auditor.
//!   * `lane_thresholds_applied` — the exact `LaneThresholds` snapshot
//!     used at adjudication time. Calibration changes to thresholds
//!     must be auditable post-hoc; this snapshot is the anchor.
//!   * `lane` + final beliefs — duplicated from the case for
//!     standalone-document semantics.
//!
//! Architecture references:
//!   * Architecture V4 P14 § Stage 8 (fusion) + § Stage 9 (lane).
//!   * ADR-0002 (Dempster-Shafer fusion).
//!   * ADR-0014 (Stage 7 cross-source rules; rationale carries Stage 7
//!     evidence verbatim).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::case::VerificationCaseId;
use super::fusion::BasicProbabilityAssignment;
use super::lane::{LaneDecision, LaneThresholds};
use super::stage::{StageId, StageOutcome, StageOutcomeKind};

/// One stage's contribution to the rationale: the high-level outcome
/// kind, a single-sentence reason extracted from the evidence JSON,
/// and the authenticity + risk BPA the stage produced.
///
/// The reason is extracted by `extract_one_line_reason` at composition
/// time so the persisted rationale is self-contained; analysts do not
/// need to re-parse the stage's evidence JSON to read the summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageRationale {
    pub stage_id: StageId,
    pub outcome_kind: StageOutcomeKind,
    /// Single-line human-readable reason. Extracted from
    /// `evidence.rule`, falling back to `evidence.rationale`, finally
    /// to the stage_id string.
    pub one_line_reason: String,
    /// BPA the stage contributed to the authenticity fusion.
    pub authenticity_bpa: BasicProbabilityAssignment,
    /// BPA the stage contributed to the risk fusion.
    pub risk_bpa: BasicProbabilityAssignment,
}

/// One step in the fusion chain: after combining `combined_stage`'s BPA
/// into the running accumulator, this is the resulting BPA. Useful for
/// reconstructing the per-step Dempster derivation in the analyst UI.
///
/// Stages that do not contribute to fusion (BeliefFusion, LaneRouting)
/// are absent from the chain; the chain length is therefore
/// `count(stages with kind != BeliefFusion && kind != LaneRouting)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FusionStep {
    pub combined_stage: StageId,
    pub running_authenticity: BasicProbabilityAssignment,
    pub running_risk: BasicProbabilityAssignment,
}

/// Snapshot of the `LaneThresholds` used by Stage 9 at the moment this
/// case was adjudicated. Persisted alongside the rationale so a future
/// re-calibration does not retroactively change the audit interpretation
/// of past cases.
///
/// Field naming mirrors the `LaneThresholds` shape but expresses the
/// same intent as documented in the explainability event spec:
///   * `red_*` — thresholds at or beyond which a Red is forced.
///   * `yellow_*` — thresholds at or beyond which the Green lane is
///     refused (yielding Yellow when not already Red).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LaneThresholdsSnapshot {
    /// Above this risk belief → Red.
    pub red_risk_min: f64,
    /// Above this risk belief → no Green (i.e. Yellow when not Red).
    pub yellow_risk_min: f64,
    /// Below this authenticity belief → Red.
    pub red_authenticity_max: f64,
    /// Below this authenticity belief → no Green (Yellow when not Red).
    pub yellow_authenticity_max: f64,
}

impl LaneThresholdsSnapshot {
    /// Capture from a `LaneThresholds` configuration.
    ///
    /// Mapping:
    ///   * `red_risk_min`            <= `LaneThresholds.red_risk_belief`
    ///   * `yellow_risk_min`         <= `LaneThresholds.green_risk_belief`
    ///   * `red_authenticity_max`    <= `LaneThresholds.red_authenticity_belief`
    ///   * `yellow_authenticity_max` <= `LaneThresholds.green_authenticity_belief`
    ///
    /// The naming inversion (`yellow_*_min` derived from the
    /// `green_*` threshold) reflects the rationale's external-facing
    /// vocabulary while keeping the internal threshold semantics
    /// untouched.
    pub fn from_thresholds(t: LaneThresholds) -> Self {
        Self {
            red_risk_min: t.red_risk_belief,
            yellow_risk_min: t.green_risk_belief,
            red_authenticity_max: t.red_authenticity_belief,
            yellow_authenticity_max: t.green_authenticity_belief,
        }
    }
}

/// The full rationale for a single verification case. One row per case
/// in the `decision_rationales` table; immutable post-write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRationale {
    pub case_id: VerificationCaseId,
    pub declaration_id: Uuid,
    pub stage_rationales: Vec<StageRationale>,
    pub fusion_steps: Vec<FusionStep>,
    pub lane_thresholds_applied: LaneThresholdsSnapshot,
    pub lane: LaneDecision,
    pub final_authenticity_belief: f64,
    pub final_authenticity_plausibility: f64,
    pub final_risk_belief: f64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub composed_at: time::OffsetDateTime,
}

impl DecisionRationale {
    /// Compose a `DecisionRationale` from the artefacts the orchestrator
    /// already has at the moment the case is finalised. Pure function;
    /// no I/O.
    ///
    /// `stage_outcomes` is consumed in order; stages where
    /// `stage_id` is `BeliefFusion` or `LaneRouting` do NOT appear in
    /// `fusion_steps` (they are not evidence sources) but ARE included
    /// in `stage_rationales` so the rationale's stage-by-stage view is
    /// complete.
    ///
    /// The fusion chain replays the orchestrator's Dempster +
    /// Yager-fallback combination exactly so the rationale's running
    /// beliefs match the fused values it persists.
    pub fn compose(
        case_id: VerificationCaseId,
        declaration_id: Uuid,
        stage_outcomes: &[StageOutcome],
        fused_authenticity: BasicProbabilityAssignment,
        fused_risk: BasicProbabilityAssignment,
        lane: LaneDecision,
        thresholds: LaneThresholds,
    ) -> Self {
        let stage_rationales: Vec<StageRationale> = stage_outcomes
            .iter()
            .map(|o| StageRationale {
                stage_id: o.stage_id,
                outcome_kind: o.kind,
                one_line_reason: extract_one_line_reason(o),
                authenticity_bpa: o.authenticity_bpa,
                risk_bpa: o.risk_bpa,
            })
            .collect();

        let fusion_steps = compose_fusion_steps(stage_outcomes);

        Self {
            case_id,
            declaration_id,
            stage_rationales,
            fusion_steps,
            lane_thresholds_applied: LaneThresholdsSnapshot::from_thresholds(thresholds),
            lane,
            final_authenticity_belief: fused_authenticity.belief_true(),
            final_authenticity_plausibility: fused_authenticity.plausibility_true(),
            final_risk_belief: fused_risk.belief_true(),
            composed_at: time::OffsetDateTime::now_utc(),
        }
    }
}

/// Extract a single-line reason from a stage outcome's evidence JSON.
///
/// Precedence:
///   1. `evidence.rule`        — when the stage labels its decision with
///      a stable rule id (e.g. Stage 6 patterns, Stage 7 cross-source).
///   2. `evidence.rationale`   — free-form sentence the stage authored
///      for human consumers.
///   3. `evidence.reason`      — alternative key some stages use.
///   4. `stage_id` as a string — last-resort label when neither is
///      present (e.g. fusion + lane-routing rows).
fn extract_one_line_reason(o: &StageOutcome) -> String {
    if let Some(s) = o.evidence.get("rule").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = o.evidence.get("rationale").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = o.evidence.get("reason").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    o.stage_id.as_str().to_string()
}

/// Replay the orchestrator's fusion chain over the stage outcomes,
/// emitting a `FusionStep` per evidence stage. The replay uses the same
/// Dempster-with-Yager-fallback combinator the orchestrator uses, so
/// the running beliefs here match the fused belief stored on the case.
fn compose_fusion_steps(stage_outcomes: &[StageOutcome]) -> Vec<FusionStep> {
    let evidence: Vec<&StageOutcome> = stage_outcomes
        .iter()
        .filter(|o| {
            !matches!(o.stage_id, StageId::BeliefFusion | StageId::LaneRouting)
        })
        .collect();

    let mut steps = Vec::with_capacity(evidence.len());
    let mut acc_auth = BasicProbabilityAssignment::vacuous();
    let mut acc_risk = BasicProbabilityAssignment::vacuous();
    for o in evidence {
        acc_auth = match acc_auth.combine(o.authenticity_bpa) {
            Ok(c) => c,
            Err(_) => acc_auth.combine_yager(o.authenticity_bpa),
        };
        acc_risk = match acc_risk.combine(o.risk_bpa) {
            Ok(c) => c,
            Err(_) => acc_risk.combine_yager(o.risk_bpa),
        };
        steps.push(FusionStep {
            combined_stage: o.stage_id,
            running_authenticity: acc_auth,
            running_risk: acc_risk,
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    //! Unit coverage for TODO-049 — five required cases plus
    //! supporting predicate tests. Persistence round-trip lives in
    //! `tests/api_integration.rs` next to the postgres-backed
    //! repository tests so the unit suite stays I/O-free.

    use super::*;
    use serde_json::json;

    fn bpa(t: f64, f: f64, u: f64) -> BasicProbabilityAssignment {
        BasicProbabilityAssignment::new(t, f, u).expect("valid BPA")
    }

    fn outcome(
        stage_id: StageId,
        kind: StageOutcomeKind,
        evidence: serde_json::Value,
        auth: BasicProbabilityAssignment,
        risk: BasicProbabilityAssignment,
    ) -> StageOutcome {
        StageOutcome {
            stage_id,
            kind,
            authenticity_bpa: auth,
            risk_bpa: risk,
            evidence,
            duration_ms: 0,
        }
    }

    #[test]
    fn composes_from_three_stage_outcome_set() {
        // Three stages, all PASS, modest authenticity, low risk.
        let outcomes = vec![
            outcome(
                StageId::SchemaValidation,
                StageOutcomeKind::Pass,
                json!({"rule": "schema.valid"}),
                bpa(0.9, 0.0, 0.1),
                BasicProbabilityAssignment::vacuous(),
            ),
            outcome(
                StageId::IdentityAuthentication,
                StageOutcomeKind::Pass,
                json!({"rationale": "BUNEC match — canonical name agrees"}),
                bpa(0.7, 0.0, 0.3),
                BasicProbabilityAssignment::vacuous(),
            ),
            outcome(
                StageId::SanctionsScreening,
                StageOutcomeKind::Pass,
                json!({}),
                bpa(0.5, 0.0, 0.5),
                bpa(0.0, 0.7, 0.3),
            ),
        ];
        let case_id = VerificationCaseId::new();
        let declaration_id = Uuid::now_v7();
        let fused_auth = bpa(0.85, 0.0, 0.15);
        let fused_risk = bpa(0.0, 0.7, 0.3);

        let r = DecisionRationale::compose(
            case_id,
            declaration_id,
            &outcomes,
            fused_auth,
            fused_risk,
            LaneDecision::Yellow,
            LaneThresholds::default(),
        );

        assert_eq!(r.case_id, case_id);
        assert_eq!(r.declaration_id, declaration_id);
        assert_eq!(r.stage_rationales.len(), 3);
        assert_eq!(r.lane, LaneDecision::Yellow);
        assert_eq!(r.final_authenticity_belief, fused_auth.belief_true());
        assert_eq!(r.final_risk_belief, fused_risk.belief_true());
    }

    #[test]
    fn one_line_reason_prefers_rule_then_rationale_then_reason_then_stage_id() {
        // 1. evidence.rule wins.
        let r1 = extract_one_line_reason(&outcome(
            StageId::PatternDetection,
            StageOutcomeKind::Fail,
            json!({"rule": "STRUCTURE_LAYERED", "rationale": "ignored"}),
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
        ));
        assert_eq!(r1, "STRUCTURE_LAYERED");

        // 2. rationale fallback.
        let r2 = extract_one_line_reason(&outcome(
            StageId::AdverseMedia,
            StageOutcomeKind::InsufficientEvidence,
            json!({"rationale": "no ICIJ candidates returned"}),
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
        ));
        assert_eq!(r2, "no ICIJ candidates returned");

        // 3. reason key when neither rule nor rationale is present.
        let r3 = extract_one_line_reason(&outcome(
            StageId::CrossSourceTriangulation,
            StageOutcomeKind::Pass,
            json!({"reason": "cross-source convergence"}),
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
        ));
        assert_eq!(r3, "cross-source convergence");

        // 4. stage_id fallback when evidence has no string keys at all.
        let r4 = extract_one_line_reason(&outcome(
            StageId::PoliticallyExposedPersons,
            StageOutcomeKind::Pass,
            json!({}),
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
        ));
        assert_eq!(r4, StageId::PoliticallyExposedPersons.as_str());
    }

    #[test]
    fn threshold_snapshot_captures_calibration() {
        let t = LaneThresholds {
            green_authenticity_belief: 0.85,
            green_risk_belief: 0.20,
            green_max_ignorance: 0.30,
            red_authenticity_belief: 0.40,
            red_risk_belief: 0.70,
        };
        let snap = LaneThresholdsSnapshot::from_thresholds(t);
        assert_eq!(snap.red_risk_min, 0.70);
        assert_eq!(snap.yellow_risk_min, 0.20);
        assert_eq!(snap.red_authenticity_max, 0.40);
        assert_eq!(snap.yellow_authenticity_max, 0.85);
    }

    #[test]
    fn fusion_step_chain_matches_orchestrator_replay() {
        // Two evidence stages plus one BeliefFusion stage. The
        // BeliefFusion stage MUST NOT contribute a fusion step. The
        // running BPAs must equal the per-step Dempster combination.
        let outcomes = vec![
            outcome(
                StageId::IdentityAuthentication,
                StageOutcomeKind::Pass,
                json!({}),
                bpa(0.6, 0.0, 0.4),
                BasicProbabilityAssignment::vacuous(),
            ),
            outcome(
                StageId::SanctionsScreening,
                StageOutcomeKind::Pass,
                json!({}),
                bpa(0.6, 0.0, 0.4),
                BasicProbabilityAssignment::vacuous(),
            ),
            outcome(
                StageId::BeliefFusion,
                StageOutcomeKind::Pass,
                json!({}),
                BasicProbabilityAssignment::vacuous(),
                BasicProbabilityAssignment::vacuous(),
            ),
        ];
        let steps = compose_fusion_steps(&outcomes);
        assert_eq!(steps.len(), 2, "BeliefFusion stage must not produce a step");
        assert_eq!(steps[0].combined_stage, StageId::IdentityAuthentication);
        assert_eq!(steps[1].combined_stage, StageId::SanctionsScreening);

        // First step: vacuous ⊕ (0.6, 0, 0.4) = (0.6, 0, 0.4).
        let first = steps[0].running_authenticity;
        assert!((first.m_true - 0.6).abs() < 1e-9);
        assert!((first.m_uncertain - 0.4).abs() < 1e-9);

        // Second step: should strengthen belief (two supportive sources).
        let second = steps[1].running_authenticity;
        assert!(second.m_true > 0.6, "two supportive sources strengthen belief");
        assert!(second.m_true < 1.0);
    }

    #[test]
    fn lane_routing_stages_appear_in_rationales_but_not_fusion() {
        let outcomes = vec![
            outcome(
                StageId::SchemaValidation,
                StageOutcomeKind::Pass,
                json!({"rule": "schema.valid"}),
                bpa(0.9, 0.0, 0.1),
                BasicProbabilityAssignment::vacuous(),
            ),
            outcome(
                StageId::LaneRouting,
                StageOutcomeKind::Pass,
                json!({"rationale": "Green lane"}),
                BasicProbabilityAssignment::vacuous(),
                BasicProbabilityAssignment::vacuous(),
            ),
        ];
        let r = DecisionRationale::compose(
            VerificationCaseId::new(),
            Uuid::now_v7(),
            &outcomes,
            bpa(0.9, 0.0, 0.1),
            BasicProbabilityAssignment::vacuous(),
            LaneDecision::Green,
            LaneThresholds::default(),
        );
        assert_eq!(r.stage_rationales.len(), 2);
        assert_eq!(r.fusion_steps.len(), 1, "LaneRouting must not produce a fusion step");
        assert_eq!(r.fusion_steps[0].combined_stage, StageId::SchemaValidation);
    }
}
