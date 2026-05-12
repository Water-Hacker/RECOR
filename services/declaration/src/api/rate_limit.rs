//! Per-principal rate limiting for the public submit endpoints (OPS-1).
//!
//! Wraps `tower-governor` with a custom [`KeyExtractor`] that pulls the
//! authenticated [`Principal`] from request extensions (set by
//! [`crate::api::auth::auth_middleware`]). The principal — not the IP
//! address — is the right identity boundary here: multiple legitimate
//! declarants commonly share an IP behind NAT, and the principal is
//! exactly the entity whose budget we want to enforce.
//!
//! Disabled when `rate_limit_per_min == 0` — the safe default for
//! tests and local dev. Production deployments set both
//! `RATE_LIMIT_PER_MIN` and `RATE_LIMIT_BURST`. GET endpoints, health
//! probes, and internal HMAC endpoints are never rate-limited; this
//! layer is wired only into the two state-changing submit routes by
//! [`crate::api::rest::router`].
//!
//! On exhaustion the layer returns 429 Too Many Requests, a
//! `Retry-After` header in seconds, and the standard service error
//! envelope:
//!
//! ```json
//! {
//!   "error": {
//!     "kind": "rate_limited",
//!     "message": "...",
//!     "retry_after_seconds": 12
//!   }
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, Response, StatusCode};
use governor::middleware::NoOpMiddleware;
use serde_json::json;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};

use crate::api::auth::Principal;

/// Custom [`KeyExtractor`] that keys the limiter on the authenticated
/// principal's subject (set by `auth_middleware` into extensions).
///
/// If the principal is absent we return `UnableToExtractKey` — this is
/// fail-closed (D14): tower-governor will surface that as 500 and
/// refuse the request. In practice the layer only sits behind
/// `auth_middleware`, so a missing principal means a routing bug; we
/// want it loud, not silently let through.
#[derive(Debug, Clone, Default)]
pub struct PrincipalKeyExtractor;

impl KeyExtractor for PrincipalKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<Principal>()
            .map(|p| p.subject.clone())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Build the governor config for the submit endpoints, or `None` when
/// rate limiting is disabled.
///
/// `per_min` is requests-per-minute (sustained); `burst` is the token
/// bucket size. The governor library models replenishment as
/// `period_between_tokens = 60s / per_min`, with a bucket of size
/// `burst` — so a principal can spike up to `burst` requests
/// instantaneously, then is throttled to `per_min` requests / minute
/// thereafter.
pub fn build_governor_config(
    per_min: u32,
    burst: u32,
) -> Option<Arc<GovernorConfig<PrincipalKeyExtractor, NoOpMiddleware>>> {
    if per_min == 0 || burst == 0 {
        return None;
    }
    // Period between token replenishments, in milliseconds. We work
    // in milliseconds so per_min values that don't divide 60 evenly
    // still produce a sensible quota.
    let period_ms: u64 = (60_000_u64 / u64::from(per_min)).max(1);

    // `GovernorConfigBuilder::default()` is typed on `PeerIpKeyExtractor`;
    // `.key_extractor(...)` returns a new builder typed on our custom
    // extractor. We re-bind, then configure period+burst on the
    // re-typed builder. (The `key_extractor` method takes &mut self and
    // RETURNS the new builder by value — we don't chain through it.)
    let base = GovernorConfigBuilder::default();
    let mut builder = {
        let mut tmp = base;
        tmp.key_extractor(PrincipalKeyExtractor)
    };
    builder
        .period(Duration::from_millis(period_ms))
        .burst_size(burst);
    let cfg = builder.finish()?;
    Some(Arc::new(cfg))
}

/// Construct the [`GovernorLayer`] from a previously-built config.
///
/// Wires our custom error handler so 429 responses match the standard
/// service error envelope (`kind: "rate_limited"`, `retry_after_seconds`)
/// and always carry a `Retry-After` header in seconds. Other governor
/// errors (e.g. `UnableToExtractKey`) map to 500 with the same
/// envelope shape.
pub fn governor_layer(
    config: Arc<GovernorConfig<PrincipalKeyExtractor, NoOpMiddleware>>,
) -> GovernorLayer<PrincipalKeyExtractor, NoOpMiddleware, Body> {
    GovernorLayer::new(config).error_handler(map_governor_error)
}

