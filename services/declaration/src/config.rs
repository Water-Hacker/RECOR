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

    /// "Still-valid old" secret accepted during a rotation window
    /// (R-LOOP-4-ROT). Empty means rotation not in progress. When
    /// set, the writeback endpoint accepts envelopes signed with
    /// EITHER `writeback_hmac_secret` or this value, enabling
    /// zero-downtime rotation. The operator clears it after the new
    /// secret has propagated through the signer side.
    #[serde(default = "default_secret")]
    pub writeback_hmac_secret_old: SecretString,

    /// Comma-separated list of principals authorised to call admin
    /// endpoints (currently: the DLQ list + replay endpoints under
    /// /v1/internal/outbox-dlq). Empty disables admin endpoints
    /// entirely (they return 503). Authenticated principals not in
    /// this list get 403.
    ///
    /// In dev, the principal is taken from the X-Recor-Dev-Principal
    /// header; in production from the verified OIDC sub claim. Either
    /// way, the principal string is compared exactly to entries in
    /// this list. (R-LOOP-DLQ-2)
    #[serde(default)]
    pub admin_principals: String,

    /// CSV of origins that may make cross-origin requests against the
    /// REST API. The declarant portal (typically served from a
    /// different origin than the service in dev / CI) needs an entry
    /// here so the browser can XHR submit. Empty disables CORS
    /// entirely — the production default when the portal proxies the
    /// API through its own nginx (same-origin from the browser's
    /// view).
    #[serde(default)]
    pub cors_allowed_origins: String,

    /// Per-principal sustained rate limit, expressed as
    /// requests-per-minute. Applied to the two state-changing public
    /// submit endpoints (POST /v1/declarations and POST
    /// /v1/declarations/{id}/supersede). 0 disables rate limiting
    /// entirely — the safe default for tests and local development,
    /// but production deployments should set this. GET endpoints and
    /// internal HMAC endpoints are never rate-limited. (OPS-1)
    #[serde(default = "default_rate_limit_per_min")]
    pub rate_limit_per_min: u32,

    /// Per-principal burst capacity for the rate limiter. The token
    /// bucket holds up to this many tokens; once exhausted the
    /// principal must wait for the bucket to refill at
    /// `rate_limit_per_min` per 60s. Ignored when
    /// `rate_limit_per_min == 0`. (OPS-1)
    #[serde(default = "default_rate_limit_burst")]
    pub rate_limit_burst: u32,

    /// PII-redaction posture for tracing logs (OPS-2). One of:
    ///   - `enabled` — full redaction (production default)
    ///   - `disabled-for-dev` — pass-through (dev default; lets
    ///     local debugging see raw values)
    ///   - `disabled` — explicit pass-through; emits a loud `warn!`
    ///     at startup so it can't quietly leak into production
    ///
    /// Empty string falls back to `enabled` in non-dev environments,
    /// `disabled-for-dev` in dev.
    #[serde(default)]
    pub log_redaction: String,

    /// 64-hex-char (32-byte) BLAKE3 keyed-MAC key used to redact
    /// SPIFFE URI paths, UUIDs in PII fields, and partial receipt
    /// hashes. REQUIRED in non-dev environments when redaction is
    /// enabled — `observability::init` refuses to start if missing.
    /// In dev the redaction layer falls back to a random key
    /// regenerated each restart (with a startup `warn!`).
    #[serde(default = "default_secret")]
    pub log_redaction_key: SecretString,

    /// COMP-2 — outbox retention worker: retention window in days.
    /// Rows in `outbox` whose `dispatched_at` is older than this are
    /// pruned by the retention worker. `0` DISABLES pruning entirely
    /// and is the safe default for tests (so test data is never
    /// silently dropped) and for any environment where the operator
    /// has not explicitly opted in. The retention worker NEVER touches
    /// `outbox_dlq` (forensic surface) or `declaration_events`
    /// (immutable event log — see migration 0007).
    #[serde(default)]
    pub outbox_retention_days: u64,

    /// COMP-2 — outbox retention worker: interval between prune
    /// cycles, in seconds. Default 86400 (daily). Ignored when
    /// `outbox_retention_days == 0`.
    #[serde(default = "default_outbox_retention_interval")]
    pub outbox_retention_interval_seconds: u64,

    /// Bind address for the gRPC server (R-DECL-8). Defaults to empty
    /// so test harnesses and local dev that only exercise REST do not
    /// need to bind a second port. Production sets
    /// `GRPC_BIND_ADDR=0.0.0.0:9080`. Empty disables the gRPC server
    /// entirely — the safe default for the existing test suite.
    ///
    /// The gRPC surface is defined in `contracts/declaration.proto`
    /// and implemented at `src/api/grpc.rs`. Same OIDC verifier as
    /// REST, wrapped in a tonic interceptor so D17 (zero trust) holds
    /// uniformly across transports.
    #[serde(default)]
    pub grpc_bind_addr: String,

    /// R-LOOP-3 — service-to-service auth transport. One of:
    ///
    /// - `"hmac"` (default): the existing HMAC-SHA256 path on
    ///   `/v1/internal/*`. No SPIFFE involvement; the V1 transport.
    /// - `"mtls"`: rustls-terminated mTLS via SPIFFE SVID; **HMAC
    ///   header is still required** as a defence-in-depth fallback
    ///   during the cutover window. Refusal at startup if the
    ///   SPIFFE Workload API is unreachable (D14 fail-closed).
    /// - `"mtls-only"`: rustls-terminated mTLS via SPIFFE SVID;
    ///   HMAC verification is dropped. This is the post-cutover
    ///   steady state.
    ///
    /// Empty defaults to `hmac` for backward compatibility with the
    /// existing integration-smoke + production rollout.
    ///
    /// See `docs/adr/0008-spiffe-mtls.md` for the design decision and
    /// `docs/runbooks/spiffe-onboarding.md` for operational
    /// procedures (registering a new workload, rotating the trust
    /// bundle, debugging SVID-fetch failures).
    #[serde(default = "default_auth_transport")]
    pub auth_transport: String,

    /// R-LOOP-3 — SPIFFE Workload API socket path. Used only when
    /// `auth_transport != "hmac"`. The socket lives on a tmpfs
    /// shared with the SPIRE agent container (see
    /// `infrastructure/spire/docker-compose.yaml`).
    #[serde(default = "default_spiffe_socket")]
    pub spiffe_socket: String,

    /// R-LOOP-3 — the SPIFFE ID this service expects from its own
    /// SVID. The Workload API may issue multiple SVIDs to a
    /// containerised workload; this config nails down which one we
    /// bind into the TLS stack. Defaults to
    /// `spiffe://recor.cm/declaration`.
    #[serde(default = "default_spiffe_id_self_declaration")]
    pub spiffe_id_self: String,

    /// R-LOOP-3 — the SPIFFE ID this service expects from inbound
    /// peers on `/v1/internal/verification-outcomes`. The
    /// verification engine is the only legitimate caller; we gate
    /// on its SPIFFE ID rather than IP/network. Defaults to
    /// `spiffe://recor.cm/verification`.
    #[serde(default = "default_spiffe_id_peer_verification")]
    pub spiffe_id_peer: String,
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
        // R-LOOP-3: validate auth_transport against the enum.
        match cfg.auth_transport.as_str() {
            "hmac" | "mtls" | "mtls-only" => {}
            other => {
                return Err(ConfigError::InvalidAuthTransport(other.to_string()));
            }
        }
        Ok(cfg)
    }

    /// True iff this service should bring up SPIFFE/mTLS at startup
    /// (i.e. `auth_transport` is `mtls` or `mtls-only`).
    pub fn mtls_enabled(&self) -> bool {
        matches!(self.auth_transport.as_str(), "mtls" | "mtls-only")
    }

    /// True iff this service still requires the HMAC header on
    /// inbound internal endpoints. `hmac` and `mtls` both keep the
    /// HMAC requirement; only `mtls-only` drops it.
    pub fn hmac_required(&self) -> bool {
        !matches!(self.auth_transport.as_str(), "mtls-only")
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_seconds)
    }

    pub fn is_dev(&self) -> bool {
        self.environment == "dev"
    }

    /// Parse `admin_principals` (CSV) into a deduplicated list of
    /// trimmed, non-empty principal strings. Returns an empty Vec
    /// when no admin principals are configured (admin endpoints
    /// then return 503).
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
    #[error("RELAY_HMAC_SECRET is required when RELAY_WEBHOOK_URL is set")]
    RelaySecretRequired,
    #[error("AUTH_TRANSPORT must be one of: hmac, mtls, mtls-only (got `{0}`)")]
    InvalidAuthTransport(String),
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

fn default_rate_limit_per_min() -> u32 {
    60
}

fn default_rate_limit_burst() -> u32 {
    10
}

fn default_outbox_retention_interval() -> u64 {
    86_400 // 24 hours
}

fn default_auth_transport() -> String {
    // R-LOOP-3: hmac is the v1 default; integration smokes opt in to
    // mtls / mtls-only explicitly so existing CI runs are unaffected.
    "hmac".to_string()
}

fn default_spiffe_socket() -> String {
    // Matches infrastructure/spire/agent.conf socket_path. Empty would
    // be a configuration error when mtls_enabled() returns true; the
    // service's main.rs checks that.
    recor_spiffe::DEFAULT_WORKLOAD_API_SOCKET.to_string()
}

fn default_spiffe_id_self_declaration() -> String {
    "spiffe://recor.cm/declaration".to_string()
}

fn default_spiffe_id_peer_verification() -> String {
    "spiffe://recor.cm/verification".to_string()
}
