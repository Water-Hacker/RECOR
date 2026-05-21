//! `recor-cli` — operator command surface for the RÉCOR platform.
//!
//! Closes TODO-056 from the audit catalogue. The CLI is the
//! bootstrap-time companion to the platform services: it gives an
//! operator a single binary to reach the four canonical surfaces
//! (`/healthz`, the audit verifier, the v-engine sanctions adapter,
//! the DLQ admin endpoints) without juggling `curl` and bearer tokens.
//!
//! ## Configuration
//!
//! Two environment variables control the runtime; both have CLI flag
//! overrides:
//!
//! - `RECOR_API_BASE_URL` — base URL for the platform's public ingress.
//!   The CLI appends a service-specific path suffix (see
//!   [`Service::default_path`]) for each request. The base URL alone
//!   determines which environment the operator is hitting; this is
//!   deliberate so the same CLI invocation against prod vs. dev only
//!   differs by one env var.
//! - `RECOR_TOKEN` — bearer token for admin-gated calls (sanctions
//!   search + the DLQ surfaces). Anonymous calls (`health`, `verify`)
//!   do NOT require it.
//!
//! ## Doctrines that bear on this crate
//!
//! - **D14 fail-closed** — every non-2xx response is surfaced as an
//!   error, never coerced into a falsy success. Admin commands refuse
//!   to send anonymously rather than emitting a 401 and showing an
//!   empty body to the operator.
//! - **D17 zero trust** — the CLI never embeds a default token. The
//!   operator must provide one via `--token`, `RECOR_TOKEN`, or a
//!   helper that exports it from a credential broker.
//! - **D18 no secrets** — `RECOR_TOKEN` is treated as a sensitive
//!   value: it is consumed from env, used once, and never logged. The
//!   Debug impl on the client redacts it.

use std::time::Duration;

use anyhow::{anyhow, Context as _, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

pub mod command;

/// Named platform service the CLI knows how to address. New services
/// are added here, not by stringly-typed paths from the call site —
/// the CLI is a small, finite surface and growing it is a one-line
/// change against this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Declaration,
    VerificationEngine,
    Person,
    Entity,
    AuditVerifier,
}

impl Service {
    /// Parse the operator-facing token. Returns an error rather than
    /// defaulting because guessing the service is exactly the kind of
    /// silent failure D14 forbids.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "declaration" | "decl" => Ok(Self::Declaration),
            "verification-engine" | "v-engine" | "venge" => Ok(Self::VerificationEngine),
            "person" | "person-service" => Ok(Self::Person),
            "entity" | "entity-service" => Ok(Self::Entity),
            "audit-verifier" | "audit" => Ok(Self::AuditVerifier),
            other => Err(anyhow!(
                "unknown service '{other}' — expected one of: \
                 declaration, verification-engine, person, entity, audit-verifier"
            )),
        }
    }

    /// Default path under the base URL for this service. Composed
    /// with `client.base_url` to produce the full request URL.
    /// The values mirror the kubernetes Ingress hostname-paths in
    /// `infrastructure/helm/` — same suffixes used by every other
    /// caller so the CLI does not invent a parallel URL shape.
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Declaration => "/declaration",
            Self::VerificationEngine => "/verification-engine",
            Self::Person => "/person",
            Self::Entity => "/entity",
            Self::AuditVerifier => "/audit-verifier",
        }
    }

    /// Display name used in operator-facing prose (errors + logs).
    pub fn display(&self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::VerificationEngine => "verification-engine",
            Self::Person => "person",
            Self::Entity => "entity",
            Self::AuditVerifier => "audit-verifier",
        }
    }
}

/// Shared CLI configuration. Constructed once at command dispatch
/// time from env + flags, then passed by reference into the command
/// handlers.
#[derive(Clone)]
pub struct CliConfig {
    pub base_url: String,
    pub token: Option<String>,
    pub timeout: Duration,
}

impl std::fmt::Debug for CliConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliConfig")
            .field("base_url", &self.base_url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl CliConfig {
    pub fn builder() -> CliConfigBuilder {
        CliConfigBuilder::default()
    }
}

#[derive(Default, Debug, Clone)]
pub struct CliConfigBuilder {
    base_url: Option<String>,
    token: Option<String>,
    timeout: Option<Duration>,
}

impl CliConfigBuilder {
    pub fn base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = Some(base.into());
        self
    }
    pub fn token(mut self, t: Option<String>) -> Self {
        self.token = t;
        self
    }
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
    pub fn build(self) -> Result<CliConfig> {
        let base = self
            .base_url
            .ok_or_else(|| anyhow!("RECOR_API_BASE_URL is required (or pass --base-url)"))?;
        // Normalise trailing slash — every call site builds URLs with
        // a leading-slash service prefix, so the base must NOT end
        // with one. Catch the most common mistake at parse time.
        let base = base.trim_end_matches('/').to_string();
        if !base.starts_with("http://") && !base.starts_with("https://") {
            return Err(anyhow!(
                "RECOR_API_BASE_URL must start with http:// or https://, got '{base}'"
            ));
        }
        Ok(CliConfig {
            base_url: base,
            token: self.token,
            timeout: self.timeout.unwrap_or_else(|| Duration::from_secs(30)),
        })
    }
}

