//! Re-export of the OIDC verifier from the shared `recor-auth-oidc` crate.
//!
//! R-AUTH-1 (#46) extracted the implementation into a workspace-level
//! shared crate so both services use the same code path. This module
//! exists only to preserve the existing import paths used by
//! `src/api/auth.rs`, `src/api/rest.rs`, and `src/main.rs`.

pub use recor_auth_oidc::*;
