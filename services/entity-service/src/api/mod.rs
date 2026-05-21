//! HTTP API.

pub mod auth;
pub mod dlq;
pub mod dto;
pub mod oidc;
pub mod openapi;
pub mod rest;

pub use auth::{AuthConfig, Principal, PrincipalSource};
pub use dlq::{list_dlq, replay_dlq, DlqAdminState};
pub use oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
pub use openapi::{build_openapi, openapi_routes, ApiDoc};
pub use rest::{metrics_only_router, router, AppState};
