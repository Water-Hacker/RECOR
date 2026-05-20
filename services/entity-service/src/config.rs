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

    // ─── FIND-018 (audit Sprint 3) / R-LOOP-3 ───────────────────────
    /// One of `"hmac"` (default), `"mtls"`, `"mtls-only"`.
    #[serde(default = "default_auth_transport")]
    pub auth_transport: String,
    /// SPIFFE Workload API socket path. Unused when
    /// `auth_transport == "hmac"`.
    #[serde(default = "default_spiffe_socket")]
    pub spiffe_socket: String,
    /// This service's own SPIFFE ID. Defaults to
    /// `spiffe://recor.cm/entity`.
    #[serde(default = "default_spiffe_id_self_entity")]
    pub spiffe_id_self: String,
    /// FIND-018 / OPS-4 placeholder for the inbound-internal HMAC
    /// secret. Empty ⇒ future endpoint disabled at startup
    /// (D14 fail-closed).
    #[serde(default = "default_secret")]
    pub internal_hmac_secret: SecretString,
    /// FIND-018 / ADR-005: previous-generation internal HMAC secret
    /// accepted during a rotation window. Empty ⇒ no rotation in
    /// progress.
    #[serde(default = "default_secret")]
    pub internal_hmac_secret_old: SecretString,

    /// COMP-2 — outbox retention worker: rows in `outbox` whose
    /// `dispatched_at` is older than this are pruned by the retention
    /// worker. `0` DISABLES pruning entirely (the safe default).
    /// NEVER touches `entity_events` (immutable event log — see
    /// migration 0001).
    #[serde(default)]
    pub outbox_retention_days: u64,

    /// COMP-2 — outbox retention worker: interval between prune cycles,
    /// in seconds. Default 86400 (daily). Ignored when
    /// `outbox_retention_days == 0`.
    #[serde(default = "default_outbox_retention_interval")]
    pub outbox_retention_interval_seconds: u64,
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
        match cfg.auth_transport.as_str() {
            "hmac" | "mtls" | "mtls-only" => {}
            other => {
                return Err(ConfigError::InvalidAuthTransport(other.to_string()));
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

    pub fn admin_principals_list(&self) -> Vec<String> {
        self.admin_principals
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// FIND-018: true iff this service should bring up SPIFFE/mTLS
    /// at startup.
    pub fn mtls_enabled(&self) -> bool {
        matches!(self.auth_transport.as_str(), "mtls" | "mtls-only")
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
    #[error("AUTH_TRANSPORT must be one of: hmac, mtls, mtls-only (got `{0}`)")]
    InvalidAuthTransport(String),
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
fn default_auth_transport() -> String {
    "hmac".to_string()
}
fn default_spiffe_socket() -> String {
    "unix:///run/spire/agent.sock".to_string()
}
fn default_spiffe_id_self_entity() -> String {
    "spiffe://recor.cm/entity".to_string()
}
fn default_outbox_retention_interval() -> u64 {
    86_400 // 24 hours
}
