//! `recor-spiffe` — SPIFFE Workload API client + rustls glue for the
//! RÉCOR Declaration service and Verification engine (R-LOOP-3).
//!
//! ## What this crate gives you
//!
//! 1. **A `WorkloadApi` trait + transport-agnostic [`SpiffeClient`]**.
//!    The trait abstracts the act of "fetch my SVID + the trust bundle
//!    from the local SPIRE agent". Production wires a gRPC client
//!    against the SPIFFE Workload API at
//!    `unix:///tmp/spire-agent/public/api.sock`; tests plug a
//!    `wiremock`-backed HTTP stub or a hand-written fixture against the
//!    same trait. This is the same shape we used for OIDC discovery
//!    in `recor-auth-oidc`: the trait is the seam.
//! 2. **rustls glue**. [`SpiffeClient::server_config`] and
//!    [`SpiffeClient::client_config`] return `rustls::ServerConfig` and
//!    `rustls::ClientConfig` respectively, configured for **mutual
//!    authentication**: the local SVID is the certificate; the
//!    Workload API trust bundle is the CA store.
//! 3. **Peer identity extraction**. [`peer_spiffe_id_from_cert`] pulls
//!    the URI SAN out of a peer certificate. The tower middleware in
//!    [`mtls_middleware`] uses this to set
//!    `Extensions::insert(PeerSpiffeId(...))` so handlers can read it.
//! 4. **Allowlist gating**. [`enforce_peer_id`] returns the
//!    canonical "403 + error envelope" tuple when a verified peer
//!    SPIFFE ID does not match the expected workload identity. The
//!    declaration + verification services use this on the inbound
//!    HMAC/mTLS internal endpoints.
//! 5. **OBS-1 metrics**. Two counters —
//!    `recor_spiffe_svid_fetch_total{result}` and
//!    `recor_spiffe_peer_verify_total{result}` — registered against a
//!    Prometheus `Registry` passed at construction. Same shape as the
//!    OIDC counters; consumers wire the existing `Metrics` registry.
//!
//! ## Doctrines
//!
//! - **D7 / D14 fail-closed**. If the Workload API is unreachable at
//!   startup, [`SpiffeClient::bootstrap`] returns `Err`. The composition
//!   roots (`services/declaration/src/main.rs`,
//!   `services/verification-engine/src/main.rs`) propagate that error
//!   so the service refuses to start under `AUTH_TRANSPORT=mtls`.
//! - **D17 zero trust**. mTLS gives us a cryptographically verified
//!   peer identity at the transport boundary; the allowlist gate makes
//!   that identity an authorisation primitive (not just an
//!   authentication one).
//! - **D18 no secrets in logs**. The Workload API talker never logs
//!   the SVID private key. Errors carry the underlying cause without
//!   the PEM body.
//!
//! ## What this crate intentionally does NOT do
//!
//! - No gRPC. The production gRPC Workload-API client is wired in the
//!   service composition root (`bin/`-ish; or a follow-up cargo
//!   feature for tonic + tower-svc). The trait shape here means we
//!   can swap implementations without touching either service's
//!   `main.rs`.
//! - No JWT-SVID. RÉCOR uses X.509-SVIDs for transport authentication;
//!   the JWT-SVID surface is a follow-up for cross-cluster federation
//!   (out of scope for R-LOOP-3).
//! - No federation. Single trust domain `recor.cm` for v1; federation
//!   (multiple trust domains, foreign-domain bundle imports) is a
//!   v2 capability.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod metrics;
pub mod middleware;
pub mod rustls_glue;
pub mod workload_api;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures;

pub use metrics::SpiffeMetrics;
pub use middleware::{enforce_peer_id, extract_peer_spiffe_id, mtls_middleware, PeerSpiffeId};
pub use rustls_glue::peer_spiffe_id_from_cert;
pub use workload_api::{HttpWorkloadApi, WorkloadApi, X509SvidResponse};

/// The trust-domain authority. Workloads are addressed as
/// `spiffe://<trust_domain>/<workload_path>`.
pub const TRUST_DOMAIN: &str = "recor.cm";

/// Default SPIFFE Workload API socket path. Matches the
/// `agent.conf` `socket_path` in `infrastructure/spire/agent.conf`.
pub const DEFAULT_WORKLOAD_API_SOCKET: &str = "unix:///tmp/spire-agent/public/api.sock";

