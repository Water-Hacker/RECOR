//! Typed configuration. Loaded from environment variables (12-factor)
//! with sensible defaults for local development.

use std::time::Duration;

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Bind address for the HTTP server, e.g. "0.0.0.0:8080".
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,

    /// Postgres connection string.
    pub database_url: SecretString,

    /// Maximum Postgres connection pool size.
    #[serde(default = "default_db_pool_size")]
    pub db_pool_max_connections: u32,

    /// Idempotency record TTL in seconds.
    #[serde(default = "default_idempotency_ttl")]
    pub idempotency_ttl_seconds: i64,

    /// OpenTelemetry OTLP endpoint, e.g. "http://otel-collector:4317".
    /// Empty string disables OTLP export and keeps tracing console-only.
    #[serde(default)]
    pub otlp_endpoint: String,

    /// Logging filter (RUST_LOG syntax), e.g. "info,recor_declaration=debug".
    #[serde(default = "default_log_filter")]
    pub log_filter: String,

    /// Service name reported in spans. Identifies this deployment in
    /// the observability stack.
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Deployment environment ("dev", "staging", "prod"). Used as a
    /// resource attribute on spans and influences a few code paths
    /// (e.g. devmode-only static JWT key).
    #[serde(default = "default_environment")]
    pub environment: String,

    /// OIDC issuer URL for verifying Bearer tokens on protected
    /// endpoints. Empty disables JWT verification and falls back to
    /// development-mode static-key verification — ONLY acceptable when
    /// environment == "dev".
    #[serde(default)]
    pub oidc_issuer_url: String,

    /// OIDC audience claim — must match the `aud` claim on every
    /// accepted token. Required whenever `oidc_issuer_url` is set;
    /// the config layer enforces this.
    #[serde(default)]
    pub oidc_audience: String,

    /// Name of the JWT claim that becomes the Principal's subject.
    /// Defaults to `"sub"`. Some issuers prefer `"preferred_username"`,
    /// `"email"`, or a custom claim. The verifier refuses tokens that
    /// lack this claim (R-AUTH-2).
    #[serde(default = "default_subject_claim")]
    pub oidc_subject_claim: String,

    /// HTTP request timeout in seconds.
    #[serde(default = "default_http_timeout")]
    pub http_timeout_seconds: u64,

    /// Outbox relay: URL to POST declaration events to. Empty disables
    /// the relay (events stay in outbox; a future ticket relays them).
    #[serde(default)]
    pub relay_webhook_url: String,

    /// Outbox relay HMAC secret. REQUIRED when relay_webhook_url is non-empty.
    #[serde(default = "default_secret")]
    pub relay_hmac_secret: SecretString,

    /// Outbox relay poll interval in seconds.
    #[serde(default = "default_relay_poll_interval")]
    pub relay_poll_interval_seconds: u64,

    /// HMAC-SHA256 secret used to verify inbound writeback envelopes
    /// from the Verification Engine on POST
    /// /v1/internal/verification-outcomes. Empty disables the endpoint
    /// (returns 503).
    #[serde(default = "default_secret")]
    pub writeback_hmac_secret: SecretString,
}

impl Config {
    /// Load configuration from environment, optionally seeded by a
    /// .env file.
    pub fn from_env() -> Result<Self, ConfigError> {
        // Best-effort .env load; absent .env is not an error.
        let _ = dotenvy::dotenv();
        let builder = config::Config::builder().add_source(
            config::Environment::default()
                .try_parsing(true)
                .separator("__"),
        );
        let cfg: Self = builder
            .build()
            .map_err(ConfigError::Build)?
            .try_deserialize()
            .map_err(ConfigError::Deserialise)?;

        // Cross-field validation.
        if cfg.environment != "dev" && cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::OidcRequiredOutsideDev);
        }
        if !cfg.oidc_issuer_url.is_empty() && cfg.oidc_audience.is_empty() {
            return Err(ConfigError::OidcAudienceRequired);
        }
        if !cfg.relay_webhook_url.is_empty() {
            use secrecy::ExposeSecret;
            if cfg.relay_hmac_secret.expose_secret().is_empty() {
                return Err(ConfigError::RelaySecretRequired);
            }
        }
        Ok(cfg)
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_seconds)
    }

    pub fn is_dev(&self) -> bool {
        self.environment == "dev"
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration build failure: {0}")]
    Build(#[source] config::ConfigError),
    #[error("configuration deserialise failure: {0}")]
    Deserialise(#[source] config::ConfigError),
    #[error("OIDC_ISSUER_URL is required outside dev")]
    OidcRequiredOutsideDev,
    #[error("OIDC_AUDIENCE is required when OIDC_ISSUER_URL is set")]
    OidcAudienceRequired,
    #[error("RELAY_HMAC_SECRET is required when RELAY_WEBHOOK_URL is set")]
    RelaySecretRequired,
}

fn default_bind_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_db_pool_size() -> u32 {
    10
}

fn default_idempotency_ttl() -> i64 {
    86_400 // 24 hours
}

fn default_log_filter() -> String {
    "info,recor_declaration=debug,sqlx=warn".to_string()
}

fn default_service_name() -> String {
    "recor-declaration".to_string()
}

fn default_environment() -> String {
    "dev".to_string()
}

fn default_http_timeout() -> u64 {
    10
}

fn default_relay_poll_interval() -> u64 {
    5
}

fn default_secret() -> SecretString {
    SecretString::from(String::new())
}

fn default_subject_claim() -> String {
    "sub".to_string()
}
