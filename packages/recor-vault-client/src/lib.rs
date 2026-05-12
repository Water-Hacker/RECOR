//! `recor-vault-client` — minimal HashiCorp Vault client for RÉCOR
//! services (OPS-4).
//!
//! ## Scope
//!
//! Just enough Vault surface to:
//!
//!   1. Authenticate via the AppRole method.
//!   2. Read KV-v2 secrets from the `secret/recor/<service>/...` tree.
//!   3. Populate the service's existing typed `Config` by mutating the
//!      `SecretString` fields the env loader left empty.
//!
//! Everything else (lease renewal, dynamic secrets, transit, PKI) is
//! out of scope for the skeleton. Production hardening (response
//! wrapping, token renewal, structured retry policy) is tracked as
//! follow-ups against this crate; the public API is designed so those
//! features land additively.
//!
//! ## Why a hand-rolled HTTP layer instead of `vaultrs`
//!
//! The RÉCOR services need just two endpoints: `POST /v1/auth/approle/login`
//! and `GET /v1/<mount>/data/<path>`. Using `reqwest` directly keeps
//! the dependency footprint small, makes the fail-closed semantics
//! explicit in this file (no upstream library quirks to inherit), and
//! lets us stub the HTTP surface in tests with `wiremock` — which is
//! the same harness recor-auth-oidc uses for JWKS. D7 (no workarounds):
//! we use the simplest primitive that closes the requirement.
//!
//! ## D14 fail-closed
//!
//! Every error path on a non-empty `VAULT_ADDR` returns
//! `VaultError::*`. The caller's contract is: if `VAULT_ADDR` is set,
//! `populate_from_vault` must succeed, or the service refuses to
//! start. There is no implicit env fallback when Vault is requested
//! and unreachable.

use std::collections::HashMap;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

/// Default HTTP timeout for Vault API calls.
///
/// Short enough that a stuck Vault doesn't silently extend startup
/// time; long enough to absorb a transient network blip.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// All errors this crate surfaces. Every variant is a
/// service-refuses-to-start condition by contract (D14).
#[derive(Debug, Error)]
pub enum VaultError {
    #[error("VAULT_ADDR is empty; cannot construct a VaultClient (caller should fall back to env-only mode)")]
    AddrUnset,

