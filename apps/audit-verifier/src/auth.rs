//! Authentication for the audit-verifier read surface.
//!
//! FIND-001 (audit Sprint 0): the verifier's
//! `GET /v1/audit/verify/{declaration_id}` returns the projection
//! payload — which carries declarant PII (name, principal id,
//! beneficial-ownership graph) — alongside the on-chain anchor. The
//! pre-Sprint-0 deployment exposed this surface unauthenticated under
//! the "public read surface" framing. Public re-derivation of the
//! receipt hash does NOT require returning the projection body; an
//! authenticated principal does. Gate the endpoint behind the same
//! OIDC verifier the rest of the platform uses.
//!
//! Two paths, mirroring `services/declaration/src/api/auth.rs`:
//!   - Production: OIDC Bearer-token verification via `recor-auth-oidc`.
//!   - Dev: `X-Recor-Dev-Principal` header, gated on `is_dev`.
//!
//! FIND-003 carry-over: `VerifierConfig::from_env` refuses to start
//! with `ENVIRONMENT=dev` AND a configured `OIDC_ISSUER_URL` — the
//! dev backdoor would otherwise bypass OIDC entirely.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use recor_auth_oidc::{OidcVerifier, VerificationError};
use tracing::warn;

/// Post-Sovim authorisation tier.
///
/// Sovim (CJEU C-37/20 + C-601/20) struck down the public-by-default
/// model for beneficial-ownership registries; access must be tiered so
/// that:
///
/// - **Admin** — competent authority, FIU, supervisor. Sees the full
///   canonical payload including national-ID-numbers, residential
///   addresses, biometric hashes, and the signer's public key
///   (REQ-fatf-c24-008-fn-27).
/// - **ObligedEntity** — regulated counter-party with a supervised
///   legitimate-interest claim (REQ-amld-iv-005). Sees a reduced
///   payload: national-ID-numbers, residential addresses, biometric
///   hashes, and signer-public-key are **never** disclosed.
/// - **PublicLegitimateInterest** — journalist or civil-society caller
///   admitted through the Sovim balancing test (REQ-cjeu-sovim-006).
///   Sees the strict minimum: cryptographic verification outcome,
///   counts, and entry-level Matched / Mismatch / Missing status. No
///   timestamps, no transaction IDs, no event types — those constitute
///   per-event metadata that bulk-scrapers could correlate.
///
/// The default tier on any token whose `scope` claim does NOT match a
/// known scope is **PublicLegitimateInterest** — fail-closed (D14).
/// TODO-006 will wire the OIDC scope `recor:obliged-entity` against
/// the supervisor onboarding workflow; the matching is already in
/// place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationTier {
    Admin,
    ObligedEntity,
    PublicLegitimateInterest,
}

impl AuthorizationTier {
    /// Parse a space-delimited OIDC `scope` claim string. Returns the
    /// most-privileged tier the claim's scopes resolve to — admin
    /// outranks obliged-entity outranks public.
    pub fn from_scope_claim(scope: &str) -> Self {
        let mut found = AuthorizationTier::PublicLegitimateInterest;
        for s in scope.split_whitespace() {
            match s {
                "recor:admin" => return AuthorizationTier::Admin,
                "recor:obliged-entity" => found = AuthorizationTier::ObligedEntity,
                _ => {}
            }
        }
        found
    }

    /// Parse the dev-mode `X-Recor-Dev-Scope` header value. Case-
    /// insensitive. Unrecognised → PublicLegitimateInterest (D14).
    pub fn from_dev_header(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => AuthorizationTier::Admin,
            "obliged-entity" | "obliged_entity" => AuthorizationTier::ObligedEntity,
            _ => AuthorizationTier::PublicLegitimateInterest,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
    /// Post-Sovim authorisation tier; drives per-tier redaction in the
    /// verifier response. TODO-007 / TODO-023 / FIND-007.
    pub tier: AuthorizationTier,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
}

#[derive(Debug)]
pub enum AuthError {
    AuthenticationRequired,
    BadRequest(&'static str),
    Internal,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::AuthenticationRequired => {
                (StatusCode::UNAUTHORIZED, "authentication required").into_response()
            }
            AuthError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AuthError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal failure").into_response()
            }
        }
    }
}

