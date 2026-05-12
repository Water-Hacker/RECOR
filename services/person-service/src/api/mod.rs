//! HTTP API.

pub mod auth;
pub mod dto;
pub mod oidc;
pub mod openapi;
pub mod rest;

pub use auth::{AuthConfig, Principal, PrincipalSource};
pub use oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
pub use openapi::{build_openapi, openapi_routes, ApiDoc};
pub use rest::{router, AppState};
