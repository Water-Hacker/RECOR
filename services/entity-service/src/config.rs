//! Typed configuration. Loaded from environment variables (12-factor)
//! with sensible defaults for local development.

use std::time::Duration;

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Bind address for the HTTP server. Defaults to `0.0.0.0:8083` —
    /// 8080 is recor-declaration, 8081 is verification-engine, 8082 is
    /// reserved for the (planned) Person service, 8083 is Entity.
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    /// FIND-007 (audit Sprint 2): bind address for an OPTIONAL separate
    /// `/metrics` listener. When set, `/metrics` is removed from the
    /// main listener; a NetworkPolicy restricts ingress on this port to
    /// the Prometheus scraper. Empty (default) keeps `/metrics` on the
    /// main listener — single-port dev posture.
    #[serde(default)]
    pub metrics_bind_addr: String,

    /// Postgres connection string.
    pub database_url: SecretString,

    #[serde(default = "default_db_pool_size")]
    pub db_pool_max_connections: u32,

    #[serde(default = "default_idempotency_ttl")]
    pub idempotency_ttl_seconds: i64,

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

    #[serde(default)]
    pub oidc_audience: String,

    #[serde(default = "default_subject_claim")]
    pub oidc_subject_claim: String,

    #[serde(default = "default_http_timeout")]
    pub http_timeout_seconds: u64,

    /// Comma-separated list of principals authorised to call admin
    /// endpoints (currently: `POST /v1/entities/{id}/dissolve`). Empty
    /// disables admin endpoints (they return 503). Authenticated
    /// principals not in this list get 403. D17 — zero-trust admin gate.
    #[serde(default)]
    pub admin_principals: String,

    /// OPS-2 redaction posture.
    #[serde(default)]
    pub log_redaction: String,

    /// OPS-2 redaction key (64-hex / 32 bytes). REQUIRED outside dev.
    #[serde(default = "default_secret")]
    pub log_redaction_key: SecretString,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let builder = config::Config::builder().add_source(
            config::Environment::default().try_parsing(true).separator("__"),
        );
        let cfg: Self = builder
            .build()
            .map_err(ConfigError::Build)?
            .try_deserialize()
            .map_err(ConfigError::Deserialise)?;
        if cfg.environment != "dev" && cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::OidcRequiredOutsideDev);
        }
        // FIND-003 (audit Sprint 0): refuse dev+oidc co-existence.
        if cfg.environment == "dev" && !cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::DevWithOidcIsIncoherent);
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
    #[error(
        "ENVIRONMENT=dev with a configured OIDC_ISSUER_URL is incoherent: \
         the dev-header backdoor would bypass OIDC verification. \
         See FIND-003 in docs/audit/10-findings.md."
    )]
    DevWithOidcIsIncoherent,
    #[error("OIDC_AUDIENCE is required when OIDC_ISSUER_URL is set")]
    OidcAudienceRequired,
}

fn default_bind_addr() -> String {
    "0.0.0.0:8083".to_string()
}
fn default_db_pool_size() -> u32 {
    10
}
fn default_idempotency_ttl() -> i64 {
    86_400
}
fn default_log_filter() -> String {
    "info,recor_entity_service=debug,sqlx=warn".to_string()
}
fn default_service_name() -> String {
    "recor-entity-service".to_string()
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