/// Result alias for crate operations.
pub type Result<T> = std::result::Result<T, SpiffeError>;

/// All errors this crate can return.
#[derive(Debug, Error)]
pub enum SpiffeError {
    /// The Workload API was unreachable at the configured socket.
    /// Production refuses to start when this fires under
    /// `AUTH_TRANSPORT=mtls` (D14 fail-closed).
    #[error("workload api unreachable: {0}")]
    WorkloadApiUnreachable(String),

    /// The Workload API returned a malformed SVID (missing certificate
    /// chain, malformed PEM, missing URI SAN, etc).
    #[error("workload api returned a malformed SVID: {0}")]
    MalformedSvid(String),

    /// The fetched SVID's SPIFFE ID did not match what the caller
    /// expected. This is the "I asked for spiffe://recor.cm/declaration
    /// and got spiffe://recor.cm/portal" case.
    #[error("expected SPIFFE ID `{expected}`; workload api issued `{actual}`")]
    SpiffeIdMismatch {
        /// The SPIFFE ID the caller asked for.
        expected: String,
        /// The SPIFFE ID the Workload API issued.
        actual: String,
    },

    /// rustls refused the SVID material (corrupted key, wrong key type,
    /// etc). Distinct from [`Self::MalformedSvid`] because the SVID
    /// parsed but rustls couldn't consume it.
    #[error("rustls configuration build failed: {0}")]
    RustlsConfig(String),

    /// A peer presented a certificate without a SPIFFE URI SAN, or with
    /// a SAN that did not parse as a SPIFFE ID.
    #[error("peer certificate did not carry a SPIFFE URI SAN: {0}")]
    PeerHasNoSpiffeId(String),

    /// A peer's SPIFFE ID did not appear in the configured allowlist.
    #[error("peer SPIFFE ID `{actual}` not in allowlist (expected `{expected}`)")]
    PeerNotAllowed {
        /// The SPIFFE ID the inbound peer presented.
        actual: String,
        /// The SPIFFE ID the caller required.
        expected: String,
    },
}

/// A pair of materials returned by the Workload API for the
/// **calling** workload: its own SVID (cert + key) and the trust
/// bundle (CA roots) used to verify peer SVIDs.
#[derive(Debug, Clone)]
pub struct SvidBundle {
    /// The X.509 SVID chain (leaf certificate first; intermediates
    /// follow). Empty leaf is impossible — the workload-api stub
    /// rejects empty chains in [`HttpWorkloadApi::fetch_svid`].
    pub chain_pem: Vec<u8>,
    /// The PKCS#8-encoded private key in PEM form.
    pub key_pem: Vec<u8>,
    /// The X.509 trust bundle in PEM form — the concatenation of
    /// every CA root the agent trusts in the local trust domain.
    pub trust_bundle_pem: Vec<u8>,
    /// The SPIFFE ID encoded in the SVID's URI SAN — convenience
    /// copy so callers don't have to re-parse the leaf certificate.
    pub spiffe_id: String,
}

impl SvidBundle {
    /// Parse the leaf chain into the rustls-typed
    /// [`CertificateDer`] vector.
    pub fn chain_der(&self) -> Result<Vec<CertificateDer<'static>>> {
        rustls_glue::pem_chain_to_der(&self.chain_pem)
    }

    /// Parse the private key into the rustls-typed
    /// [`PrivateKeyDer`].
    pub fn key_der(&self) -> Result<PrivateKeyDer<'static>> {
        rustls_glue::pem_key_to_der(&self.key_pem)
    }

    /// Parse every CA root in the trust bundle into the rustls-typed
    /// [`CertificateDer`] vector. Order is preserved.
    pub fn trust_bundle_der(&self) -> Result<Vec<CertificateDer<'static>>> {
        rustls_glue::pem_chain_to_der(&self.trust_bundle_pem)
    }
}

/// The transport-agnostic client. Wraps a [`WorkloadApi`]
/// implementation + optional metrics + the cached
/// [`SvidBundle`] last seen from the agent.
///
/// Production wires this with `HttpWorkloadApi::new(socket)` (gRPC
/// flavour is a follow-up); tests plug `MockWorkloadApi` or a
/// wiremock fixture.
pub struct SpiffeClient {
    api: Arc<dyn WorkloadApi>,
    metrics: Option<Arc<SpiffeMetrics>>,
    cached: tokio::sync::RwLock<Option<SvidBundle>>,
}