/// Map [`GovernorError`] → uniform service-style JSON response.
fn map_governor_error(err: GovernorError) -> Response<Body> {
    match err {
        GovernorError::TooManyRequests { wait_time, headers } => {
            // Governor reports `wait_time` in seconds; it may come back
            // as 0 when the bucket has just-now refilled by exactly
            // one token. Clamp to ≥1 so clients don't busy-loop. (D14:
            // fail-closed against confusing 0 values.)
            let retry_after = wait_time.max(1);
            let body = json!({
                "error": {
                    "kind": "rate_limited",
                    "message": format!(
                        "rate limit exceeded; retry in {retry_after}s"
                    ),
                    "retry_after_seconds": retry_after,
                }
            });
            let mut response = json_response(StatusCode::TOO_MANY_REQUESTS, &body);
            if let Some(extra) = headers {
                for (name, value) in &extra {
                    // Skip the limiter-supplied `retry-after`/
                    // `x-ratelimit-after`: they carry the raw
                    // (possibly-0) `wait_time`. We override below
                    // with the clamped value so headers and body
                    // agree.
                    let name_str = name.as_str();
                    if name_str == "retry-after" || name_str == "x-ratelimit-after" {
                        continue;
                    }
                    response.headers_mut().insert(name.clone(), value.clone());
                }
            }
            if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                response.headers_mut().insert("retry-after", v.clone());
                response.headers_mut().insert("x-ratelimit-after", v);
            }
            response
        }
        GovernorError::UnableToExtractKey => {
            // Fail-closed (D14): we couldn't identify the principal.
            // This should never reach a real client — the limiter sits
            // behind auth_middleware. Log via the response only; the
            // governor layer doesn't give us a tracing context here.
            let body = json!({
                "error": {
                    "kind": "internal",
                    "message": "internal failure",
                }
            });
            json_response(StatusCode::INTERNAL_SERVER_ERROR, &body)
        }
        GovernorError::Other { code, msg, headers } => {
            let body = json!({
                "error": {
                    "kind": "rate_limited",
                    "message": msg.unwrap_or_else(|| "rate limit error".to_string()),
                }
            });
            let mut response = json_response(code, &body);
            if let Some(extra) = headers {
                for (name, value) in &extra {
                    response.headers_mut().insert(name.clone(), value.clone());
                }
            }
            response
        }
    }
}

fn json_response(status: StatusCode, body: &serde_json::Value) -> Response<Body> {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

// Touch HeaderMap to silence the unused-import lint when the file is
// included but the type isn't directly named at the top level.
#[allow(dead_code)]
fn _force_headermap(_h: HeaderMap) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::{Principal, PrincipalSource};
    use axum::http::Request;

    fn make_request_with_principal(subject: &str) -> Request<()> {
        let mut req = Request::builder().uri("/v1/declarations").body(()).unwrap();
        req.extensions_mut().insert(Principal {
            subject: subject.to_string(),
            source: PrincipalSource::DevHeader,
        });
        req
    }

    #[test]
    fn key_extractor_returns_principal_subject() {
        let extractor = PrincipalKeyExtractor;
        let req = make_request_with_principal("spiffe://recor.cm/alice");
        let key = extractor.extract(&req).expect("principal present");
        assert_eq!(key, "spiffe://recor.cm/alice");
    }

    #[test]
    fn key_extractor_distinguishes_two_principals() {
        let extractor = PrincipalKeyExtractor;
        let alice = extractor
            .extract(&make_request_with_principal("spiffe://recor.cm/alice"))
            .unwrap();
        let bob = extractor
            .extract(&make_request_with_principal("spiffe://recor.cm/bob"))
            .unwrap();
        assert_ne!(alice, bob, "different principals MUST produce different keys");
    }

    #[test]
    fn key_extractor_fails_closed_without_principal() {
        let extractor = PrincipalKeyExtractor;
        let req: Request<()> = Request::builder().uri("/v1/declarations").body(()).unwrap();
        let err = extractor.extract(&req).expect_err("no principal in extensions");
        assert!(matches!(err, GovernorError::UnableToExtractKey));
    }

    #[test]
    fn build_governor_config_disabled_when_per_min_zero() {
        assert!(build_governor_config(0, 10).is_none());
    }

    #[test]
    fn build_governor_config_disabled_when_burst_zero() {
        assert!(build_governor_config(60, 0).is_none());
    }

    #[test]
    fn build_governor_config_produces_config_for_valid_inputs() {
        assert!(build_governor_config(60, 10).is_some());
        assert!(build_governor_config(1, 1).is_some());
        assert!(build_governor_config(u32::MAX, 1).is_some());
    }

    #[tokio::test]
    async fn map_governor_error_429_carries_standard_envelope_and_retry_after() {
        use http_body_util::BodyExt;
        let err = GovernorError::TooManyRequests {
            wait_time: 12,
            headers: None,
        };
        let response = map_governor_error(err);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry = response
            .headers()
            .get("retry-after")
            .expect("retry-after present")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(retry, "12");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );

        // Body shape: { error: { kind: rate_limited, retry_after_seconds: 12, ... } }
        let (_parts, body) = response.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["error"]["kind"], json!("rate_limited"));
        assert_eq!(parsed["error"]["retry_after_seconds"], json!(12));
        assert!(parsed["error"]["message"].is_string());
    }

    #[test]
    fn map_governor_error_429_clamps_zero_wait_to_one_second() {
        // wait_time can momentarily come back as 0 from governor when
        // the bucket has just refilled by exactly one token. The
        // Retry-After header must still suggest a reasonable wait so
        // clients don't busy-loop. (D14: fail-closed against confusing
        // 0 values.)
        let err = GovernorError::TooManyRequests {
            wait_time: 0,
            headers: None,
        };
        let response = map_governor_error(err);
        let retry = response.headers().get("retry-after").unwrap();
        assert_eq!(retry.to_str().unwrap(), "1");
    }

    #[test]
    fn map_governor_error_unable_to_extract_key_returns_500() {
        let response = map_governor_error(GovernorError::UnableToExtractKey);
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
