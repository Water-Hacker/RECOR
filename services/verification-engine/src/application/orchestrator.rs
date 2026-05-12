//! Pipeline orchestrator.
//!
//! Runs stages in order:
//!   Stage 1 (Schema)             — short-circuits the rest on fail
//!   Stages 2-7 (Evidence sources) — produce BPAs, can return InsufficientEvidence
//!   Stage 8 (Fusion)              — combines authenticity + risk BPAs via Dempster
//!   Stage 9 (Lane Routing)        — applies thresholds, decides green/yellow/red

use std::sync::Arc;

use time::OffsetDateTime;
use tracing::{info, instrument, warn};

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, LaneDecision, LaneThresholds, Stage, StageId,
    StageOutcome, StageOutcomeKind, VerificationCase, VerificationCaseId,
};

pub struct PipelineOrchestrator {
    stages: Vec<Arc<dyn Stage>>,
    thresholds: LaneThresholds,
}

impl PipelineOrchestrator {
    pub fn new(stages: Vec<Arc<dyn Stage>>, thresholds: LaneThresholds) -> Self {
        // Sanity check: stages are sorted by id ascending.
        let mut sorted = stages;
        sorted.sort_by_key(|s| s.id().ordinal());
        Self { stages: sorted, thresholds }
    }

    #[instrument(skip_all, fields(declaration_id = %declaration.declaration_id))]
    pub async fn run(&self, declaration: DeclarationSnapshot) -> VerificationCase {
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
            let outcome = stage.run(&declaration).await;
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

        VerificationCase {
            case_id,
            declaration,
            stage_outcomes: outcomes,
            fused_authenticity,
            fused_risk,
            lane,
            created_at: started,
            completed_at: completed,
            total_duration_ms,
        }
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
