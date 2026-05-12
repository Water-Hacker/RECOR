//! Stage 4 — Politically Exposed Persons. STUB.
//! Follow-up: R-VER-3 (commercial PEP feed + sovereign domestic PEP register).

use async_trait::async_trait;
use serde_json::json;
use crate::domain::{BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind};

pub struct PepStub;
impl PepStub { pub fn new() -> Self { Self } }
impl Default for PepStub { fn default() -> Self { Self::new() } }

#[async_trait]
impl Stage for PepStub {
    fn id(&self) -> StageId { StageId::PoliticallyExposedPersons }
    async fn run(&self, _d: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::PoliticallyExposedPersons,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({ "stub": true, "rationale": "R-VER-3: PEP feeds not yet integrated" }),
            duration_ms: 0,
        }
    }
}
