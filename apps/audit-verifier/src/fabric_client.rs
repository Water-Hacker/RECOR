//! Read-side client for the audit-witness chaincode.
//!
//! Queries are issued over the Gateway shim's `/v1/queries/...` HTTP
//! endpoint (the read-side complement to the bridge's `/v1/transactions/...`).
//! Wire shape:
//!
//! ```text
//! POST {gateway_url}/v1/queries/{channel}/{chaincode}
//! { "method": "ListAuditEntriesForDeclaration", "args": ["decl-uuid"] }
//!
//! 200 OK
//! { "entries": [
//!     { "event_id": "...", "declaration_id": "...",
//!       "receipt_hash_hex": "...", "ts": "...",
//!       "tx_id": "..." }, ... ] }
//!
//! 5xx — retryable (callers return 503 to the user)
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum FabricClientError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("upstream error: {0}")]
    Upstream(String),
}

/// One audit entry as stored on the Fabric audit channel, plus the
/// transaction id that committed it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnChainEntry {
    pub event_id: String,
    pub declaration_id: String,
    pub receipt_hash_hex: String,
    pub ts: String,
    /// The Fabric transaction id. Useful for cross-referencing with
    /// the channel's block explorer if the operator runs one.
    pub tx_id: String,
}

#[async_trait]
pub trait FabricClient: Send + Sync + std::fmt::Debug {
    async fn list_for_declaration(
        &self,
        declaration_id: &str,
    ) -> Result<Vec<OnChainEntry>, FabricClientError>;
}

#[derive(Debug, Serialize)]
struct QueryBody<'a> {
    method: &'a str,
    args: &'a [String],
}

#[derive(Debug, Deserialize)]
struct QueryResponse {
    #[serde(default)]
    entries: Vec<OnChainEntry>,
    #[serde(default)]
    error: Option<String>,
}

pub struct HttpFabricClient {
    client: reqwest::Client,
    base_url: String,
    channel: String,
    chaincode: String,
    bearer_token: Option<String>,
}

impl std::fmt::Debug for HttpFabricClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpFabricClient")
            .field("base_url", &self.base_url)
            .field("channel", &self.channel)
            .field("chaincode", &self.chaincode)
            .field("bearer_token", &self.bearer_token.as_ref().map(|_| "***"))
            .finish()
    }
}

impl HttpFabricClient {
    pub fn new(
        gateway_url: &str,
        channel: &str,
        chaincode: &str,
        timeout: std::time::Duration,
        bearer_token: Option<String>,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            client,
            base_url: gateway_url.trim_end_matches('/').to_string(),
            channel: channel.to_string(),
            chaincode: chaincode.to_string(),
            bearer_token,
        })
    }
}

#[async_trait]
impl FabricClient for HttpFabricClient {
    async fn list_for_declaration(
        &self,
        declaration_id: &str,
    ) -> Result<Vec<OnChainEntry>, FabricClientError> {
        let url = format!(
            "{}/v1/queries/{}/{}",
            self.base_url, self.channel, self.chaincode
        );
        let body = QueryBody {
            method: "ListAuditEntriesForDeclaration",
            args: &[declaration_id.to_string()],
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(token) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| FabricClientError::Transport(e.to_string()))?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| FabricClientError::Transport(format!("body read: {e}")))?;

        let parsed: QueryResponse = serde_json::from_slice(&bytes)
            .map_err(|e| FabricClientError::Decode(e.to_string()))?;

        if !status.is_success() {
            warn!(status = %status, "gateway query non-2xx");
            return Err(FabricClientError::Upstream(
                parsed.error.unwrap_or_else(|| format!("http {}", status.as_u16())),
            ));
        }
        Ok(parsed.entries)
    }
}

/// In-memory stub used by tests and the local-dev mode.
#[derive(Debug, Default)]
pub struct InMemoryFabricClient {
    pub entries:
        tokio::sync::Mutex<std::collections::HashMap<String, Vec<OnChainEntry>>>,
    pub fail: tokio::sync::Mutex<bool>,
}

impl InMemoryFabricClient {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn add(&self, decl_id: &str, entry: OnChainEntry) {
        self.entries
            .lock()
            .await
            .entry(decl_id.to_string())
            .or_default()
            .push(entry);
    }
    pub async fn set_fail(&self, fail: bool) {
        *self.fail.lock().await = fail;
    }
}

#[async_trait]
impl FabricClient for InMemoryFabricClient {
    async fn list_for_declaration(
        &self,
        declaration_id: &str,
    ) -> Result<Vec<OnChainEntry>, FabricClientError> {
        if *self.fail.lock().await {
            return Err(FabricClientError::Transport("synthetic".to_string()));
        }
        Ok(self
            .entries
            .lock()
            .await
            .get(declaration_id)
            .cloned()
            .unwrap_or_default())
    }
}
