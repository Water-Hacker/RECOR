//! OIDC verifier re-export so the auth module can reach it via a stable
//! local path identical to recor-declaration. The shared implementation
//! lives in `recor-auth-oidc` (R-AUTH-1).

pub use recor_auth_oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
