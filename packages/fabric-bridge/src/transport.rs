//! Transport layer for the Fabric Bridge.
//!
//! Defines the `Transport` trait so the bridge can be tested without a
//! live Fabric Gateway, and provides an HTTP implementation that talks
//! to the Gateway shim. The shim's wire contract is intentionally
//! lightweight:
//!
//! ```text
//! POST {gateway_url}/v1/transactions/{channel}/{chaincode}
//! Content-Type: application/json
//! Authorization: Bearer {token}  (if configured)
//! {
//!   "method": "PutAuditEntry",
//!   "args": ["evt-uuid", "decl-uuid", "hash...", "ts", "att-hex"]
//! }
//!
//! 200 OK
//! { "tx_id": "abc123...", "already_committed": false }
//!
//! 200 OK (idempotent replay)
//! { "tx_id": "abc123...", "already_committed": true }
//!
//! 4xx (non-retryable, e.g., invalid args)
//! { "error": "receipt_hash_hex must be 64 chars" }
//!
//! 5xx / network failure (retryable)
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BridgeConfig, ChaincodeRequest};

/// Transport-layer errors. The bridge distinguishes retryable from
/// non-retryable based on this taxonomy.
#[derive(Debug, Error)]
pub enum TransportError {
    /// 5xx, timeout, or connection failure. The bridge will retry.
    #[error("retryable transport error: {0}")]
    Retryable(String),
    /// 4xx (other than already-committed) — the gateway considers the
    /// request malformed. Retries are pointless.
    #[error("non-retryable transport error: {0}")]
    NonRetryable(String),
}

/// Successful transport response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    /// Fabric transaction id, if the gateway has one to return.
    pub tx_id: Option<String>,
    /// True if the chaincode reported the entry already existed.
    pub already_committed: bool,
}

#[async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    async fn submit_transaction(
        &self,
        channel: &str,
        chaincode: &str,
        request: &ChaincodeRequest,
    ) -> Result<TransportResponse, TransportError>;
}

#[derive(Debug, Deserialize)]
struct GatewayResponseBody {
    tx_id: Option<String>,
    #[serde(default)]
    already_committed: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct GatewayRequestBody<'a> {
    method: &'a str,
    args: &'a [String],
}

/// HTTP transport over the Fabric Gateway shim.
pub struct HttpTransport {
    client: reqwest::Client,
    base_url: String,
    bearer_token: Option<String>,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .field("bearer_token", &self.bearer_token.as_ref().map(|_| "***"))
            .finish()
    }
}

impl HttpTransport {
    pub fn new(config: &BridgeConfig) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()?;
        Ok(Self {
            client,
            base_url: config.gateway_url.trim_end_matches('/').to_string(),
            bearer_token: config.bearer_token.clone(),
        })
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn submit_transaction(
        &self,
        channel: &str,
        chaincode: &str,
        request: &ChaincodeRequest,
    ) -> Result<TransportResponse, TransportError> {
        let url = format!(
            "{}/v1/transactions/{}/{}",
            self.base_url, channel, chaincode
        );
        let body = GatewayRequestBody {
            method: &request.method,
            args: &request.args,
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(token) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                TransportError::Retryable(format!("network: {e}"))
            } else {
                TransportError::Retryable(format!("send: {e}"))
            }
        })?;

        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| {
            TransportError::Retryable(format!("body read: {e}"))
        })?;

        // The shim should always return JSON, but some upstream proxies
        // return text/html on 5xx — accept a missing body as "we don't
        // know what happened, but the status code does the talking".
        let parsed: GatewayResponseBody =
            serde_json::from_slice(&bytes).unwrap_or(GatewayResponseBody {
                tx_id: None,
                already_committed: false,
                error: Some(format!("non-json body, len={}", bytes.len())),
            });

        if status.is_success() {
            if let Some(err) = parsed.error {
                // The shim returned 2xx but an error body. Treat as
                // non-retryable because the gateway clearly didn't
                // submit and isn't asking us to retry.
                return Err(TransportError::NonRetryable(err));
            }
            return Ok(TransportResponse {
                tx_id: parsed.tx_id,
                already_committed: parsed.already_committed,
            });
        }

        // 4xx: non-retryable, with one exception — 409 Conflict is the
        // shim's signal for "already committed" (it surfaces the
        // chaincode's idempotency check at the HTTP layer for callers
        // that don't want to introspect the body).
        if status.as_u16() == 409 {
            return Ok(TransportResponse {
                tx_id: parsed.tx_id,
                already_committed: true,
            });
        }

        let msg = parsed
            .error
            .unwrap_or_else(|| format!("http {}", status.as_u16()));
        if status.is_client_error() {
            Err(TransportError::NonRetryable(format!(
                "{} {}",
                status.as_u16(),
                msg
            )))
        } else {
            Err(TransportError::Retryable(format!(
                "{} {}",
                status.as_u16(),
                msg
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::{BridgeConfig, FabricBridge};

    const VALID_HASH: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn config(server: &MockServer) -> BridgeConfig {
        BridgeConfig {
            gateway_url: server.uri(),
            channel: "recor-audit".into(),
            chaincode: "audit-witness".into(),
            max_attempts: 3,
            backoff_base: Duration::from_millis(1),
            request_timeout: Duration::from_secs(5),
            bearer_token: None,
        }
    }

    #[tokio::test]
    async fn http_transport_commits_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/transactions/recor-audit/audit-witness"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tx_id": "tx-abc",
                "already_committed": false,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let bridge = FabricBridge::new(config(&server)).unwrap();
        let outcome = bridge
            .commit_audit_entry("evt", "decl", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();
        assert_eq!(outcome.tx_id().0, "tx-abc");
    }

    #[tokio::test]
    async fn http_transport_treats_409_as_idempotent_replay() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "tx_id": "tx-prior",
                "error": "already exists",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let bridge = FabricBridge::new(config(&server)).unwrap();
        let outcome = bridge
            .commit_audit_entry("evt", "decl", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();
        match outcome {
            crate::CommitOutcome::AlreadyCommitted(tx) => assert_eq!(tx.0, "tx-prior"),
            other => panic!("expected AlreadyCommitted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_transport_retries_on_5xx() {
        let server = MockServer::start().await;
        // First two responses 503; third 200.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tx_id": "tx-after-retry",
                "already_committed": false,
            })))
            .mount(&server)
            .await;

        let bridge = FabricBridge::new(config(&server)).unwrap();
        let outcome = bridge
            .commit_audit_entry("evt", "decl", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();
        assert_eq!(outcome.tx_id().0, "tx-after-retry");
    }

    #[tokio::test]
    async fn http_transport_returns_nonretryable_on_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "malformed args",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let bridge = FabricBridge::new(config(&server)).unwrap();
        let err = bridge
            .commit_audit_entry("evt", "decl", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap_err();
        assert!(matches!(err, crate::BridgeError::NonRetryable(_)));
    }

    #[tokio::test]
    async fn http_transport_authorisation_header_sent_when_configured() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wiremock::matchers::header("Authorization", "Bearer s3cret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tx_id": "tx-authed",
                "already_committed": false,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut cfg = config(&server);
        cfg.bearer_token = Some("s3cret".into());
        let bridge = FabricBridge::new(cfg).unwrap();
        let outcome = bridge
            .commit_audit_entry("evt", "decl", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();
        assert_eq!(outcome.tx_id().0, "tx-authed");
    }

    // Sanity: the transport trait is object-safe.
    #[allow(dead_code)]
    fn _trait_object(_: Arc<dyn Transport>) {}
}
