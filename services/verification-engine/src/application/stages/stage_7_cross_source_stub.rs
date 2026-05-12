//! Stage 7 — Cross-source triangulation against ARMP/DGI/concession cadastres. STUB.
//! Follow-up: R-VER-6 (cross-source consumer integrations + Tier B reasoning).

use async_trait::async_trait;
use serde_json::json;
use crate::domain::{BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind};

pub struct CrossSourceStub;
impl CrossSourceStub { pub fn new() -> Self { Self } }
impl Default for CrossSourceStub { fn default() -> Self { Self::new() } }

#[async_trait]
impl Stage for CrossSourceStub {
    fn id(&self) -> StageId { StageId::CrossSourceTriangulation }
    async fn run(&self, _d: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::CrossSourceTriangulation,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({ "stub": true, "rationale": "R-VER-6: ARMP/DGI/concession integrations not yet implemented" }),
            duration_ms: 0,
        }
    }
}
