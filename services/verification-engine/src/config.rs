//! Typed configuration.

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    /// FIND-007 (audit Sprint 2): bind address for an OPTIONAL separate
    /// metrics listener. When set, `/metrics` is removed from the main
    /// listener and mounted exclusively on this listener; a NetworkPolicy
    /// in `infrastructure/networks/` restricts ingress on this port to
    /// the Prometheus scraper's pod CIDR. When empty (default),
    /// `/metrics` stays on the main listener — backwards-compatible for
    /// dev / single-port deployments. Operators MUST set this in any
    /// deployment where the main listener is reachable from outside the
    /// cluster (D17 zero trust at the network boundary).
    #[serde(default)]
    pub metrics_bind_addr: String,
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
    /// OIDC audience claim — required whenever `oidc_issuer_url` is set.
    #[serde(default)]
    pub oidc_audience: String,
    /// Name of the JWT claim that becomes the Principal's subject.
    /// Defaults to `"sub"`. (R-AUTH-2)
    #[serde(default = "default_subject_claim")]
    pub oidc_subject_claim: String,
    #[serde(default = "default_http_timeout")]
    pub http_timeout_seconds: u64,

    /// HMAC-SHA256 secret shared with the Declaration service's outbox
    /// relay. Required for the /v1/internal/declaration-events endpoint
    /// to verify inbound webhook signatures. Empty disables the
    /// internal endpoint (rejects every request with 503).
    #[serde(default = "default_secret")]
    pub inbound_hmac_secret: SecretString,

    /// "Still-valid old" inbound secret accepted during a rotation
    /// window (R-LOOP-4-ROT). Empty means rotation not in progress.
    #[serde(default = "default_secret")]
    pub inbound_hmac_secret_old: SecretString,

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

    /// Comma-separated list of principals authorised to call admin
    /// endpoints (currently: the V-engine DLQ list + replay endpoints
    /// under /v1/internal/verification-outbox-dlq). Empty disables the
    /// admin endpoints entirely (they return 503). Authenticated
    /// principals not in this list get 403.
    ///
    /// In dev, the principal is taken from the X-Recor-Dev-Principal
    /// header; in production from the verified OIDC sub claim. Either
    /// way, the principal string is compared exactly to entries in
    /// this list. (R-LOOP-DLQ-3)
    #[serde(default)]
    pub admin_principals: String,

    /// PII-redaction posture for tracing logs (OPS-2). One of:
    ///   - `enabled` — full redaction (production default)
    ///   - `disabled-for-dev` — pass-through (dev default)
    ///   - `disabled` — explicit pass-through; warns at startup
    ///
    /// Empty string falls back to `enabled` in non-dev environments,
    /// `disabled-for-dev` in dev.
    #[serde(default)]
    pub log_redaction: String,

    /// COMP-2 — verification outbox retention worker: retention
    /// window in days. Rows in `verification_outbox` whose
    /// `dispatched_at` is older than this are pruned by the retention
    /// worker. `0` DISABLES pruning entirely and is the safe default
    /// for tests. The worker NEVER touches `verification_outbox_dlq`
    /// (forensic surface) or `verification_cases` (append-only — see
    /// migration 0003).
    #[serde(default)]
    pub outbox_retention_days: u64,

    /// COMP-2 — verification outbox retention worker: interval
    /// between prune cycles, in seconds. Default 86400 (daily).
    #[serde(default = "default_outbox_retention_interval")]
    pub outbox_retention_interval_seconds: u64,

    /// 64-hex-char (32-byte) BLAKE3 keyed-MAC key for redaction.
    /// REQUIRED in non-dev environments when redaction is enabled.
    /// Dev falls back to a random per-restart key with a startup warn.
    #[serde(default = "default_secret")]
    pub log_redaction_key: SecretString,

    // ─── R-LOOP-2: Kafka transport (alongside HTTP webhook) ──────────
    /// Comma-separated list of Kafka broker addresses, e.g.
    /// `kafka:29092`. Empty disables the Kafka consumer entirely —
    /// the safe default for tests. When non-empty AND
    /// `verification_transport == "kafka"`, the boot wiring spawns a
    /// Kafka consumer alongside (or instead of) the HTTP webhook on
    /// `POST /v1/internal/declaration-events`.
    #[serde(default)]
    pub kafka_brokers: String,

    /// Kafka consumer group id. Multiple V-engine replicas in the
    /// same group split partitions; multiple groups (e.g.
    /// `recor-v-engine-prod` vs `recor-v-engine-replay`) consume
    /// independently. Defaults to `recor-verification-engine`.
    #[serde(default = "default_consumer_group")]
    pub kafka_consumer_group: String,

    /// Kafka topic to consume from. Defaults to
    /// `recor.declaration.events.v1` — must match the producer side.
    #[serde(default = "default_declaration_topic")]
    pub kafka_declaration_topic: String,

    /// Transport selector for the inbound declaration-events path.
    /// One of `"http"` (default — preserves current behaviour) or
    /// `"kafka"` (spawn the consumer). Both can be active during
    /// the cutover; the V-engine's idempotency-on-event-id absorbs
    /// duplicates. See `docs/adr/0007-kafka-transport-cutover.md`.
    #[serde(default = "default_verification_transport")]
    pub verification_transport: String,

    /// R-LOOP-3 — service-to-service auth transport. One of:
    ///
    /// - `"hmac"` (default): HMAC-SHA256 path on `/v1/internal/*`.
    /// - `"mtls"`: rustls-terminated mTLS via SPIFFE SVID; HMAC
    ///   header is still required as defence-in-depth during cutover.
    /// - `"mtls-only"`: mTLS-only steady state, HMAC dropped.
    ///
    /// See `docs/adr/0008-spiffe-mtls.md` for the design and
    /// `docs/runbooks/spiffe-onboarding.md` for operational
    /// procedures.
    #[serde(default = "default_auth_transport")]
    pub auth_transport: String,

    /// R-LOOP-3 — SPIFFE Workload API socket. Used only when
    /// `auth_transport != "hmac"`.
    #[serde(default = "default_spiffe_socket")]
    pub spiffe_socket: String,

    /// R-LOOP-3 — this service's own SPIFFE ID. Defaults to
    /// `spiffe://recor.cm/verification`.
    #[serde(default = "default_spiffe_id_self_verification")]
    pub spiffe_id_self: String,

    /// R-LOOP-3 — the SPIFFE ID expected from inbound peers on
    /// `/v1/internal/declaration-events`. Defaults to
    /// `spiffe://recor.cm/declaration` (the declaration service is
    /// the only legitimate caller of the inbound surface).
    #[serde(default = "default_spiffe_id_peer_declaration")]
    pub spiffe_id_peer: String,

    // ─── FIND-009: real-stage activation flags ──────────────────────
    //
    // Each flag swaps a stub stage for its real implementation:
    //
    //   - sanctions     (Stage 3) → PostgresSanctionsAdapter + BunecNameResolver
    //   - PEP           (Stage 4) → PostgresPepAdapter + BunecNameResolver
    //   - adverse media (Stage 5) → PostgresIcijRepository + InferenceGateway
    //   - patterns      (Stage 6) → in-process structural checks against the pool
    //
    // Default is `false` for every flag — operators must opt in once
    // the upstream data sources are populated. The audit calls this
    // out under FIND-009: the real implementations have shipped since
    // R-VER-2..6 but were unreachable because the wiring registered
    // the stubs unconditionally.
    //
    // Stage 7 (cross-source) has no real implementation yet; the stub
    // stays in place until a separate ticket lands it.
    /// Activate the real Stage 3 (sanctions screening) instead of the
    /// stub. Requires the `sanctions_persons` table to be populated.
    #[serde(default)]
    pub enable_real_sanctions: bool,
    /// Activate the real Stage 4 (PEP screening) instead of the stub.
    /// Requires the `peps` table to be populated.
    #[serde(default)]
    pub enable_real_pep: bool,
    /// Activate the real Stage 5 (adverse media). Requires the
    /// `icij_persons` table AND a configured Anthropic API key
    /// (`ANTHROPIC_API_KEY`). When the API key is unset, the real
    /// gateway runs in fixture mode (see `recor-inference-gateway`).
    #[serde(default)]
    pub enable_real_adverse_media: bool,
    /// Activate the real Stage 6 (pattern detection) instead of the
    /// stub.
    #[serde(default)]
    pub enable_real_patterns: bool,
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
        // FIND-003 (audit Sprint 0): refuse dev+oidc co-existence. See
        // services/declaration/src/config.rs for the rationale and the
        // audit finding.
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
    pub fn is_dev(&self) -> bool { self.environment == "dev" }

    /// True iff this service should bring up SPIFFE/mTLS at startup.
    pub fn mtls_enabled(&self) -> bool {
        matches!(self.auth_transport.as_str(), "mtls" | "mtls-only")
    }

    /// True iff the inbound internal endpoint still requires the
    /// HMAC header. `hmac` + `mtls` both require it (defence in
    /// depth during cutover); only `mtls-only` drops it.
    pub fn hmac_required(&self) -> bool {
        !matches!(self.auth_transport.as_str(), "mtls-only")
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
    #[error("config build failure: {0}")]
    Build(#[source] config::ConfigError),
    #[error("config deserialise failure: {0}")]
    Deserialise(#[source] config::ConfigError),
    #[error("OIDC_ISSUER_URL is required outside dev")]
    OidcRequiredOutsideDev,
    #[error(
        "ENVIRONMENT=dev with a configured OIDC_ISSUER_URL is incoherent: \
         the dev-header backdoor would bypass OIDC verification entirely. \
         See FIND-003 in docs/audit/10-findings.md."
    )]
    DevWithOidcIsIncoherent,
    #[error("OIDC_AUDIENCE is required when OIDC_ISSUER_URL is set")]
    OidcAudienceRequired,
    #[error("AUTH_TRANSPORT must be one of: hmac, mtls, mtls-only (got `{0}`)")]
    InvalidAuthTransport(String),
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
fn default_subject_claim() -> String { "sub".to_string() }
fn default_outbox_retention_interval() -> u64 { 86_400 }
fn default_consumer_group() -> String { "recor-verification-engine".to_string() }
fn default_declaration_topic() -> String { "recor.declaration.events.v1".to_string() }
fn default_verification_transport() -> String { "http".to_string() }

// R-LOOP-3.
fn default_auth_transport() -> String {
    "hmac".to_string()
}
fn default_spiffe_socket() -> String {
    recor_spiffe::DEFAULT_WORKLOAD_API_SOCKET.to_string()
}
fn default_spiffe_id_self_verification() -> String {
    "spiffe://recor.cm/verification".to_string()
}
fn default_spiffe_id_peer_declaration() -> String {
    "spiffe://recor.cm/declaration".to_string()
}