    #[error("HTTP request to Vault failed: {0}")]
    Http(#[source] reqwest::Error),

    #[error("Vault returned HTTP {status} for {endpoint}: {body}")]
    HttpStatus {
        status: u16,
        endpoint: String,
        body: String,
    },

    #[error("Vault response could not be parsed as JSON: {0}")]
    Decode(#[source] serde_json::Error),

    #[error("AppRole login returned no client_token; check role_id/secret_id and policy bindings")]
    LoginNoToken,

    #[error("KV-v2 secret at {path} is missing required key {key}")]
    MissingKey { path: String, key: String },

    #[error("invalid URL: {0}")]
    Url(#[source] reqwest::Error),
}

/// AppRole credentials. The two strings together are the bootstrap
/// secret RÉCOR explicitly accepts (D18): the only env-borne secret
/// in the production deployment. They are scoped to one role and
/// short-lived (24h secret-id TTL by default — see
/// `infrastructure/vault/scripts/init-dev-vault.sh`).
#[derive(Clone)]
pub struct AppRole {
    pub role_id: SecretString,
    pub secret_id: SecretString,
}

/// Vault client. Holds an authenticated session (the AppRole
/// `client_token`) and uses it for every subsequent KV read.
///
/// Construction = AppRole login. A `VaultClient` you hold is one
/// that has already authenticated successfully. Failure to log in is
/// surfaced at `new()` so callers can fail-closed at startup.
pub struct VaultClient {
    base_url: String,
    http: reqwest::Client,
    token: SecretString,
}

impl std::fmt::Debug for VaultClient {
    /// D18: never include `token` in the Debug output, even by
    /// accident through a derived Debug. Hand-rolled to keep the
    /// surface tight if new fields are added later.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl VaultClient {
    /// Construct from an address + AppRole credentials. Performs the
    /// login round-trip; returns the authenticated client on success.
    ///
    /// `addr` is the full Vault URL, e.g. `http://127.0.0.1:8200`.
    /// Empty addr is an error — callers that want optional Vault
    /// integration should check `addr.is_empty()` themselves and skip
    /// the constructor.
    #[instrument(skip(role), fields(addr = %addr))]
    pub async fn new(addr: &str, role: AppRole) -> Result<Self, VaultError> {
        if addr.is_empty() {
            return Err(VaultError::AddrUnset);
        }
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(VaultError::Http)?;
        let base_url = addr.trim_end_matches('/').to_string();
        let token = approle_login(&http, &base_url, &role).await?;
        info!("Vault AppRole login succeeded");
        Ok(Self {
            base_url,
            http,
            token,
        })
    }

    /// Read a KV-v2 secret at `mount/data/path`, returning the map of
    /// keys to string values that Vault stored under `data.data`.
    ///
    /// `mount` is the secret engine mount, typically `secret`.
    /// `path` is the path *inside* the mount, e.g. `recor/declaration/database`.
    #[instrument(skip(self), fields(mount, path))]
    pub async fn read_kv2(
        &self,
        mount: &str,
        path: &str,
    ) -> Result<HashMap<String, String>, VaultError> {
        let endpoint = format!("{}/v1/{}/data/{}", self.base_url, mount, path);
        let resp = self
            .http
            .get(&endpoint)
            .header("X-Vault-Token", self.token.expose_secret())
            .send()
            .await
            .map_err(VaultError::Http)?;
        let status = resp.status();
        let body = resp.text().await.map_err(VaultError::Http)?;
        if !status.is_success() {
            return Err(VaultError::HttpStatus {
                status: status.as_u16(),
                endpoint,
                body,
            });
        }
        let parsed: KvV2Response = serde_json::from_str(&body).map_err(VaultError::Decode)?;
        debug!(keys = parsed.data.data.len(), "KV-v2 read");
        Ok(parsed.data.data)
    }
}

#[derive(Deserialize)]
struct KvV2Response {
    data: KvV2Data,
}

#[derive(Deserialize)]
struct KvV2Data {
    data: HashMap<String, String>,
}

#[derive(Deserialize)]
struct LoginResponse {
    auth: LoginAuth,
}

#[derive(Deserialize)]
struct LoginAuth {
    client_token: String,
}

#[instrument(skip(http, role), fields(base_url))]
async fn approle_login(
    http: &reqwest::Client,
    base_url: &str,
    role: &AppRole,
) -> Result<SecretString, VaultError> {
    let endpoint = format!("{}/v1/auth/approle/login", base_url);
    let body = serde_json::json!({
        "role_id":   role.role_id.expose_secret(),
        "secret_id": role.secret_id.expose_secret(),
    });
    let resp = http
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .map_err(VaultError::Http)?;
    let status = resp.status();
    let text = resp.text().await.map_err(VaultError::Http)?;
    if !status.is_success() {
        return Err(VaultError::HttpStatus {
            status: status.as_u16(),
            endpoint,
            body: text,
        });
    }
    let parsed: LoginResponse = serde_json::from_str(&text).map_err(VaultError::Decode)?;
    if parsed.auth.client_token.is_empty() {
        return Err(VaultError::LoginNoToken);
    }
    Ok(SecretString::from(parsed.auth.client_token))
}

/// Bridge between the typed env-loaded `Config` and Vault.
///
/// Each call describes one Vault path and the assignments to make
/// once the secret has been fetched. The trait is implemented by the
/// per-service config crate (in this skeleton: the in-tree Config
/// types live in their respective services and call the bridge from
/// `main.rs`; the bridge itself is intentionally tiny so each service
/// owns the policy of which keys it cares about).
///
/// Concrete usage looks like:
///
/// ```ignore
/// let client = VaultClient::new(&addr, role).await?;
/// let kv = client.read_kv2("secret", "recor/declaration/database").await?;
/// cfg.database_url = require_secret(&kv, "DATABASE_URL", "recor/declaration/database")?;
/// ```
///
/// The helper below extracts the common "required key" pattern.
pub fn require_secret(
    kv: &HashMap<String, String>,
    key: &str,
    path: &str,
) -> Result<SecretString, VaultError> {
    kv.get(key)
        .map(|v| SecretString::from(v.clone()))
        .ok_or_else(|| VaultError::MissingKey {
            path: path.to_string(),
            key: key.to_string(),
        })
}

/// As `require_secret` but the key may be absent — returns an empty
/// `SecretString` in that case. Used for the rotation-window OLD
/// secrets where empty == rotation not in progress.
pub fn optional_secret(kv: &HashMap<String, String>, key: &str) -> SecretString {
    SecretString::from(kv.get(key).cloned().unwrap_or_default())
}

/// As `require_secret` but the value is *not* secret (e.g. an
/// `OIDC_ISSUER_URL`). Returns owned `String`. Empty means the key
/// was absent, matching the env-loader's default-empty semantics.
pub fn optional_value(kv: &HashMap<String, String>, key: &str) -> String {
    kv.get(key).cloned().unwrap_or_default()
}

/// Convenience: read AppRole credentials from env. Returns None when
/// either var is unset/empty so the caller can decide whether that's
/// fatal.
pub fn approle_from_env() -> Option<AppRole> {
    let role_id = std::env::var("VAULT_ROLE_ID").ok().filter(|s| !s.is_empty())?;
    let secret_id = std::env::var("VAULT_SECRET_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(AppRole {
        role_id: SecretString::from(role_id),
        secret_id: SecretString::from(secret_id),
    })
}

/// Convenience: read `VAULT_ADDR`, trimmed. Empty when unset.
pub fn vault_addr_from_env() -> String {
    std::env::var("VAULT_ADDR").unwrap_or_default().trim().to_string()
}

/// Wrapper that the service `main.rs` calls before `Config::from_env`.
/// When Vault is requested (`VAULT_ADDR` non-empty), populate the env
/// from Vault paths so the existing env-driven config loader sees the
/// secrets and refuses to start if any are missing — same fail-closed
/// path the env-only mode uses, no new validation surface.
///
/// `paths` is a list of (KV-v2 path, list of (Vault-key, env-var-name)).
/// Example:
/// ```ignore
/// load_into_env(&client, "secret", &[
///     ("recor/declaration/database", &[("DATABASE_URL", "DATABASE_URL")]),
///     ("recor/declaration/relay",    &[
///         ("RELAY_HMAC_SECRET",     "RELAY_HMAC_SECRET"),
///         ("RELAY_HMAC_SECRET_OLD", "RELAY_HMAC_SECRET_OLD"),
///     ]),
/// ]).await?;
/// ```
///
/// Why env-injection and not direct struct mutation: the existing
/// `Config::from_env()` validation lives in services/{declaration,
/// verification-engine}/src/config.rs and runs cross-field checks
/// (e.g. relay_hmac_secret required when relay_webhook_url is set).
/// Re-implementing those checks here would duplicate D7-violating
/// logic; instead, we feed env then call the existing loader. The
/// only "secret in env" at this point is in-process, never on disk,
/// and lives only until `Config::from_env()` consumes it.
#[instrument(skip(client, paths))]
pub async fn load_into_env(
    client: &VaultClient,
    mount: &str,
    paths: &[(&str, &[(&str, &str)])],
) -> Result<(), VaultError> {
    for (path, mappings) in paths {
        let kv = client.read_kv2(mount, path).await?;
        for (vault_key, env_var) in *mappings {
            match kv.get(*vault_key) {
                Some(v) if !v.is_empty() => {
                    // SAFETY: std::env::set_var is safe in a
                    // single-threaded startup phase. Vault loading
                    // runs before any worker spawns. If a service
                    // ever calls load_into_env after spawning, this
                    // must move to a typed-only bridge.
                    // SAFETY justification: see comment above.
                    unsafe {
                        std::env::set_var(env_var, v);
                    }
                }
                _ => {
                    debug!(env_var = %env_var, "Vault key absent or empty; leaving env untouched");
                }
            }
        }
    }
    info!(paths = paths.len(), "Vault secrets loaded into process env");
    Ok(())
}

/// Top-level bridge invoked from each service's `main.rs`. Reads
/// `VAULT_ADDR` from env; when empty, returns Ok(false) so the
/// caller knows to log the env-only fallback warning. When non-empty,
/// logs in via AppRole and loads the per-service secret paths into
/// env, returning Ok(true).
///
/// `paths` is service-specific — the caller passes the mapping from
/// Vault paths to env var names (see each service's main.rs).
#[instrument(skip(paths))]
pub async fn populate_from_vault(
    paths: &[(&str, &[(&str, &str)])],
) -> Result<bool, VaultError> {
    let addr = vault_addr_from_env();
    if addr.is_empty() {
        warn!(
            "VAULT_ADDR is empty — falling back to env-only secret loading. \
             Production deployments MUST set VAULT_ADDR (D18 / OPS-4)."
        );
        return Ok(false);
    }
    let role = approle_from_env().ok_or(VaultError::LoginNoToken)?;
    let client = VaultClient::new(&addr, role).await?;
    load_into_env(&client, "secret", paths).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn dev_approle() -> AppRole {
        AppRole {
            role_id: SecretString::from("test-role-id".to_string()),
            secret_id: SecretString::from("test-secret-id".to_string()),
        }
    }

    async fn mount_login_ok(server: &MockServer, token: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/auth/approle/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "auth": {
                    "client_token": token,
                    "lease_duration": 3600,
                }
            })))
            .mount(server)
            .await;
    }

    /// Test 1 — successful AppRole login produces an authenticated
    /// client. The login round-trip is the only effect of `new()`.
    #[tokio::test]
    async fn new_succeeds_when_login_returns_token() {
        let server = MockServer::start().await;
        mount_login_ok(&server, "s.test-token-abc").await;

        let client = VaultClient::new(&server.uri(), dev_approle())
            .await
            .expect("login should succeed");
        // Token is held opaquely; we can only assert by exposing it
        // in a controlled test context.
        assert_eq!(client.token.expose_secret(), "s.test-token-abc");
    }

    /// Test 2 — Vault returning 4xx on login is a fail-closed startup
    /// error. We never silently fall back.
    #[tokio::test]
    async fn new_fails_when_login_returns_403() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/auth/approle/login"))
            .respond_with(ResponseTemplate::new(403).set_body_string("permission denied"))
            .mount(&server)
            .await;

        let err = VaultClient::new(&server.uri(), dev_approle())
            .await
            .expect_err("403 must error");
        match err {
            VaultError::HttpStatus { status: 403, .. } => {}
            other => panic!("expected HttpStatus 403, got {:?}", other),
        }
    }

    /// Test 3 — login that returns 200 but no `client_token` is
    /// treated as fail-closed (defensive against a misconfigured
    /// role or a Vault bug).
    #[tokio::test]
    async fn new_fails_when_login_returns_empty_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/auth/approle/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "auth": {
                    "client_token": "",
                    "lease_duration": 3600,
                }
            })))
            .mount(&server)
            .await;

        let err = VaultClient::new(&server.uri(), dev_approle())
            .await
            .expect_err("empty token must error");
        assert!(matches!(err, VaultError::LoginNoToken));
    }

    /// Test 4 — KV-v2 read returns the inner data map. The wire format
    /// is `data.data` (the outer `data` is the envelope, the inner is
    /// the user's secret bundle).
    #[tokio::test]
    async fn read_kv2_returns_inner_data_map() {
        let server = MockServer::start().await;
        mount_login_ok(&server, "s.token").await;
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/recor/declaration/database"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "data": {
                        "DATABASE_URL": "postgres://recor:pw@db:5432/declaration",
                    },
                    "metadata": {
                        "version": 1,
                    }
                }
            })))
            .mount(&server)
            .await;

        let client = VaultClient::new(&server.uri(), dev_approle())
            .await
            .expect("login");
        let kv = client
            .read_kv2("secret", "recor/declaration/database")
            .await
            .expect("kv read");
        assert_eq!(
            kv.get("DATABASE_URL").map(String::as_str),
            Some("postgres://recor:pw@db:5432/declaration")
        );
    }

    /// Test 5 — KV-v2 404 (no such secret) is an error, not an empty
    /// map. Calling code should fail-closed on a missing required
    /// secret rather than silently substitute a default.
    #[tokio::test]
    async fn read_kv2_fails_on_404() {
        let server = MockServer::start().await;
        mount_login_ok(&server, "s.token").await;
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/recor/declaration/nope"))
            .respond_with(ResponseTemplate::new(404).set_body_string("{}"))
            .mount(&server)
            .await;

        let client = VaultClient::new(&server.uri(), dev_approle())
            .await
            .expect("login");
        let err = client
            .read_kv2("secret", "recor/declaration/nope")
            .await
            .expect_err("404 must error");
        match err {
            VaultError::HttpStatus { status: 404, .. } => {}
            other => panic!("expected HttpStatus 404, got {:?}", other),
        }
    }

    /// Test 6 — `require_secret` returns the SecretString when
    /// present, MissingKey when absent.
    #[test]
    fn require_secret_missing_returns_error() {
        let mut kv = HashMap::new();
        kv.insert("PRESENT".to_string(), "ok".to_string());

        let ok = require_secret(&kv, "PRESENT", "path").expect("present");
        assert_eq!(ok.expose_secret(), "ok");

        let err = require_secret(&kv, "ABSENT", "path").expect_err("absent");
        match err {
            VaultError::MissingKey { path, key } => {
                assert_eq!(path, "path");
                assert_eq!(key, "ABSENT");
            }
            other => panic!("expected MissingKey, got {:?}", other),
        }
    }

    /// Test 7 — `optional_secret` returns an empty SecretString for
    /// the absent case (rotation-window OLD secret pattern).
    #[test]
    fn optional_secret_absent_returns_empty() {
        let kv = HashMap::new();
        let s = optional_secret(&kv, "ABSENT");
        assert!(s.expose_secret().is_empty());
    }

    /// Test 8 — empty `VAULT_ADDR` is a `AddrUnset` error from
    /// `new()` (the caller should have checked addr first; the
    /// constructor is defensive about it).
    #[tokio::test]
    async fn new_rejects_empty_addr() {
        let err = VaultClient::new("", dev_approle())
            .await
            .expect_err("empty addr must error");
        assert!(matches!(err, VaultError::AddrUnset));
    }
}
