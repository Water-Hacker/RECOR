//! Re-export of the OIDC verifier from the shared `recor-auth-oidc` crate.
//!
//! See `services/declaration/src/api/oidc.rs` for the rationale — same
//! pattern, single shared crate, no duplication.

pub use recor_auth_oidc::*;
