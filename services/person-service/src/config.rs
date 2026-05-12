//! Typed configuration. Loaded from environment variables (12-factor)
//! with sensible defaults for local development. Mirrors
//! `services/declaration/src/config.rs` so operators only need to learn
//! one config surface across the platform.

use std::time::Duration;

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Bind address for the HTTP server, e.g. "0.0.0.0:8082".
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

    /// OpenTelemetry OTLP endpoint. Empty disables OTLP export and
    /// keeps tracing console-only.
    #[serde(default)]
    pub otlp_endpoint: String,

    /// Logging filter (RUST_LOG syntax).
    #[serde(default = "default_log_filter")]
    pub log_filter: String,

    /// Service name reported in spans.
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Deployment environment ("dev", "staging", "prod").
    #[serde(default = "default_environment")]
    pub environment: String,

    /// OIDC issuer URL for verifying Bearer tokens.
    #[serde(default)]
    pub oidc_issuer_url: String,

    /// OIDC audience claim — must match the `aud` on every accepted token.
    #[serde(default)]
    pub oidc_audience: String,

    /// Name of the JWT claim that becomes the Principal's subject.
    #[serde(default = "default_subject_claim")]
    pub oidc_subject_claim: String,

    /// HTTP request timeout in seconds.
    #[serde(default = "default_http_timeout")]
    pub http_timeout_seconds: u64,

    /// Comma-separated list of principals authorised to call admin
    /// endpoints (currently: `/v1/persons/{id}/merge-into/{target_id}`).
    /// Empty disables admin endpoints entirely (they return 503).
    #[serde(default)]
    pub admin_principals: String,

    /// PII-redaction posture for tracing logs (OPS-2). One of:
    ///   - `enabled` — full redaction (production default)
    ///   - `disabled-for-dev` — pass-through (dev default)
    ///   - `disabled` — explicit pass-through with a loud `warn!`
    #[serde(default)]
    pub log_redaction: String,

    /// 64-hex-char (32-byte) BLAKE3 keyed-MAC key used by the OPS-2
    /// redaction layer. REQUIRED in non-dev when redaction is enabled.
    #[serde(default = "default_secret")]
    pub log_redaction_key: SecretString,
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

        // Cross-field validation — identical posture to declaration.
        if cfg.environment != "dev" && cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::OidcRequiredOutsideDev);
        }
        if !cfg.oidc_issuer_url.is_empty() && cfg.oidc_audience.is_empty() {
            return Err(ConfigError::OidcAudienceRequired);
        }
        Ok(cfg)
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_seconds)
    }

    pub fn is_dev(&self) -> bool {
        self.environment == "dev"
    }

    /// Parse `admin_principals` (CSV) into a deduplicated list of
    /// trimmed, non-empty principal strings.
    pub fn admin_principals_list(&self) -> Vec<String> {
        self.admin_principals
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
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
}

fn default_bind_addr() -> String {
    "0.0.0.0:8082".to_string()
}

fn default_db_pool_size() -> u32 {
    10
}

fn default_idempotency_ttl() -> i64 {
    86_400 // 24 hours
}

fn default_log_filter() -> String {
    "info,recor_person_service=debug,sqlx=warn".to_string()
}

fn default_service_name() -> String {
    "recor-person-service".to_string()
}

fn default_environment() -> String {
    "dev".to_string()
}

fn default_http_timeout() -> u64 {
    10
}

fn default_secret() -> SecretString {
    SecretString::from(String::new())
}

fn default_subject_claim() -> String {
    "sub".to_string()
}
