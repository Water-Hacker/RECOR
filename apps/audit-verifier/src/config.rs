//! Verifier configuration.

use std::time::Duration;

use secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct VerifierConfig {
    pub bind_addr: String,
    pub database_url: SecretString,
    pub gateway_url: String,
    pub channel: String,
    pub chaincode: String,
    pub request_timeout: Duration,
    pub gateway_bearer_token: Option<SecretString>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {message}")]
    Invalid { var: &'static str, message: String },
}

impl VerifierConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8091".into()),
            database_url: SecretString::new(
                std::env::var("DATABASE_URL")
                    .map_err(|_| ConfigError::Missing("DATABASE_URL"))?
                    .into(),
            ),
            gateway_url: std::env::var("FABRIC_GATEWAY_URL")
                .map_err(|_| ConfigError::Missing("FABRIC_GATEWAY_URL"))?,
            channel: std::env::var("FABRIC_CHANNEL").unwrap_or_else(|_| "recor-audit".into()),
            chaincode: std::env::var("FABRIC_CHAINCODE")
                .unwrap_or_else(|_| "audit-witness".into()),
            request_timeout: Duration::from_millis(parse_u64(
                "FABRIC_GATEWAY_QUERY_TIMEOUT_MS",
                10_000,
            )?),
            gateway_bearer_token: std::env::var("FABRIC_GATEWAY_TOKEN")
                .ok()
                .map(|s| SecretString::new(s.into())),
        })
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
