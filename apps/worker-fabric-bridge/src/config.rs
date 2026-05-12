//! Environment-driven configuration for the worker.

use std::time::Duration;

use secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Where to bind the HTTP receiver + healthz + metrics surface.
    pub bind_addr: String,
    /// Postgres URL for the DLQ table.
    pub database_url: SecretString,
    /// Fabric Gateway HTTP shim base URL.
    pub gateway_url: String,
    /// Fabric channel + chaincode names.
    pub channel: String,
    pub chaincode: String,
    /// Shared HMAC secret the relay uses; the receiver verifies the
    /// `X-RECOR-Signature` header against this. The worker refuses to
    /// start without it (D18: no secrets in code, but the *requirement*
    /// is enforced here).
    pub hmac_secret: SecretString,
    /// Bridge tuning.
    pub max_attempts: u32,
    pub request_timeout: Duration,
    pub backoff_base: Duration,
    /// Optional bearer token passed to the gateway.
    pub gateway_bearer_token: Option<SecretString>,
    /// Transport switch — when "kafka", the worker additionally starts
    /// a Kafka consumer (deferred to R-LOOP-2; this skeleton only ever
    /// observes "http" today and logs a warning on "kafka").
    pub transport: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {message}")]
    Invalid { var: &'static str, message: String },
}

impl WorkerConfig {
    /// Build from environment variables. Required:
    /// - `DATABASE_URL`
    /// - `FABRIC_GATEWAY_URL`
    /// - `RECOR_FABRIC_BRIDGE_HMAC` (shared with the declaration service)
    ///
    /// Optional with defaults:
    /// - `BIND_ADDR` (default `0.0.0.0:8090`)
    /// - `FABRIC_CHANNEL` (default `recor-audit`)
    /// - `FABRIC_CHAINCODE` (default `audit-witness`)
    /// - `FABRIC_BRIDGE_MAX_ATTEMPTS` (default 5)
    /// - `FABRIC_BRIDGE_REQUEST_TIMEOUT_MS` (default 10000)
    /// - `FABRIC_BRIDGE_BACKOFF_BASE_MS` (default 500)
    /// - `FABRIC_GATEWAY_TOKEN`
    /// - `FABRIC_BRIDGE_TRANSPORT` (default `http`)
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8090".to_string()),
            database_url: SecretString::new(
                std::env::var("DATABASE_URL")
                    .map_err(|_| ConfigError::Missing("DATABASE_URL"))?
                    .into(),
            ),
            gateway_url: std::env::var("FABRIC_GATEWAY_URL")
                .map_err(|_| ConfigError::Missing("FABRIC_GATEWAY_URL"))?,
            channel: std::env::var("FABRIC_CHANNEL")
                .unwrap_or_else(|_| "recor-audit".to_string()),
            chaincode: std::env::var("FABRIC_CHAINCODE")
                .unwrap_or_else(|_| "audit-witness".to_string()),
            hmac_secret: SecretString::new(
                std::env::var("RECOR_FABRIC_BRIDGE_HMAC")
                    .map_err(|_| ConfigError::Missing("RECOR_FABRIC_BRIDGE_HMAC"))?
                    .into(),
            ),
            max_attempts: parse_u32("FABRIC_BRIDGE_MAX_ATTEMPTS", 5)?,
            request_timeout: Duration::from_millis(
                parse_u64("FABRIC_BRIDGE_REQUEST_TIMEOUT_MS", 10_000)?,
            ),
            backoff_base: Duration::from_millis(
                parse_u64("FABRIC_BRIDGE_BACKOFF_BASE_MS", 500)?,
            ),
            gateway_bearer_token: std::env::var("FABRIC_GATEWAY_TOKEN")
                .ok()
                .map(|s| SecretString::new(s.into())),
            transport: std::env::var("FABRIC_BRIDGE_TRANSPORT")
                .unwrap_or_else(|_| "http".to_string()),
        })
    }
}

fn parse_u32(var: &'static str, default: u32) -> Result<u32, ConfigError> {
    match std::env::var(var) {
        Ok(v) => v.parse::<u32>().map_err(|e| ConfigError::Invalid {
            var,
            message: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}

fn parse_u64(var: &'static str, default: u64) -> Result<u64, ConfigError> {
    match std::env::var(var) {
        Ok(v) => v.parse::<u64>().map_err(|e| ConfigError::Invalid {
            var,
            message: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}
