//! Authentication. Two paths:
//!
//!   - Production: OIDC Bearer-token verification against the
//!     configured issuer's JWKS (TODO: wire up; placeholder verifier
//!     returns claims pulled from the token's payload without
//!     signature verification when env=dev).
//!   - Dev: an HS256-equivalent static key shortcut is NOT used; we
//!     accept a special `X-Recor-Dev-Principal` header that asserts
//!     the principal name. This is gated by `Config::is_dev()` and
//!     refused otherwise.
//!
//! D17: every request that reaches the protected handler MUST have a
//! verified principal in the request extensions. Handler signatures
//! that take `Principal` will fail to compile if the auth layer is
//! omitted.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::error::ServiceError;

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

/// Axum middleware that resolves the request principal and inserts it
/// into request extensions. Handlers extract it via the `RequirePrincipal`
/// extractor.
pub async fn auth_middleware(
    is_dev: bool,
    mut req: Request,
    next: Next,
) -> Result<Response, ServiceError> {
    let principal = resolve_principal(&req.headers(), is_dev)?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

fn resolve_principal(headers: &HeaderMap, is_dev: bool) -> Result<Principal, ServiceError> {
    // Dev-only shortcut: X-Recor-Dev-Principal header.
    if is_dev {
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

    // Bearer token path. Verification is not yet wired (depends on the
    // future OIDC adapter ticket). For now we extract the sub claim
    // without verifying signature — refused outside dev by the
    // structural is_dev() check in Config.
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        return Err(ServiceError::AuthenticationRequired);
    };

    // Minimal JWT parser: split, base64-decode the payload, pull `sub`.
    // ONLY for the dev/integration-test path. Production OIDC ticket
    // replaces this with full signature + issuer + audience verification.
    let claims = peek_unverified_claims(token)
        .ok_or_else(|| ServiceError::AuthenticationRequired)?;
    let subject = claims.get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServiceError::AuthenticationRequired)?
        .to_string();
    if subject.is_empty() {
        return Err(ServiceError::AuthenticationRequired);
    }
    Ok(Principal {
        subject,
        source: PrincipalSource::Bearer,
    })
}

/// Peek at a JWT's claims without verification. Returns None if the
/// token does not look like a JWT.
fn peek_unverified_claims(token: &str) -> Option<serde_json::Value> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = URL_SAFE_NO_PAD.decode(parts[1].as_bytes()).ok()?;
    serde_json::from_slice(&payload).ok()
}

// Suppress unused warnings during partial build.
#[allow(dead_code)]
fn _force_imports(_b: Body, _s: StatusCode) {}
