//! RÉCOR Inference Gateway — the workspace's only Anthropic-Messages
//! client (Doctrine 22: Anthropic-primary AI inference).
//!
//! Responsibilities:
//!   * Speak the Anthropic Messages API via `reqwest`.
//!   * Force structured output via tool-use: callers declare a JSON
//!     schema; the gateway turns it into a single Anthropic tool, sets
//!     `tool_choice = {type: tool, name: <name>}`, and parses the
//!     `tool_use` block. We refuse non-tool-use responses.
//!   * Pin model identifiers: callers pass a `Tier`; the gateway maps
//!     to the wire-level model string. v1 mapping:
//!       - Tier A → `claude-opus-4-7` (default for adverse-media)
//!       - Tier B → `claude-haiku-4-5-20251001`
//!   * Track per-case token usage so the caller can attribute spend.
//!   * Fixture mode: when `ANTHROPIC_API_KEY` is empty, return a
//!     deterministic vacuous response so tests don't need network.
//!     This is D14 fail-closed at the integration boundary: a missing
//!     key never crashes the pipeline; it returns "I don't know."
//!
//! The gateway is intentionally narrow — one method, `messages` — and
//! has zero dependency on any callsite. Stage 5 wraps it with the
//! adverse-media prompt template and the ICIJ retrieval shape.

#![deny(unsafe_code)]
#![warn(clippy::all)]

pub mod budget;
pub mod fixture;
pub mod model;
pub mod prompt;

pub use budget::TokenBudget;
pub use fixture::FixtureResponse;
pub use model::Tier;
pub use prompt::{ToolSchema, StructuredResponse};

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Configuration for the inference gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub api_key: SecretString,
    pub base_url: String,
    pub default_tier: Tier,
    pub request_timeout: Duration,
    /// Soft per-process token ceiling. The gateway records usage but
    /// does NOT refuse calls when over budget (refusal is a follow-up
    /// concern — for v1 we surface usage as a metric and let humans
    /// page).
    pub session_token_ceiling: Option<u64>,
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map(SecretString::from)
            .unwrap_or_else(|_| SecretString::from(String::new()));
        let base_url = std::env::var("ANTHROPIC_API_URL")
            .unwrap_or_else(|_| ANTHROPIC_API_URL.to_string());
        Self {
            api_key,
            base_url,
            default_tier: Tier::A,
            request_timeout: Duration::from_secs(30),
            session_token_ceiling: None,
        }
    }

    pub fn is_fixture_mode(&self) -> bool {
        self.api_key.expose_secret().is_empty()
    }
}

/// The gateway. Holds a `reqwest::Client` + an in-process token-budget
/// counter. Clone-able (the inner state is `Arc`).
#[derive(Clone)]
pub struct InferenceGateway {
    client: Client,
    config: GatewayConfig,
    budget: Arc<TokenBudget>,
}

