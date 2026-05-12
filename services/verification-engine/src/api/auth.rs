//! Authentication middleware — mirrors the pattern from
//! services/declaration. Dev path accepts `X-Recor-Dev-Principal`;
//! production requires Bearer JWT (signature verification is stubbed
//! pending R-VER-7).

use axum::{
    extract::Request,
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};

use crate::error::ServiceError;

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
}

pub async fn auth_middleware(
    is_dev: bool,
    mut req: Request,
    next: Next,
) -> Result<Response, ServiceError> {
    let principal = resolve_principal(req.headers(), is_dev)?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

fn resolve_principal(headers: &HeaderMap, is_dev: bool) -> Result<Principal, ServiceError> {
    if is_dev {
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
    let claims = peek_unverified_claims(token).ok_or(ServiceError::AuthenticationRequired)?;
    let subject = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or(ServiceError::AuthenticationRequired)?
        .to_string();
    if subject.is_empty() {
        return Err(ServiceError::AuthenticationRequired);
    }
    Ok(Principal { subject })
}

fn peek_unverified_claims(token: &str) -> Option<serde_json::Value> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = URL_SAFE_NO_PAD.decode(parts[1].as_bytes()).ok()?;
    serde_json::from_slice(&payload).ok()
}
