//! VerificationCase — the aggregate that records the result of running
//! a declaration through the pipeline. One case per declaration.
//!
//! A case carries:
//!   * the declaration snapshot (the immutable input)
//!   * the ordered list of stage outcomes
//!   * the fused authenticity and risk BPAs
//!   * the final lane decision
//!   * timestamps and provenance

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::declaration_snapshot::DeclarationSnapshot;
use super::fusion::BasicProbabilityAssignment;
use super::lane::LaneDecision;
use super::stage::StageOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerificationCaseId(pub Uuid);

impl VerificationCaseId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for VerificationCaseId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VerificationCaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCase {
    pub case_id: VerificationCaseId,
    pub declaration: DeclarationSnapshot,
    pub stage_outcomes: Vec<StageOutcome>,
    pub fused_authenticity: BasicProbabilityAssignment,
    pub fused_risk: BasicProbabilityAssignment,
    pub lane: LaneDecision,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub completed_at: time::OffsetDateTime,
    /// Total wall-clock duration of the pipeline, including all stages.
    pub total_duration_ms: u64,
}
