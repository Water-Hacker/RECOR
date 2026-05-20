//! Authentication. Two paths:
//!
//!   - Production: OIDC Bearer-token verification via the shared
//!     `recor-auth-oidc` crate. Real signature + iss + aud + exp + nbf.
//!   - Dev: `X-Recor-Dev-Principal` header. Gated by `Config::is_dev()`.
//!
//! D14 (fail-closed): bearer-token requests with no verifier configured
//! are rejected with 401. Config refuses to start in non-dev environments
//! without `OIDC_ISSUER_URL`.
//!
//! D17: every request that reaches a protected handler MUST have a
//! verified principal in the request extensions.

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
use crate::metrics::Metrics;

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
    pub source: PrincipalSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalSource {
    DevHeader,
    Bearer,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
    pub metrics: Arc<Metrics>,
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
        if let Some(value) = headers.get("x-recor-dev-principal") {
            let subject = value
                .to_str()
                .map_err(|_| ServiceError::BadRequest("malformed dev principal header".into()))?
                .trim()
                .to_string();
            if subject.is_empty() {
                return Err(ServiceError::BadRequest(
                    "empty dev principal header".into(),
                ));
            }
            return Ok(Principal {
                subject,
                source: PrincipalSource::DevHeader,
            });
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
        let label = match &e {
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => "unavailable",
            _ => "invalid",
        };
        state
            .metrics
            .oidc_verify_total
            .with_label_values(&[label])
            .inc();
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
                ServiceError::AuthenticationRequired
            }
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => ServiceError::Internal,
        }
    })?;

    if claims.sub.trim().is_empty() {
        state
            .metrics
            .oidc_verify_total
            .with_label_values(&["invalid"])
            .inc();
        return Err(ServiceError::AuthenticationRequired);
    }
    state
        .metrics
        .oidc_verify_total
        .with_label_values(&["success"])
        .inc();
    Ok(Principal {
        subject: claims.sub,
        source: PrincipalSource::Bearer,
    })
}
