//! Stage trait + outcome types.
//!
//! Each verification stage is an independent unit. Stages run in a
//! defined order (1 → 9). Stages 1-2 are deterministic (no AI). Stages
//! 3-5 reach external feeds (sanctions, PEP, adverse media). Stages 6-7
//! do pattern detection. Stage 8 is the Dempster-Shafer fusion of the
//! evidence from stages 2-7. Stage 9 routes to a lane based on fused
//! scores.
//!
//! Every stage produces a `StageOutcome` carrying:
//!   * A kind (Pass / Fail / InsufficientEvidence)
//!   * A BPA contributing to authenticity reasoning (for stages 2-7)
//!   * Structured evidence (a serde JSON value) explaining the decision
//!
//! Stages do NOT mutate the case directly; the orchestrator collects
//! their outcomes and the fusion stage produces the final BPA.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::declaration_snapshot::DeclarationSnapshot;
use super::fusion::BasicProbabilityAssignment;

/// Stable identifier for a stage. The orchestrator runs stages in
/// ascending numeric order. Each id corresponds to one Architecture
/// V4 P14 stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum StageId {
    SchemaValidation = 1,
    IdentityAuthentication = 2,
    SanctionsScreening = 3,
    PoliticallyExposedPersons = 4,
    AdverseMedia = 5,
    PatternDetection = 6,
    CrossSourceTriangulation = 7,
    BeliefFusion = 8,
    LaneRouting = 9,
}

impl StageId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SchemaValidation => "schema_validation",
            Self::IdentityAuthentication => "identity_authentication",
            Self::SanctionsScreening => "sanctions_screening",
            Self::PoliticallyExposedPersons => "politically_exposed_persons",
            Self::AdverseMedia => "adverse_media",
            Self::PatternDetection => "pattern_detection",
            Self::CrossSourceTriangulation => "cross_source_triangulation",
            Self::BeliefFusion => "belief_fusion",
            Self::LaneRouting => "lane_routing",
        }
    }

    pub fn ordinal(self) -> u8 {
        self as u8
    }
}

/// Classification of a stage's outcome at the high level. Detail goes
/// in the `evidence` JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOutcomeKind {
    /// Evidence supports authenticity / no risk concern.
    Pass,
    /// Evidence contradicts authenticity / raises risk.
    Fail,
    /// Stage could not produce evidence either way; contributes
    /// vacuous BPA to fusion.
    InsufficientEvidence,
    /// Stage encountered a deterministic short-circuit (e.g. Stage 1
    /// schema invalid). Pipeline fails closed; later stages do not run.
    ShortCircuitFailClosed,
}

/// One stage's result for one declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutcome {
    pub stage_id: StageId,
    pub kind: StageOutcomeKind,
    /// BPA contribution to authenticity fusion. Stage 8 (fusion itself)
    /// and Stage 9 (lane routing) return `BasicProbabilityAssignment::vacuous()`
    /// because they are not evidence sources — they consume evidence.
    pub authenticity_bpa: BasicProbabilityAssignment,
    /// BPA contribution to risk fusion. Vacuous for stages that don't
    /// produce risk signal.
    pub risk_bpa: BasicProbabilityAssignment,
    /// Structured evidence — stage-specific JSON shape. Surfaced in the
    /// case detail for analyst review.
    pub evidence: serde_json::Value,
    /// Wall-clock duration the stage took, in milliseconds.
    pub duration_ms: u64,
}

/// The trait every stage implements. Stages are stateless and
/// reentrant; orchestrator instantiates them once and reuses.
#[async_trait]
pub trait Stage: Send + Sync {
    fn id(&self) -> StageId;

    /// Process a declaration. Returns a `StageOutcome`. Implementations
    /// should never panic; any infrastructure failure (DB down, network
    /// timeout) returns an `InsufficientEvidence` outcome with the
    /// failure reason in the evidence JSON.
    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome;
}
