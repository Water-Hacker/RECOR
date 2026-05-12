//! Authentication. Two paths:
//!
//!   - Production: OIDC Bearer-token verification against the
//!     configured issuer's JWKS. Real signature + iss + aud + exp +
//!     nbf checking via `crate::api::oidc::OidcVerifier`. The verifier
//!     is constructed at startup; the middleware shares an `Arc<_>`.
//!   - Dev: an HS256-equivalent static key shortcut is NOT used; we
//!     accept a special `X-Recor-Dev-Principal` header that asserts
//!     the principal name. This is gated by `Config::is_dev()` and
//!     refused otherwise.
//!
//! D14 (fail-closed): bearer-token requests with no verifier configured
//! are rejected with 401, not silently allowed through. The config
//! layer refuses to start outside dev when `OIDC_ISSUER_URL` is empty,
//! so a production deployment cannot land in the "no verifier" state.
//!
//! D17: every request that reaches the protected handler MUST have a
//! verified principal in the request extensions.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
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

/// Shared state for the auth middleware. `None` for `oidc` means no
/// verifier was configured at startup — bearer tokens are then rejected.
/// Dev-header path still works if `is_dev == true`.
#[derive(Clone)]
pub struct AuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
    /// OBS-1: shared Prometheus registry so the middleware can record
    /// per-verify outcomes (`recor_oidc_verify_total{result}`). The
    /// label `result` is a 3-value bounded enum (D18).
    pub metrics: Arc<Metrics>,
}

/// Axum middleware that resolves the request principal and inserts it
/// into request extensions. Handlers extract it via the `RequirePrincipal`
/// extractor.
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
    // Dev-only shortcut: X-Recor-Dev-Principal header.
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

    // Bearer token path.
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        return Err(ServiceError::AuthenticationRequired);
    };

    let Some(verifier) = state.oidc.as_ref() else {
        // Defensive: should not be reachable in production because
        // Config refuses to start when OIDC_ISSUER_URL is unset and
        // environment != "dev". Log loudly if we see it anyway.
        warn!("bearer token received but no OIDC verifier configured");
        return Err(ServiceError::AuthenticationRequired);
    };

    let claims = verifier.verify(token).await.map_err(|e| {
        warn!(error = %e, "bearer token failed verification");
        // OBS-1: bounded-cardinality outcome label. `unavailable` is
        // an infrastructure fault (JWKS / discovery 5xx) — distinct
        // from a client-side `invalid` so on-call can tell apart
        // "OIDC backend down" from "bad tokens flooding in".
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

// Suppress unused warnings during partial build.
#[allow(dead_code)]
fn _force_imports(_b: Body, _s: StatusCode) {}
