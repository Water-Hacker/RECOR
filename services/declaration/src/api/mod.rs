//! HTTP API.

pub mod auth;
pub mod dto;
pub mod internal;
pub mod oidc;
pub mod rest;

pub use internal::{handle_verification_outcome, InternalAppState};
pub use oidc::{OidcVerifier, VerificationError};
pub use rest::{router, AppState};
