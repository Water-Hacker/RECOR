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
    /// Deployment environment ("dev" / "staging" / "prod"). Gates the
    /// `X-Recor-Dev-Principal` header backdoor in `auth.rs`.
    pub environment: String,
    /// OIDC issuer URL. Required outside dev. FIND-001 / FIND-003.
    pub oidc_issuer_url: String,
    /// OIDC audience claim — must match `aud` on every accepted token.
    pub oidc_audience: String,
    /// JWT claim that becomes the verified Principal's subject.
    pub oidc_subject_claim: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {message}")]
    Invalid { var: &'static str, message: String },
    #[error("OIDC_ISSUER_URL is required outside dev")]
    OidcRequiredOutsideDev,
    #[error(
        "ENVIRONMENT=dev with a configured OIDC_ISSUER_URL is incoherent: \
         the dev-header backdoor (X-Recor-Dev-Principal) is active in dev \
         mode and would allow bypassing OIDC verification entirely. \
         Either unset ENVIRONMENT (or set it to staging/prod) so the dev \
         backdoor is closed, or unset OIDC_ISSUER_URL to run a pure dev \
         stack. See FIND-003 in docs/audit/10-findings.md."
    )]
    DevWithOidcIsIncoherent,
    #[error("OIDC_AUDIENCE is required when OIDC_ISSUER_URL is set")]
    OidcAudienceRequired,
}

impl VerifierConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let cfg = Self {
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
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".into()),
            oidc_issuer_url: std::env::var("OIDC_ISSUER_URL").unwrap_or_default(),
            oidc_audience: std::env::var("OIDC_AUDIENCE").unwrap_or_default(),
            oidc_subject_claim: std::env::var("OIDC_SUBJECT_CLAIM")
                .unwrap_or_else(|_| "sub".into()),
        };

        // FIND-001 / FIND-003 cross-field validation.
        if cfg.environment != "dev" && cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::OidcRequiredOutsideDev);
        }
        if cfg.environment == "dev" && !cfg.oidc_issuer_url.is_empty() {
            return Err(ConfigError::DevWithOidcIsIncoherent);
        }
        if !cfg.oidc_issuer_url.is_empty() && cfg.oidc_audience.is_empty() {
            return Err(ConfigError::OidcAudienceRequired);
        }
        Ok(cfg)
    }

    pub fn is_dev(&self) -> bool {
        self.environment == "dev"
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
