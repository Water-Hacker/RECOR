//! Typed configuration.

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    pub database_url: SecretString,
    #[serde(default = "default_pool_size")]
    pub db_pool_max_connections: u32,
    #[serde(default)]
    pub otlp_endpoint: String,
    #[serde(default = "default_log_filter")]
    pub log_filter: String,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default)]
    pub oidc_issuer_url: String,
    #[serde(default = "default_http_timeout")]
    pub http_timeout_seconds: u64,

    /// HMAC-SHA256 secret shared with the Declaration service's outbox
    /// relay. Required for the /v1/internal/declaration-events endpoint
    /// to verify inbound webhook signatures. Empty disables the
    /// internal endpoint (rejects every request with 503).
    #[serde(default = "default_secret")]
    pub inbound_hmac_secret: SecretString,

    /// URL of the Declaration service's /v1/internal/verification-outcomes
    /// endpoint. The outbox-relay POSTs verification.completed events
    /// here. Empty disables the relay (rows accumulate undispatched).
    #[serde(default)]
    pub writeback_url: String,

    /// HMAC-SHA256 secret shared with the Declaration service for
    /// signing outbound writeback envelopes. Distinct from
    /// `inbound_hmac_secret`: the Declaration service has its own
    /// secret for the writeback channel.
    #[serde(default = "default_secret")]
    pub writeback_hmac_secret: SecretString,

    /// Outbox-relay poll interval in seconds. Defaults to 5s.
    #[serde(default = "default_writeback_poll_interval")]
    pub writeback_poll_interval_seconds: u64,

    /// Maximum number of dispatch attempts before a row is abandoned.
    /// Defaults to 12. Each failure records `last_error` and bumps
    /// `dispatch_attempts`; rows at or above this threshold are skipped.
    #[serde(default = "default_writeback_max_attempts")]
    pub writeback_max_attempts: i32,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let cfg: Self = config::Config::builder()
            .add_source(config::Environment::default().try_parsing(true).separator("__"))
            .build()
            .map_err(ConfigError::Build)?
            .try_deserialize()
            .map_err(ConfigError::Deserialise)?;
        if cfg.environment != "dev" && cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::OidcRequiredOutsideDev);
        }
        Ok(cfg)
    }
    pub fn is_dev(&self) -> bool { self.environment == "dev" }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config build failure: {0}")]
    Build(#[source] config::ConfigError),
    #[error("config deserialise failure: {0}")]
    Deserialise(#[source] config::ConfigError),
    #[error("OIDC_ISSUER_URL is required outside dev")]
    OidcRequiredOutsideDev,
}

fn default_bind_addr() -> String { "0.0.0.0:8081".to_string() }
fn default_pool_size() -> u32 { 10 }
fn default_log_filter() -> String { "info,recor_verification_engine=debug,sqlx=warn".to_string() }
fn default_service_name() -> String { "recor-verification-engine".to_string() }
fn default_environment() -> String { "dev".to_string() }
fn default_http_timeout() -> u64 { 30 }
fn default_secret() -> SecretString { SecretString::from(String::new()) }
fn default_writeback_poll_interval() -> u64 { 5 }
fn default_writeback_max_attempts() -> i32 { 12 }
