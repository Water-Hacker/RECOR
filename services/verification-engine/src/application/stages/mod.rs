//! Concrete stage implementations.
//!
//! Stages 1, 2, 9 ship real logic in v1. Stages 3-7 ship as stubs
//! that return `InsufficientEvidence` with structured evidence
//! explaining the reason. Each stub corresponds to a follow-up ticket
//! that wires it to a real data source.
//!
//! Stage 8 (Dempster-Shafer fusion) is implemented in the orchestrator
//! rather than as a stage — the orchestrator already has access to
//! every stage outcome and the fusion is a pure function over them.

pub mod name_resolver;
pub mod stage3_sanctions;
pub mod stage4_pep;
pub mod stage5_adverse_media;
pub mod stage6_patterns;
pub mod stage_1_schema_validation;
pub mod stage_2_identity_authentication;
pub mod stage_3_sanctions_stub;
pub mod stage_4_pep_stub;
pub mod stage_5_adverse_media_stub;
pub mod stage_6_pattern_detection_stub;
pub mod stage_7_cross_source_real;
pub mod stage_7_cross_source_stub;

pub use name_resolver::BunecNameResolver;
pub use stage3_sanctions::{NameResolver, ResolvedName, SanctionsStage};
pub use stage4_pep::PepStage;
pub use stage5_adverse_media::AdverseMediaStage;
pub use stage6_patterns::PatternDetectionStage;
pub use stage_1_schema_validation::SchemaValidationStage;
pub use stage_2_identity_authentication::IdentityAuthenticationStage;
pub use stage_3_sanctions_stub::SanctionsStub;
pub use stage_4_pep_stub::PepStub;
pub use stage_5_adverse_media_stub::AdverseMediaStub;
pub use stage_6_pattern_detection_stub::PatternDetectionStub;
pub use stage_7_cross_source_real::CrossSourceTriangulationStage;
pub use stage_7_cross_source_stub::CrossSourceStub;
