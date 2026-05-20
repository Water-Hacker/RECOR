//! HTTP + gRPC API.

pub mod auth;
pub mod discrepancies;
pub mod dlq;
pub mod dto;
pub mod fiu;
pub mod grpc;
pub mod internal;
pub mod oidc;
pub mod openapi;
pub mod public_feedback;
pub mod rate_limit;
pub mod rest;
pub mod sanctions;

pub use dlq::{DlqAdminState, list_dlq, replay_dlq};
pub use grpc::{DeclarationGrpcService, GrpcAuthConfig};
pub use internal::{handle_verification_outcome, InternalAppState};
pub use oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
pub use openapi::{build_openapi, openapi_routes, ApiDoc};
pub use rest::{metrics_only_router, router, AppState};
