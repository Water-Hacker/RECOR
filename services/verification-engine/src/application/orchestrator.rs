//! Pipeline orchestrator.
//!
//! Runs stages in order:
//!   Stage 1 (Schema)             — short-circuits the rest on fail
//!   Stages 2-7 (Evidence sources) — produce BPAs, can return InsufficientEvidence
//!   Stage 8 (Fusion)              — combines authenticity + risk BPAs via Dempster
//!   Stage 9 (Lane Routing)        — applies thresholds, decides green/yellow/red
//!
//! TODO-049 — after fusion + lane routing, the orchestrator composes a
//! `DecisionRationale` carrying the per-stage reasoning, the fusion
//! chain, and the threshold snapshot used at adjudication time. The
//! rationale is persisted alongside the case (same transaction) so
//! every adjudicated case has an immutable, defensible explanation.

use std::sync::Arc;

use time::OffsetDateTime;
use tracing::{info, instrument, warn};

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, DecisionRationale, LaneDecision,
    LaneThresholds, Stage, StageId, StageOutcome, StageOutcomeKind, VerificationCase,
    VerificationCaseId,
};

pub struct PipelineOrchestrator {
    stages: Vec<Arc<dyn Stage>>,
    thresholds: LaneThresholds,
}

/// Result of one pipeline run: the case plus the rationale composed
/// from its stages, fusion chain, and lane thresholds. The two are
/// always emitted as a pair so the caller persists them atomically.
pub struct PipelineOutcome {
    pub case: VerificationCase,
    pub rationale: DecisionRationale,
}

impl PipelineOrchestrator {
    pub fn new(stages: Vec<Arc<dyn Stage>>, thresholds: LaneThresholds) -> Self {
        // Sanity check: stages are sorted by id ascending.
        let mut sorted = stages;
        sorted.sort_by_key(|s| s.id().ordinal());
        Self { stages: sorted, thresholds }
    }

    /// Expose the thresholds used for adjudication. The rationale
    /// composer captures these as part of every case so a calibration
    /// change does not retroactively shift the audit interpretation
    /// of past cases.
    pub fn thresholds(&self) -> LaneThresholds {
        self.thresholds
    }

    #[instrument(skip_all, fields(declaration_id = %declaration.declaration_id))]
    pub async fn run(&self, declaration: DeclarationSnapshot) -> PipelineOutcome {
        let case_id = VerificationCaseId::new();
        let started = OffsetDateTime::now_utc();
        let start_instant = std::time::Instant::now();
        let mut outcomes: Vec<StageOutcome> = Vec::with_capacity(self.stages.len());
        let mut short_circuited = false;

        for stage in &self.stages {
            if short_circuited {
                // Skip remaining stages, record their absence as an
                // insufficient-evidence vacuous outcome so the case
                // explicitly records every stage's status.
                outcomes.push(StageOutcome {
                    stage_id: stage.id(),
                    kind: StageOutcomeKind::InsufficientEvidence,
                    authenticity_bpa: BasicProbabilityAssignment::vacuous(),
                    risk_bpa: BasicProbabilityAssignment::vacuous(),
                    evidence: serde_json::json!({
                        "skipped": true,
                        "reason": "earlier stage short-circuited the pipeline",
                    }),
                    duration_ms: 0,
                });
                continue;
            }
            let outcome = stage
                .run_with_context(&declaration, &outcomes)
                .await;
            if matches!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed) {
                warn!(stage_id = ?stage.id(), "stage short-circuited pipeline fail-closed");
                short_circuited = true;
            }
            outcomes.push(outcome);
        }

        // Stage 8: fusion of every contributing outcome.
        let (fused_authenticity, fused_risk) = fuse_outcomes(&outcomes);

        // Stage 9: lane routing.
        let lane = if short_circuited {
            LaneDecision::Red
        } else {
            self.thresholds.route(fused_authenticity, fused_risk)
        };

        let completed = OffsetDateTime::now_utc();
        let total_duration_ms = u64::try_from(start_instant.elapsed().as_millis()).unwrap_or(u64::MAX);

        info!(
            case_id = %case_id,
            lane = lane.as_str(),
            authenticity_belief = fused_authenticity.belief_true(),
            authenticity_plausibility = fused_authenticity.plausibility_true(),
            risk_belief = fused_risk.belief_true(),
            stage_count = outcomes.len(),
            total_duration_ms,
            "pipeline complete"
        );

        // TODO-049 — compose the rationale BEFORE the case takes
        // ownership of `declaration` so the borrow checker permits
        // both. The composer is pure; no I/O on this path.
        let declaration_id = declaration.declaration_id;
        let rationale = DecisionRationale::compose(
            case_id,
            declaration_id,
            &outcomes,
            fused_authenticity,
            fused_risk,
            lane,
            self.thresholds,
        );

        let case = VerificationCase {
            case_id,
            declaration,
            stage_outcomes: outcomes,
            fused_authenticity,
            fused_risk,
            lane,
            created_at: started,
            completed_at: completed,
            total_duration_ms,
        };

        PipelineOutcome { case, rationale }
    }
}

/// Fuse every stage's BPAs into a single authenticity and risk BPA.
/// Uses Dempster's rule; falls back to Yager on a hit of total
/// conflict.
fn fuse_outcomes(
    outcomes: &[StageOutcome],
) -> (BasicProbabilityAssignment, BasicProbabilityAssignment) {
    let auth_bpas: Vec<BasicProbabilityAssignment> = outcomes
        .iter()
        // Stage 8 + 9 don't contribute to fusion; their BPAs are vacuous
        // anyway, but be explicit.
        .filter(|o| {
            !matches!(o.stage_id, StageId::BeliefFusion | StageId::LaneRouting)
        })
        .map(|o| o.authenticity_bpa)
        .collect();
    let risk_bpas: Vec<BasicProbabilityAssignment> = outcomes
        .iter()
        .filter(|o| {
            !matches!(o.stage_id, StageId::BeliefFusion | StageId::LaneRouting)
        })
        .map(|o| o.risk_bpa)
        .collect();

    let auth = fuse_with_yager_fallback(auth_bpas);
    let risk = fuse_with_yager_fallback(risk_bpas);
    (auth, risk)
}

fn fuse_with_yager_fallback(
    bpas: Vec<BasicProbabilityAssignment>,
) -> BasicProbabilityAssignment {
    let mut acc = BasicProbabilityAssignment::vacuous();
    for bpa in bpas {
        match acc.combine(bpa) {
            Ok(c) => acc = c,
            Err(_) => acc = acc.combine_yager(bpa),
        }
    }
    acc
}
