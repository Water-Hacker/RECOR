//! Stage 3 — Sanctions screening. STUB.
//!
//! Real implementation requires daily-refreshed UN / OFAC / EU / UK
//! sanctions feeds + Tier A Anthropic reasoning over ambiguous fuzzy
//! matches. None of those data pipelines exist yet.
//!
//! v1 returns InsufficientEvidence (vacuous BPA) — the engine
//! correctly notes that this signal class has not yet been measured
//! and the lane router treats the gap as ignorance.
//!
//! Follow-up: R-VER-2 (sanctions feed ingestion + Tier A reasoning).

use async_trait::async_trait;
use serde_json::json;

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind,
};

pub struct SanctionsStub;
impl SanctionsStub { pub fn new() -> Self { Self } }
impl Default for SanctionsStub { fn default() -> Self { Self::new() } }

#[async_trait]
impl Stage for SanctionsStub {
    fn id(&self) -> StageId { StageId::SanctionsScreening }

    async fn run(&self, _declaration: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::SanctionsScreening,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({
                "stub": true,
                "rationale": "R-VER-2 follow-up: sanctions feed ingestion + Tier A reasoning not yet implemented",
                "v1_signal": "vacuous",
            }),
            duration_ms: 0,
        }
    }
}
