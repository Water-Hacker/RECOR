//! Test-only fixtures + in-process mocks.
//!
//! These are gated behind `cfg(test)` (and the optional
//! `test-fixtures` feature, if any consumer ever wants them at
//! integration-test scope). They are NEVER compiled into a release
//! build of the crate.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{Result, SpiffeError, WorkloadApi, X509SvidResponse};

/// A self-contained [`WorkloadApi`] implementation that hands out a
/// canned [`X509SvidResponse`]. Each call to [`Self::fetch_svid`]
/// returns a clone of the configured response (or, if a script of
/// responses was queued via [`Self::queue`], the next scripted one).
///
/// Counters of calls + failure injection are exposed so unit tests
/// can assert that `bootstrap` retried / failed / succeeded as
/// expected.
pub struct MockWorkloadApi {
    responses: Mutex<Vec<Result<X509SvidResponse>>>,
    default: Option<X509SvidResponse>,
    call_count: Mutex<usize>,
}

impl MockWorkloadApi {
    /// Build a mock that always returns `default` when no scripted
    /// response is queued.
    pub fn new(default: X509SvidResponse) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(Vec::new()),
            default: Some(default),
            call_count: Mutex::new(0),
        })
    }

    /// Build a mock with no default — the first call returns
    /// `Err(SpiffeError::WorkloadApiUnreachable)`. Useful for
    /// asserting the bootstrap error path.
    pub fn unreachable() -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(Vec::new()),
            default: None,
            call_count: Mutex::new(0),
        })
    }

    /// Queue a scripted response. The next call to `fetch_svid`
    /// consumes it (FIFO). When the queue is empty, the `default`
    /// (if any) is used.
    pub async fn queue(&self, response: Result<X509SvidResponse>) {
        self.responses.lock().await.push(response);
    }

    /// Number of times `fetch_svid` has been called since construction.
    pub async fn call_count(&self) -> usize {
        *self.call_count.lock().await
    }
}

#[async_trait]
impl WorkloadApi for MockWorkloadApi {
    async fn fetch_svid(&self) -> Result<X509SvidResponse> {
        *self.call_count.lock().await += 1;
        let mut queue = self.responses.lock().await;
        if !queue.is_empty() {
            return queue.remove(0);
        }
        drop(queue);
        match self.default.clone() {
            Some(r) => Ok(r),
            None => Err(SpiffeError::WorkloadApiUnreachable(
                "mock unreachable (no default)".into(),
            )),
        }
    }
}

/// Build a minimal **syntactically valid** PEM blob that
/// [`crate::rustls_glue::pem_chain_to_der`] accepts. Note: this is
/// *not* a real certificate — it parses through `rustls_pemfile`
/// because that crate only checks the PEM framing, not the DER.
/// Tests that need a real X.509 must mint one (e.g. via `rcgen`)
/// rather than this fixture.
pub fn dummy_pem_cert() -> Vec<u8> {
    // A small DER blob, base64-encoded between PEM markers.
    // `rustls_pemfile::certs` returns this as a CertificateDer; it
    // will obviously NOT verify in a real TLS handshake.
    b"-----BEGIN CERTIFICATE-----\nMIIBADCBu6ADAgECAgEBMA==\n-----END CERTIFICATE-----\n".to_vec()
}

/// A fully-shaped `X509SvidResponse` for tests that only need the
/// happy-path SPIFFE-ID extraction, not real TLS termination.
pub fn dummy_svid(spiffe_id: &str) -> X509SvidResponse {
    X509SvidResponse {
        spiffe_id: spiffe_id.to_string(),
        chain_pem: dummy_pem_cert(),
        key_pem: b"-----BEGIN PRIVATE KEY-----\nMIIBADCBu6ADAgECAgEBMA==\n-----END PRIVATE KEY-----\n".to_vec(),
        trust_bundle_pem: dummy_pem_cert(),
    }
}
