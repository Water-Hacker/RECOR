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

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
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
            return Ok(Principal { subject });
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
    Ok(Principal {
        subject: claims.sub,
    })
}
