//! Stage 5 — Adverse media & ICIJ leaks. STUB.
//! Follow-up: R-VER-4 (multilingual news corpus + ICIJ archives + Tier A RAG reasoning).

use async_trait::async_trait;
use serde_json::json;
use crate::domain::{BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind};

pub struct AdverseMediaStub;
impl AdverseMediaStub { pub fn new() -> Self { Self } }
impl Default for AdverseMediaStub { fn default() -> Self { Self::new() } }

#[async_trait]
impl Stage for AdverseMediaStub {
    fn id(&self) -> StageId { StageId::AdverseMedia }
    async fn run(&self, _d: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::AdverseMedia,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({ "stub": true, "rationale": "R-VER-4: news corpus + ICIJ ingestion + Tier A reasoning not yet implemented" }),
            duration_ms: 0,
        }
    }
}