/// Build a `reqwest::Client` configured with the operator timeout.
/// Separated out so tests can inject a wiremock-served `base_url`
/// without rebuilding the whole binary.
pub fn http_client(cfg: &CliConfig) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(cfg.timeout)
        .build()
        .context("build reqwest client")
}

/// Compose a request URL from the base + service prefix + path
/// fragment. The fragment must begin with a `/`.
pub fn build_url(cfg: &CliConfig, svc: Service, path_suffix: &str) -> Result<String> {
    if !path_suffix.starts_with('/') {
        return Err(anyhow!(
            "internal: path_suffix must start with '/' (got '{path_suffix}')"
        ));
    }
    Ok(format!("{}{}{}", cfg.base_url, svc.prefix(), path_suffix))
}

/// Attach `Authorization: Bearer <token>` and JSON content-type to
/// the supplied headers. Returns an error if a token is required
/// but the operator did not supply one — admin commands fail-closed
/// rather than emitting an unauthenticated request the server will
/// 401.
pub fn auth_headers(cfg: &CliConfig, require_token: bool) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(token) = cfg.token.as_deref() {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("token contained an invalid header byte")?;
        headers.insert(AUTHORIZATION, value);
    } else if require_token {
        return Err(anyhow!(
            "this command requires an admin token; set RECOR_TOKEN or pass --token"
        ));
    }
    Ok(headers)
}

/// Health probe response. Every service exposes `/healthz` with the
/// shape `{"status":"ok"}` plus optional service-specific fields. The
/// CLI deserialises into a permissive map so we never reject a future
/// extension.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(flatten)]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_parse_canonical_names() {
        assert_eq!(Service::parse("declaration").unwrap(), Service::Declaration);
        assert_eq!(
            Service::parse("verification-engine").unwrap(),
            Service::VerificationEngine
        );
        assert_eq!(Service::parse("person").unwrap(), Service::Person);
        assert_eq!(Service::parse("entity").unwrap(), Service::Entity);
        assert_eq!(
            Service::parse("audit-verifier").unwrap(),
            Service::AuditVerifier
        );
    }

    #[test]
    fn service_parse_aliases() {
        // The aliases exist so an operator typing `recor-cli health
        // decl` doesn't have to remember the full service name. Each
        // alias must map to exactly one canonical service.
        assert_eq!(Service::parse("decl").unwrap(), Service::Declaration);
        assert_eq!(
            Service::parse("v-engine").unwrap(),
            Service::VerificationEngine
        );
        assert_eq!(Service::parse("audit").unwrap(), Service::AuditVerifier);
    }

    #[test]
    fn service_parse_rejects_unknown() {
        let err = Service::parse("nope").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown service"));
        // The error must list every canonical service name so the
        // operator can correct the typo without re-reading docs.
        for s in [
            "declaration",
            "verification-engine",
            "person",
            "entity",
            "audit-verifier",
        ] {
            assert!(msg.contains(s), "missing '{s}' in error: {msg}");
        }
    }

    #[test]
    fn cli_config_rejects_non_http_base() {
        let err = CliConfig::builder()
            .base_url("file:///etc/passwd")
            .build()
            .unwrap_err();
        assert!(format!("{err}").contains("http://"));
    }

    #[test]
    fn cli_config_strips_trailing_slash() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test/")
            .build()
            .unwrap();
        assert_eq!(cfg.base_url, "https://api.example.test");
    }

    #[test]
    fn cli_config_redacts_token_in_debug() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test")
            .token(Some("hunter2".into()))
            .build()
            .unwrap();
        let dbg = format!("{cfg:?}");
        assert!(
            !dbg.contains("hunter2"),
            "token leaked through Debug: {dbg}"
        );
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn build_url_composes_prefix_and_suffix() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test")
            .build()
            .unwrap();
        let url = build_url(&cfg, Service::Declaration, "/healthz").unwrap();
        assert_eq!(url, "https://api.example.test/declaration/healthz");
    }

    #[test]
    fn auth_headers_omits_authorization_when_not_required() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test")
            .build()
            .unwrap();
        let h = auth_headers(&cfg, false).unwrap();
        assert!(h.get(AUTHORIZATION).is_none());
    }

    #[test]
    fn auth_headers_fails_closed_when_token_required() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test")
            .build()
            .unwrap();
        let err = auth_headers(&cfg, true).unwrap_err();
        assert!(format!("{err}").contains("admin token"));
    }

    #[test]
    fn auth_headers_attaches_bearer_when_token_present() {
        let cfg = CliConfig::builder()
            .base_url("https://api.example.test")
            .token(Some("t0k3n".into()))
            .build()
            .unwrap();
        let h = auth_headers(&cfg, true).unwrap();
        let bearer = h.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert_eq!(bearer, "Bearer t0k3n");
    }
}
