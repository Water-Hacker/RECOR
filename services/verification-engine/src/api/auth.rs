//! Authentication middleware — mirrors the pattern from
//! services/declaration. Dev path accepts `X-Recor-Dev-Principal`;
//! production verifies the Bearer JWT against the configured OIDC
//! issuer's JWKS (`crate::api::oidc::OidcVerifier`).
//!
//! D14 (fail-closed): bearer tokens with no verifier configured are
//! rejected with 401. Production config refuses to start when
//! `OIDC_ISSUER_URL` is unset and `environment != "dev"`.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};
use tracing::warn;

use crate::api::oidc::{OidcVerifier, VerificationError};
use crate::error::ServiceError;

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
}

pub async fn auth_middleware(
    State(state): State<AuthConfig>,
    mut req: Request,
    next: Next,
) -> Result<Response, ServiceError> {
    let principal = resolve_principal(req.headers(), &state).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

async fn resolve_principal(
    headers: &HeaderMap,
    state: &AuthConfig,
) -> Result<Principal, ServiceError> {
    if state.is_dev {
        if let Some(v) = headers.get("x-recor-dev-principal") {
            let subject = v
                .to_str()
                .map_err(|_| ServiceError::BadRequest("malformed dev principal header".into()))?
                .trim()
                .to_string();
            if !subject.is_empty() {
                return Ok(Principal { subject });
            }
        }
    }
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let Some(token) = bearer else {
        return Err(ServiceError::AuthenticationRequired);
    };

    let Some(verifier) = state.oidc.as_ref() else {
        warn!("bearer token received but no OIDC verifier configured");
        return Err(ServiceError::AuthenticationRequired);
    };

    let claims = verifier.verify(token).await.map_err(|e| {
        warn!(error = %e, "bearer token failed verification");
        match e {
            VerificationError::TokenInvalid(_)
            | VerificationError::MalformedHeader
            | VerificationError::MissingKid
            | VerificationError::UnknownKid(_)
            | VerificationError::UnsupportedAlgorithm(_)
            | VerificationError::NoUsableKey => ServiceError::AuthenticationRequired,
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => ServiceError::Internal,
        }
    })?;

    if claims.sub.trim().is_empty() {
        return Err(ServiceError::AuthenticationRequired);
    }
    Ok(Principal { subject: claims.sub })
}