impl InferenceGateway {
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        let client = Client::builder()
            .default_headers(headers)
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| GatewayError::ClientInit(e.to_string()))?;
        let budget = Arc::new(TokenBudget::new(config.session_token_ceiling));
        Ok(Self { client, config, budget })
    }

    pub fn budget(&self) -> Arc<TokenBudget> {
        self.budget.clone()
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// Make a structured call.
    ///
    /// `purpose` is a short tag for budget attribution (e.g.
    /// "adverse_media", "pattern_explain"). `schema` declares the
    /// expected JSON shape via tool-use. `prompt` is the user-content
    /// prompt; `system` is the system prompt.
    ///
    /// Returns the parsed `StructuredResponse`. In fixture mode (no
    /// API key) returns the deterministic vacuous fixture so tests
    /// are reproducible.
    #[instrument(skip_all, fields(purpose = %purpose))]
    pub async fn messages(
        &self,
        purpose: &str,
        tier: Tier,
        system: &str,
        user_prompt: &str,
        schema: &ToolSchema,
    ) -> Result<StructuredResponse, GatewayError> {
        let model = tier.model_id();
        if self.config.is_fixture_mode() {
            info!("ANTHROPIC_API_KEY unset; returning fixture response (D14 fail-closed)");
            let fixture = FixtureResponse::vacuous(purpose, model);
            self.budget.record(purpose, model, 0);
            return Ok(fixture.into_structured(schema));
        }

        let body = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "system": system,
            "messages": [
                { "role": "user", "content": user_prompt }
            ],
            "tools": [{
                "name": schema.name,
                "description": schema.description,
                "input_schema": schema.json_schema,
            }],
            "tool_choice": { "type": "tool", "name": schema.name },
        });

        let resp = self
            .client
            .post(&self.config.base_url)
            .header(
                "x-api-key",
                self.config.api_key.expose_secret(),
            )
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            warn!(status = %status, body = %text, "anthropic API non-2xx");
            return Err(GatewayError::Upstream { status: status.as_u16(), body: text });
        }

        let parsed: AnthropicMessagesResponse = resp
            .json()
            .await
            .map_err(|e| GatewayError::Decode(e.to_string()))?;

        if let Some(usage) = &parsed.usage {
            let total = usage.input_tokens + usage.output_tokens;
            self.budget.record(purpose, model, total);
            debug!(
                input = usage.input_tokens,
                output = usage.output_tokens,
                "anthropic usage"
            );
        }

        // Find the tool-use block.
        let tool_block = parsed
            .content
            .iter()
            .find(|b| matches!(b, AnthropicContentBlock::ToolUse { .. }))
            .ok_or(GatewayError::NoToolUse)?;
        let input = match tool_block {
            AnthropicContentBlock::ToolUse { input, .. } => input.clone(),
            _ => unreachable!(),
        };
        // D17 zero trust: validate the input matches the schema's
        // top-level field names.
        prompt::validate_against_schema(schema, &input)?;

        Ok(StructuredResponse {
            tool_input: input,
            stop_reason: parsed.stop_reason.unwrap_or_default(),
            usage_input_tokens: parsed.usage.as_ref().map(|u| u.input_tokens),
            usage_output_tokens: parsed.usage.as_ref().map(|u| u.output_tokens),
            model: parsed.model,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    stop_reason: Option<String>,
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text {
        #[allow(dead_code)]
        text: String,
    },
    ToolUse {
        #[allow(dead_code)]
        id: String,
        #[allow(dead_code)]
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("HTTP client init failure: {0}")]
    ClientInit(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("upstream non-2xx ({status}): {body}")]
    Upstream { status: u16, body: String },
    #[error("decode: {0}")]
    Decode(String),
    #[error("response did not include a tool_use block")]
    NoToolUse,
    #[error("schema validation: {0}")]
    SchemaValidation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_mode_when_key_unset() {
        let cfg = GatewayConfig {
            api_key: SecretString::from(String::new()),
            base_url: ANTHROPIC_API_URL.to_string(),
            default_tier: Tier::A,
            request_timeout: Duration::from_secs(1),
            session_token_ceiling: None,
        };
        assert!(cfg.is_fixture_mode());
    }

    #[tokio::test]
    async fn fixture_mode_returns_deterministic_vacuous() {
        let cfg = GatewayConfig {
            api_key: SecretString::from(String::new()),
            base_url: ANTHROPIC_API_URL.to_string(),
            default_tier: Tier::A,
            request_timeout: Duration::from_secs(1),
            session_token_ceiling: None,
        };
        let gw = InferenceGateway::new(cfg).unwrap();
        let schema = ToolSchema {
            name: "adverse_media_verdict",
            description: "test",
            json_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "verdict": {"type": "string"},
                    "confidence": {"type": "number"},
                    "evidence_citations": {"type": "array"}
                },
                "required": ["verdict", "confidence", "evidence_citations"]
            }),
        };
        let r = gw
            .messages("adverse_media", Tier::A, "sys", "user", &schema)
            .await
            .unwrap();
        assert_eq!(r.tool_input["verdict"], "insufficient_evidence");
    }
}