impl SpiffeClient {
    /// Build a client that talks to `api`. Metrics may be `None` in
    /// tests; production should always pass `Some(metrics)`.
    pub fn new(api: Arc<dyn WorkloadApi>, metrics: Option<Arc<SpiffeMetrics>>) -> Self {
        Self {
            api,
            metrics,
            cached: tokio::sync::RwLock::new(None),
        }
    }

    /// Fetch the SVID + trust bundle from the Workload API and cache
    /// them. Returns a clone of the freshly-fetched bundle.
    ///
    /// **Fail-closed (D14)**: if the underlying API is unreachable or
    /// returns garbage, the error propagates and the caller (typically
    /// the service's composition root) should refuse to start under
    /// `AUTH_TRANSPORT=mtls` / `AUTH_TRANSPORT=mtls-only`.
    pub async fn bootstrap(&self, expected_spiffe_id: &str) -> Result<SvidBundle> {
        let resp = match self.api.fetch_svid().await {
            Ok(r) => {
                if let Some(m) = &self.metrics {
                    m.svid_fetch_total
                        .with_label_values(&["success"])
                        .inc();
                }
                r
            }
            Err(e) => {
                if let Some(m) = &self.metrics {
                    m.svid_fetch_total
                        .with_label_values(&["failure"])
                        .inc();
                }
                return Err(e);
            }
        };
        if resp.spiffe_id != expected_spiffe_id {
            if let Some(m) = &self.metrics {
                m.svid_fetch_total
                    .with_label_values(&["mismatch"])
                    .inc();
            }
            return Err(SpiffeError::SpiffeIdMismatch {
                expected: expected_spiffe_id.to_string(),
                actual: resp.spiffe_id,
            });
        }
        let bundle = SvidBundle {
            chain_pem: resp.chain_pem,
            key_pem: resp.key_pem,
            trust_bundle_pem: resp.trust_bundle_pem,
            spiffe_id: resp.spiffe_id,
        };
        *self.cached.write().await = Some(bundle.clone());
        Ok(bundle)
    }

    /// Return the most recently fetched bundle, if any. Useful for
    /// rustls config builders that re-build the per-connection
    /// material from the cached state.
    pub async fn current(&self) -> Option<SvidBundle> {
        self.cached.read().await.clone()
    }

    /// Build a `rustls::ServerConfig` for **inbound** mTLS.
    /// Requires the bundle to have been fetched via
    /// [`Self::bootstrap`]. Re-fetches automatically if the cache
    /// is empty.
    pub async fn server_config(
        &self,
        expected_spiffe_id: &str,
    ) -> Result<Arc<rustls::ServerConfig>> {
        let bundle = match self.current().await {
            Some(b) => b,
            None => self.bootstrap(expected_spiffe_id).await?,
        };
        rustls_glue::build_server_config(&bundle)
    }

    /// Build a `rustls::ClientConfig` for **outbound** mTLS.
    pub async fn client_config(
        &self,
        expected_spiffe_id: &str,
    ) -> Result<Arc<rustls::ClientConfig>> {
        let bundle = match self.current().await {
            Some(b) => b,
            None => self.bootstrap(expected_spiffe_id).await?,
        };
        rustls_glue::build_client_config(&bundle)
    }
}

/// A SPIFFE-ID-shaped envelope. Parses + validates against the
/// `spiffe://<trust_domain>/<path>` shape; the helpers
/// [`Self::trust_domain`] and [`Self::path`] return the parsed
/// components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpiffeId(String);

impl SpiffeId {
    /// Parse a raw string into a `SpiffeId`. Returns
    /// `SpiffeError::MalformedSvid` for any shape that does not match
    /// `spiffe://<trust_domain>/<path>` with at least one path
    /// segment.
    pub fn parse(s: impl Into<String>) -> Result<Self> {
        let s: String = s.into();
        if !s.starts_with("spiffe://") {
            return Err(SpiffeError::MalformedSvid(format!(
                "missing spiffe:// scheme in `{s}`"
            )));
        }
        let after_scheme = &s[9..];
        let (td, path) = match after_scheme.find('/') {
            Some(i) => after_scheme.split_at(i),
            None => {
                return Err(SpiffeError::MalformedSvid(format!(
                    "no path component in `{s}`"
                )))
            }
        };
        if td.is_empty() || path.len() < 2 {
            return Err(SpiffeError::MalformedSvid(format!(
                "trust domain or path is empty in `{s}`"
            )));
        }
        Ok(SpiffeId(s))
    }

