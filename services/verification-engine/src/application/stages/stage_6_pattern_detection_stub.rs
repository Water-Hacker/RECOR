//! Stage 6 — Pattern detection (8 signature classes). STUB.
//! Follow-up: R-VER-5 (each of the 8 signature detectors + Tier B reasoning).

use async_trait::async_trait;
use serde_json::json;
use crate::domain::{BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind};

pub struct PatternDetectionStub;
impl PatternDetectionStub { pub fn new() -> Self { Self } }
impl Default for PatternDetectionStub { fn default() -> Self { Self::new() } }

#[async_trait]
impl Stage for PatternDetectionStub {
    fn id(&self) -> StageId { StageId::PatternDetection }
    async fn run(&self, _d: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::PatternDetection,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({ "stub": true, "rationale": "R-VER-5: 8 signature detectors + ownership-graph (Neo4j) not yet wired" }),
            duration_ms: 0,
        }
    }
}
