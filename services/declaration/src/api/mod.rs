//! HTTP API.

pub mod auth;
pub mod dlq;
pub mod dto;
pub mod internal;
pub mod oidc;
pub mod rest;

pub use dlq::{DlqAdminState, list_dlq, replay_dlq};
pub use internal::{handle_verification_outcome, InternalAppState};
pub use oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
pub use rest::{router, AppState};