    /// The full SPIFFE ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `<trust_domain>` portion (between `spiffe://` and the
    /// first `/`).
    pub fn trust_domain(&self) -> &str {
        let after = &self.0[9..];
        match after.find('/') {
            Some(i) => &after[..i],
            None => after,
        }
    }

    /// The `<path>` portion (the part after the trust domain, **with**
    /// the leading `/`).
    pub fn path(&self) -> &str {
        let after = &self.0[9..];
        match after.find('/') {
            Some(i) => &after[i..],
            None => "",
        }
    }
}

impl std::fmt::Display for SpiffeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiffe_id_parses_well_formed() {
        let id = SpiffeId::parse("spiffe://recor.cm/declaration").expect("parses");
        assert_eq!(id.trust_domain(), "recor.cm");
        assert_eq!(id.path(), "/declaration");
        assert_eq!(id.as_str(), "spiffe://recor.cm/declaration");
    }

    #[test]
    fn spiffe_id_rejects_missing_scheme() {
        assert!(matches!(
            SpiffeId::parse("recor.cm/declaration"),
            Err(SpiffeError::MalformedSvid(_))
        ));
    }

    #[test]
    fn spiffe_id_rejects_missing_path() {
        assert!(matches!(
            SpiffeId::parse("spiffe://recor.cm"),
            Err(SpiffeError::MalformedSvid(_))
        ));
    }

    #[test]
    fn spiffe_id_rejects_empty_trust_domain() {
        assert!(matches!(
            SpiffeId::parse("spiffe:///declaration"),
            Err(SpiffeError::MalformedSvid(_))
        ));
    }

    #[test]
    fn default_socket_matches_agent_conf() {
        // Sanity: the socket path the crate advertises has to match
        // the one infrastructure/spire/agent.conf exposes via the
        // Workload API.
        assert_eq!(
            DEFAULT_WORKLOAD_API_SOCKET,
            "unix:///tmp/spire-agent/public/api.sock"
        );
    }

    #[test]
    fn trust_domain_constant_matches_brief() {
        assert_eq!(TRUST_DOMAIN, "recor.cm");
    }

    // ─── SpiffeClient bootstrap tests (using the MockWorkloadApi) ───
    //
    // These exercise the bootstrap code paths without spinning up
    // a wiremock HTTP server. The wiremock-backed integration tests
    // live under `tests/workload_api_wiremock.rs`.

    #[tokio::test]
    async fn bootstrap_with_mock_caches_bundle() {
        use crate::fixtures::{dummy_svid, MockWorkloadApi};
        let api = MockWorkloadApi::new(dummy_svid("spiffe://recor.cm/declaration"));
        let client = SpiffeClient::new(api.clone(), None);

        let b = client
            .bootstrap("spiffe://recor.cm/declaration")
            .await
            .expect("bootstrap with happy-path mock");
        assert_eq!(b.spiffe_id, "spiffe://recor.cm/declaration");
        assert_eq!(api.call_count().await, 1);

        // current() must return the cached bundle without re-calling.
        let cached = client.current().await.expect("cache populated");
        assert_eq!(cached.spiffe_id, "spiffe://recor.cm/declaration");
        assert_eq!(api.call_count().await, 1, "current() must not re-fetch");
    }

    #[tokio::test]
    async fn bootstrap_returns_mismatch_when_api_issues_wrong_id() {
        use crate::fixtures::{dummy_svid, MockWorkloadApi};
        let api = MockWorkloadApi::new(dummy_svid("spiffe://recor.cm/portal"));
        let client = SpiffeClient::new(api, None);

        let r = client.bootstrap("spiffe://recor.cm/declaration").await;
        assert!(matches!(r, Err(SpiffeError::SpiffeIdMismatch { .. })));
    }

    #[tokio::test]
    async fn bootstrap_propagates_unreachable_error() {
        use crate::fixtures::MockWorkloadApi;
        let api = MockWorkloadApi::unreachable();
        let client = SpiffeClient::new(api, None);

        let r = client.bootstrap("spiffe://recor.cm/declaration").await;
        assert!(matches!(r, Err(SpiffeError::WorkloadApiUnreachable(_))));
    }
}