pub async fn auth_middleware(
    State(state): State<AuthConfig>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let principal = resolve_principal(req.headers(), &state).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

async fn resolve_principal(
    headers: &HeaderMap,
    state: &AuthConfig,
) -> Result<Principal, AuthError> {
    if state.is_dev {
        if let Some(value) = headers.get("x-recor-dev-principal") {
            let subject = value
                .to_str()
                .map_err(|_| AuthError::BadRequest("malformed dev principal header"))?
                .trim()
                .to_string();
            if subject.is_empty() {
                return Err(AuthError::BadRequest("empty dev principal header"));
            }
            // Resolve the dev tier from `X-Recor-Dev-Scope`. The
            // fail-closed default is PublicLegitimateInterest — the
            // tightest payload subset — so a forgotten header in a
            // local integration test never accidentally surfaces an
            // admin response.
            let tier = headers
                .get("x-recor-dev-scope")
                .and_then(|v| v.to_str().ok())
                .map(AuthorizationTier::from_dev_header)
                .unwrap_or(AuthorizationTier::PublicLegitimateInterest);
            return Ok(Principal { subject, tier });
        }
    }

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        return Err(AuthError::AuthenticationRequired);
    };

    let Some(verifier) = state.oidc.as_ref() else {
        warn!("bearer token received but no OIDC verifier configured");
        return Err(AuthError::AuthenticationRequired);
    };

    let claims = verifier.verify(token).await.map_err(|e| {
        warn!(error = %e, "bearer token failed verification");
        match e {
            VerificationError::TokenInvalid(_)
            | VerificationError::MalformedHeader
            | VerificationError::MissingKid
            | VerificationError::UnknownKid(_)
            | VerificationError::UnsupportedAlgorithm(_)
            | VerificationError::NoUsableKey
            | VerificationError::MissingClaim(_)
            | VerificationError::InsufficientAssurance { .. }
            | VerificationError::SubjectClaimAbsent { .. } => {
                AuthError::AuthenticationRequired
            }
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => AuthError::Internal,
        }
    })?;

    if claims.sub.trim().is_empty() {
        return Err(AuthError::AuthenticationRequired);
    }
    // Sovim tier resolution from the verified OIDC `scope` claim
    // inside `claims.raw`. Empty / absent claim →
    // PublicLegitimateInterest (the tightest subset) per D14
    // fail-closed.
    let tier = claims
        .raw
        .get("scope")
        .and_then(|v| v.as_str())
        .map(AuthorizationTier::from_scope_claim)
        .unwrap_or(AuthorizationTier::PublicLegitimateInterest);
    Ok(Principal {
        subject: claims.sub,
        tier,
    })
}

#[cfg(test)]
mod tier_tests {
    use super::*;

    #[test]
    fn admin_scope_wins_over_obliged_entity() {
        assert_eq!(
            AuthorizationTier::from_scope_claim("recor:obliged-entity recor:admin"),
            AuthorizationTier::Admin
        );
    }

    #[test]
    fn obliged_entity_when_only_obliged_present() {
        assert_eq!(
            AuthorizationTier::from_scope_claim("openid recor:obliged-entity"),
            AuthorizationTier::ObligedEntity
        );
    }

    #[test]
    fn unknown_scope_defaults_to_public_legitimate_interest() {
        assert_eq!(
            AuthorizationTier::from_scope_claim("openid profile"),
            AuthorizationTier::PublicLegitimateInterest
        );
        assert_eq!(
            AuthorizationTier::from_scope_claim(""),
            AuthorizationTier::PublicLegitimateInterest
        );
    }

    #[test]
    fn dev_header_admin() {
        assert_eq!(
            AuthorizationTier::from_dev_header("admin"),
            AuthorizationTier::Admin
        );
        assert_eq!(
            AuthorizationTier::from_dev_header("ADMIN"),
            AuthorizationTier::Admin
        );
    }

    #[test]
    fn dev_header_obliged_entity_either_spelling() {
        assert_eq!(
            AuthorizationTier::from_dev_header("obliged-entity"),
            AuthorizationTier::ObligedEntity
        );
        assert_eq!(
            AuthorizationTier::from_dev_header("obliged_entity"),
            AuthorizationTier::ObligedEntity
        );
    }

    #[test]
    fn dev_header_unknown_is_public() {
        assert_eq!(
            AuthorizationTier::from_dev_header("hacker"),
            AuthorizationTier::PublicLegitimateInterest
        );
    }
}
