//! Domain layer for the Verification Engine.
//!
//! Pure types, no I/O. The Dempster-Shafer fusion library, the Stage
//! trait, the VerificationCase aggregate, and the LaneDecision logic
//! all live here.

pub mod case;
pub mod declaration_snapshot;
pub mod decision_rationale;
pub mod fusion;
pub mod lane;
pub mod serde_helpers;
pub mod stage;

pub use case::{VerificationCase, VerificationCaseId};
pub use declaration_snapshot::{DeclarationSnapshot, OwnerSnapshot};
pub use decision_rationale::{
    DecisionRationale, FusionStep, LaneThresholdsSnapshot, StageRationale,
};
pub use fusion::{BeliefMass, BinaryFrame, BasicProbabilityAssignment, FusionError};
pub use lane::{LaneDecision, LaneThresholds};
pub use stage::{Stage, StageId, StageOutcome, StageOutcomeKind};
