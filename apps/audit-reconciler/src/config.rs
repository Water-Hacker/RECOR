//! Environment-driven configuration.

use std::time::Duration;

use secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ReconcilerConfig {
    /// HTTP bind address for the operational surface (`/healthz`,
    /// `/readyz`, `/metrics`). Defaults to `0.0.0.0:8092`.
    pub bind_addr: String,
    /// Postgres URL pointing at the declaration service's database
    /// (read-only access is sufficient; the cron only SELECTs from
    /// `declaration_events`).
    pub database_url: SecretString,
    /// Fabric Gateway HTTP shim base URL.
    pub gateway_url: String,
    pub channel: String,
    pub chaincode: String,
    pub gateway_bearer_token: Option<SecretString>,
    /// Per-query timeout for the Fabric gateway.
    pub request_timeout: Duration,

    /// How often to run a full reconciliation pass. Default 600s.
    pub reconcile_interval: Duration,
    /// Don't consider an event "missing" until it has been in the
    /// event log for at least this long; covers normal bridge
    /// dispatch lag. Default 300s.
    pub grace_period: Duration,
    /// How far back to look on each pass. Default 24h. A scheduled
    /// daily pass covers 24h cleanly; shorter intervals overlap and
    /// re-cover events the previous run already checked (idempotent
    /// — divergences only emit metrics, no state mutation).
    pub lookback: Duration,
    /// Maximum number of distinct declaration_ids to reconcile in a
    /// single pass. Defaults to 1000. Acts as a back-pressure valve
    /// against runaway query fan-out if the bridge is broken AND a
    /// flood of events lands.
    pub max_declarations_per_run: i64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {message}")]
    Invalid { var: &'static str, message: String },
}

impl ReconcilerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: std::env::var("BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8092".into()),
            database_url: SecretString::new(
                std::env::var("DATABASE_URL")
                    .map_err(|_| ConfigError::Missing("DATABASE_URL"))?
                    .into(),
            ),
            gateway_url: std::env::var("FABRIC_GATEWAY_URL")
                .map_err(|_| ConfigError::Missing("FABRIC_GATEWAY_URL"))?,
            channel: std::env::var("FABRIC_CHANNEL")
                .unwrap_or_else(|_| "recor-audit".into()),
            chaincode: std::env::var("FABRIC_CHAINCODE")
                .unwrap_or_else(|_| "audit-witness".into()),
            gateway_bearer_token: std::env::var("FABRIC_GATEWAY_TOKEN")
                .ok()
                .map(|s| SecretString::new(s.into())),
            request_timeout: Duration::from_millis(parse_u64(
                "FABRIC_GATEWAY_QUERY_TIMEOUT_MS",
                10_000,
            )?),
            reconcile_interval: Duration::from_secs(parse_u64(
                "RECONCILE_INTERVAL_SECONDS",
                600,
            )?),
            grace_period: Duration::from_secs(parse_u64(
                "RECONCILE_GRACE_SECONDS",
                300,
            )?),
            lookback: Duration::from_secs(parse_u64(
                "RECONCILE_LOOKBACK_SECONDS",
                86_400,
            )?),
            max_declarations_per_run: parse_i64(
                "RECONCILE_MAX_DECLARATIONS_PER_RUN",
                1_000,
            )?,
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

fn parse_i64(var: &'static str, default: i64) -> Result<i64, ConfigError> {
    match std::env::var(var) {
        Ok(v) => v.parse::<i64>().map_err(|e| ConfigError::Invalid {
            var,
            message: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}
