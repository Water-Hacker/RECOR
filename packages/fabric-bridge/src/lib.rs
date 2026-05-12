//! RÉCOR Fabric Bridge — Hyperledger Fabric gateway client wrapper.
//!
//! ## What this crate does
//!
//! Commits one audit entry per declaration event to the `audit-witness`
//! chaincode (see `chaincode/audit-witness/`) on the Fabric audit
//! channel. The bridge is the operator-side complement to the chaincode:
//! the chaincode enforces idempotency at the contract layer; the bridge
//! handles transient transport errors, retries, and translation between
//! the application-level `declaration.*` event envelope and the
//! chaincode method signature.
//!
//! ## Why HTTP (and not a Rust Fabric SDK)
//!
//! At the time of writing, the Rust ecosystem does not have a mature,
//! widely-deployed Hyperledger Fabric Gateway SDK. The two existing
//! crates (`hlf-sdk-rs`, `fabric-rust-sdk`) are early-stage and
//! lack production track records. Rather than couple the platform to
//! an unmaintained dependency, we depend on a thin **Fabric Gateway
//! HTTP shim** the operator stands up alongside the peer — a small Go
//! sidecar that translates HTTP POST `/v1/transactions/{channel}/{chaincode}`
//! into a Fabric Gateway gRPC `Endorse + Submit + CommitStatus` flow.
//! See `docs/runbooks/fabric-bridge.md` for the shim deployment.
//!
//! The bridge is structured so the transport is an injection point
//! (`Transport` trait); a future ticket can drop in a native gRPC
//! transport without changing the public API.
//!
//! ## Idempotency
//!
//! `commit_audit_entry` is idempotent at TWO layers:
//!
//! 1. **Chaincode layer**: a second invocation with the same `event_id`
//!    returns the "already exists" error from `PutAuditEntry`. The
//!    bridge maps this to `Ok(TxId::AlreadyCommitted)` — the caller
//!    treats it as success (D13).
//! 2. **Network layer**: transient errors (HTTP 5xx, timeouts) are
//!    retried with exponential backoff up to `max_attempts`. After
//!    exhaustion the bridge returns `Err(BridgeError::Permanent)`.
//!
//! ## Doctrines
//!
//! - **D13 idempotency**: idempotent at the bridge boundary regardless
//!   of how many times the worker retries.
//! - **D14 fail-closed**: permanent errors bubble up; the worker
//!   writes the row to `fabric_bridge_dlq` rather than acknowledging
//!   it as dispatched.
//! - **D15 cryptographic provenance**: the bridge passes through the
//!   declarant's receipt hash unchanged.
//! - **D16 observability**: the bridge accepts an optional
//!   `BridgeMetrics` handle and emits per-attempt latency + result
//!   counters.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

pub mod transport;

pub use transport::{HttpTransport, Transport, TransportError, TransportResponse};

/// Identifier of a committed Fabric transaction. The Hyperledger
/// Fabric Gateway returns these as opaque hex strings; we wrap them in
/// a strong type so callers can't confuse them with declaration_ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxId(pub String);

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result of a commit_audit_entry call. Distinguishes "we made a fresh
/// commit" from "the chaincode already had this entry" (idempotency).
/// Both are `Ok` outcomes for the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    /// A new entry was written to the audit channel in this call.
    Committed(TxId),
    /// The entry already existed (a prior call landed it). The bridge
    /// re-fetched the existing TxId from the chaincode and returned it.
    /// When the chaincode does not expose the originating TxId, the
    /// returned TxId is the placeholder string "already-committed".
    AlreadyCommitted(TxId),
}

impl CommitOutcome {
    /// Extract the underlying TxId regardless of variant.
    pub fn tx_id(&self) -> &TxId {
        match self {
            Self::Committed(t) | Self::AlreadyCommitted(t) => t,
        }
    }
}

/// Errors raised by the bridge. Two flavours: transient (the caller
/// should retry) and permanent (write to DLQ).
#[derive(Debug, Error)]
pub enum BridgeError {
    /// All retries exhausted. The underlying transport error message
    /// is captured for the DLQ row.
    #[error("permanent failure after {attempts} attempts: {source}")]
    Permanent {
        attempts: u32,
        #[source]
        source: TransportError,
    },

    /// Configuration error — the bridge can never succeed regardless
    /// of retries (e.g., malformed gateway URL). Caller should DLQ
    /// immediately rather than spinning.
    #[error("configuration error: {0}")]
    Config(String),

    /// The transport responded with a non-retryable application error
    /// (4xx that is NOT "already committed"). Permanent; DLQ.
    #[error("non-retryable transport error: {0}")]
    NonRetryable(String),
}

/// Configuration for a `FabricBridge` instance.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Base URL of the Fabric Gateway HTTP shim, e.g.
    /// `https://fabric-gateway.recor.local:8443`.
    pub gateway_url: String,
    /// Channel name the audit-witness chaincode is instantiated on.
    /// Default: `recor-audit`.
    pub channel: String,
    /// Chaincode name. Default: `audit-witness`.
    pub chaincode: String,
    /// Max attempts on transient errors before declaring permanent.
    /// Default: 5.
    pub max_attempts: u32,
    /// Base back-off between retries; doubled on each attempt up to a
    /// 30s cap.
    pub backoff_base: Duration,
    /// Per-attempt request timeout.
    pub request_timeout: Duration,
    /// Optional bearer token presented to the gateway shim
    /// (the shim verifies it against the peer's mTLS-derived identity
    /// or against its own static token list per operator policy).
    pub bearer_token: Option<String>,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://127.0.0.1:7050".to_string(),
            channel: "recor-audit".to_string(),
            chaincode: "audit-witness".to_string(),
            max_attempts: 5,
            backoff_base: Duration::from_millis(500),
            request_timeout: Duration::from_secs(10),
            bearer_token: None,
        }
    }
}

/// Optional metrics hook. The worker constructs a Prometheus-backed
/// implementation and passes it in via `with_metrics`. Implementors
/// must be `Send + Sync` because the bridge is used across tokio tasks.
pub trait BridgeMetrics: Send + Sync + std::fmt::Debug {
    /// Called after every attempt (success or failure), exactly once.
    /// `result_label` is one of "committed", "already_committed",
    /// "retried", "permanent_failure" — the same set the worker's
    /// Counter is configured with.
    fn record_attempt(&self, result_label: &str, latency_seconds: f64);
}

/// The bridge struct.
#[derive(Debug)]
pub struct FabricBridge {
    config: BridgeConfig,
    transport: Arc<dyn Transport>,
    metrics: Option<Arc<dyn BridgeMetrics>>,
}

impl FabricBridge {
    /// Build a bridge with the default HTTP transport.
    pub fn new(config: BridgeConfig) -> Result<Self, BridgeError> {
        if config.gateway_url.is_empty() {
            return Err(BridgeError::Config("gateway_url is empty".to_string()));
        }
        let transport = HttpTransport::new(&config)
            .map_err(|e| BridgeError::Config(format!("transport init: {e}")))?;
        Ok(Self {
            config,
            transport: Arc::new(transport),
            metrics: None,
        })
    }

    /// Build a bridge with a custom transport (test injection point).
    pub fn with_transport(config: BridgeConfig, transport: Arc<dyn Transport>) -> Self {
        Self {
            config,
            transport,
            metrics: None,
        }
    }

    /// Wire an optional metrics handle. Returns self for chaining.
    pub fn with_metrics(mut self, metrics: Arc<dyn BridgeMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Commit one audit entry. Idempotent and retried.
    ///
    /// Arguments:
    /// - `event_id`: the UUIDv7 the Declaration service minted.
    /// - `declaration_id`: the aggregate id.
    /// - `receipt_hash_hex`: 64 lowercase hex characters (BLAKE3-256).
    /// - `ts`: RFC3339 timestamp of the event.
    ///
    /// Returns:
    /// - `Ok(CommitOutcome::Committed(tx_id))` on fresh commit.
    /// - `Ok(CommitOutcome::AlreadyCommitted(tx_id))` on idempotent replay.
    /// - `Err(BridgeError::Permanent)` after `max_attempts` exhausted.
    /// - `Err(BridgeError::NonRetryable)` on application-level rejection
    ///   (e.g., malformed receipt hash — usually a bridge bug, never a
    ///   transient condition).
    #[instrument(skip_all, fields(event_id = %event_id, declaration_id = %declaration_id))]
    pub async fn commit_audit_entry(
        &self,
        event_id: &str,
        declaration_id: &str,
        receipt_hash_hex: &str,
        ts: &str,
    ) -> Result<CommitOutcome, BridgeError> {
        if receipt_hash_hex.len() != 64 {
            return Err(BridgeError::NonRetryable(format!(
                "receipt_hash_hex must be 64 chars, got {}",
                receipt_hash_hex.len()
            )));
        }

        let request = ChaincodeRequest {
            method: "PutAuditEntry".to_string(),
            args: vec![
                event_id.to_string(),
                declaration_id.to_string(),
                receipt_hash_hex.to_string(),
                ts.to_string(),
                // Signing peer attestation: BLAKE3(event_id || declaration_id ||
                // receipt_hash_hex) hex-encoded. The Gateway shim re-signs the
                // bytes with the peer's MSP-issued certificate before forwarding
                // to the orderer; the chaincode stores the resulting signature.
                // The Rust side passes the digest because we don't have access
                // to the peer's private key — that lives in the operator's HSM.
                signing_peer_attestation_hex(event_id, declaration_id, receipt_hash_hex),
            ],
        };

        let mut attempt: u32 = 0;
        let mut last_err: Option<TransportError> = None;

        while attempt < self.config.max_attempts {
            attempt += 1;
            let started = std::time::Instant::now();

            let result = self
                .transport
                .submit_transaction(&self.config.channel, &self.config.chaincode, &request)
                .await;

            let latency = started.elapsed().as_secs_f64();

            match result {
                Ok(resp) => {
                    if resp.already_committed {
                        if let Some(m) = self.metrics.as_ref() {
                            m.record_attempt("already_committed", latency);
                        }
                        let tx_id = resp
                            .tx_id
                            .unwrap_or_else(|| "already-committed".to_string());
                        info!(%tx_id, attempt, "audit entry already committed (idempotent replay)");
                        return Ok(CommitOutcome::AlreadyCommitted(TxId(tx_id)));
                    }

                    let tx_id = resp.tx_id.ok_or_else(|| {
                        BridgeError::NonRetryable("gateway returned no tx_id".to_string())
                    })?;
                    if let Some(m) = self.metrics.as_ref() {
                        m.record_attempt("committed", latency);
                    }
                    info!(%tx_id, attempt, "audit entry committed");
                    return Ok(CommitOutcome::Committed(TxId(tx_id)));
                }
                Err(TransportError::NonRetryable(msg)) => {
                    if let Some(m) = self.metrics.as_ref() {
                        m.record_attempt("permanent_failure", latency);
                    }
                    warn!(error = %msg, attempt, "non-retryable transport error");
                    return Err(BridgeError::NonRetryable(msg));
                }
                Err(e) => {
                    if let Some(m) = self.metrics.as_ref() {
                        m.record_attempt("retried", latency);
                    }
                    warn!(error = %e, attempt, max_attempts = self.config.max_attempts, "transient transport error, will retry");
                    last_err = Some(e);
                    if attempt < self.config.max_attempts {
                        let delay = backoff(self.config.backoff_base, attempt);
                        debug!(delay_ms = delay.as_millis() as u64, "backoff before retry");
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(BridgeError::Permanent {
            attempts: attempt,
            source: last_err.unwrap_or_else(|| {
                TransportError::Retryable("retries exhausted with no captured error".to_string())
            }),
        })
    }
}

/// Wire request to the gateway shim.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct ChaincodeRequest {
    pub method: String,
    pub args: Vec<String>,
}

/// Exponential backoff capped at 30s.
fn backoff(base: Duration, attempt: u32) -> Duration {
    let cap = Duration::from_secs(30);
    let scaled = base.saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1)));
    std::cmp::min(scaled, cap)
}

/// Compute the signing-peer attestation digest. This is the hex-encoded
/// BLAKE3-256 over the concatenation `event_id || ":" || declaration_id
/// || ":" || receipt_hash_hex`. The separator is deliberate — without it
/// the hash would collide on values that happen to share a prefix.
pub fn signing_peer_attestation_hex(
    event_id: &str,
    declaration_id: &str,
    receipt_hash_hex: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(event_id.as_bytes());
    hasher.update(b":");
    hasher.update(declaration_id.as_bytes());
    hasher.update(b":");
    hasher.update(receipt_hash_hex.as_bytes());
    hex::encode(hasher.finalize().as_bytes())
}

// ── In-process stub transport (used by both worker tests and integration) ─

/// In-memory transport that records every call. Public so the worker
/// crate can use the same fixture in its tests.
#[derive(Debug, Default)]
pub struct InMemoryTransport {
    calls: tokio::sync::Mutex<Vec<ChaincodeRequest>>,
    response: tokio::sync::Mutex<TransportBehaviour>,
}

#[derive(Debug, Clone)]
enum TransportBehaviour {
    /// Return Ok(committed) with a deterministic tx_id derived from the args.
    Ok,
    /// Return Ok(already_committed) the first call; tests use this to
    /// verify idempotency without coupling to a counter.
    AlreadyCommitted,
    /// Return Err(retryable) `count` times then succeed.
    FailThenSucceed { count: u32, current: u32 },
    /// Always return Err(retryable).
    AlwaysRetryable,
    /// Return Err(non-retryable).
    NonRetryable,
}

impl Default for TransportBehaviour {
    fn default() -> Self {
        Self::Ok
    }
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set_always_ok(&self) {
        *self.response.lock().await = TransportBehaviour::Ok;
    }
    pub async fn set_already_committed(&self) {
        *self.response.lock().await = TransportBehaviour::AlreadyCommitted;
    }
    pub async fn set_fail_then_succeed(&self, count: u32) {
        *self.response.lock().await = TransportBehaviour::FailThenSucceed { count, current: 0 };
    }
    pub async fn set_always_retryable(&self) {
        *self.response.lock().await = TransportBehaviour::AlwaysRetryable;
    }
    pub async fn set_non_retryable(&self) {
        *self.response.lock().await = TransportBehaviour::NonRetryable;
    }

    pub async fn calls(&self) -> Vec<ChaincodeRequest> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl Transport for InMemoryTransport {
    async fn submit_transaction(
        &self,
        _channel: &str,
        _chaincode: &str,
        request: &ChaincodeRequest,
    ) -> Result<TransportResponse, TransportError> {
        self.calls.lock().await.push(request.clone());
        let mut behaviour = self.response.lock().await;
        match &mut *behaviour {
            TransportBehaviour::Ok => Ok(TransportResponse {
                tx_id: Some(deterministic_tx_id(request)),
                already_committed: false,
            }),
            TransportBehaviour::AlreadyCommitted => Ok(TransportResponse {
                tx_id: Some("already-committed-stub".to_string()),
                already_committed: true,
            }),
            TransportBehaviour::FailThenSucceed { count, current } => {
                if *current < *count {
                    *current += 1;
                    Err(TransportError::Retryable(format!(
                        "synthetic transient failure {current}/{count}"
                    )))
                } else {
                    Ok(TransportResponse {
                        tx_id: Some(deterministic_tx_id(request)),
                        already_committed: false,
                    })
                }
            }
            TransportBehaviour::AlwaysRetryable => {
                Err(TransportError::Retryable("synthetic transient".to_string()))
            }
            TransportBehaviour::NonRetryable => Err(TransportError::NonRetryable(
                "synthetic permanent".to_string(),
            )),
        }
    }
}

fn deterministic_tx_id(req: &ChaincodeRequest) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(req.method.as_bytes());
    for a in &req.args {
        hasher.update(a.as_bytes());
        hasher.update(&[0]);
    }
    hex::encode(hasher.finalize().as_bytes())
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_HASH: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn config() -> BridgeConfig {
        BridgeConfig {
            backoff_base: Duration::from_millis(1),
            max_attempts: 4,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn commit_succeeds_on_first_attempt() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_always_ok().await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let outcome = bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();

        assert!(matches!(outcome, CommitOutcome::Committed(_)));
        let calls = transport.calls().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "PutAuditEntry");
        // 5 args: event_id, declaration_id, receipt_hash, ts, attestation
        assert_eq!(calls[0].args.len(), 5);
    }

    #[tokio::test]
    async fn already_committed_returns_idempotent_outcome() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_already_committed().await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let outcome = bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();

        match outcome {
            CommitOutcome::AlreadyCommitted(tx) => assert_eq!(tx.0, "already-committed-stub"),
            other => panic!("expected AlreadyCommitted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retries_transient_failures_then_succeeds() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_fail_then_succeed(2).await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let outcome = bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();

        assert!(matches!(outcome, CommitOutcome::Committed(_)));
        let calls = transport.calls().await;
        assert_eq!(calls.len(), 3, "two failures + one success");
    }

    #[tokio::test]
    async fn returns_permanent_after_max_attempts() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_always_retryable().await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let err = bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap_err();

        match err {
            BridgeError::Permanent { attempts, .. } => assert_eq!(attempts, 4),
            other => panic!("expected Permanent, got {other:?}"),
        }
        let calls = transport.calls().await;
        assert_eq!(calls.len(), 4);
    }

    #[tokio::test]
    async fn non_retryable_returns_immediately() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_non_retryable().await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let err = bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap_err();

        assert!(matches!(err, BridgeError::NonRetryable(_)));
        let calls = transport.calls().await;
        assert_eq!(calls.len(), 1, "no retries on non-retryable");
    }

    #[tokio::test]
    async fn rejects_malformed_receipt_hash() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_always_ok().await;
        let bridge = FabricBridge::with_transport(config(), transport.clone());

        let err = bridge
            .commit_audit_entry("evt-1", "decl-1", "tooshort", "2026-05-12T10:00:00Z")
            .await
            .unwrap_err();

        assert!(matches!(err, BridgeError::NonRetryable(_)));
        let calls = transport.calls().await;
        assert_eq!(calls.len(), 0, "validation runs BEFORE transport");
    }

    #[test]
    fn new_rejects_empty_gateway_url() {
        let cfg = BridgeConfig {
            gateway_url: String::new(),
            ..Default::default()
        };
        let err = FabricBridge::new(cfg).unwrap_err();
        assert!(matches!(err, BridgeError::Config(_)));
    }

    #[test]
    fn signing_peer_attestation_is_deterministic() {
        let a = signing_peer_attestation_hex("evt", "decl", VALID_HASH);
        let b = signing_peer_attestation_hex("evt", "decl", VALID_HASH);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "BLAKE3-256 hex form");
        let c = signing_peer_attestation_hex("evt2", "decl", VALID_HASH);
        assert_ne!(a, c);
    }

    #[test]
    fn backoff_caps_at_thirty_seconds() {
        let base = Duration::from_secs(1);
        // 2^10 = 1024 → would be > 30s if uncapped.
        assert_eq!(backoff(base, 11), Duration::from_secs(30));
        // 2^0 = 1 → still base.
        assert_eq!(backoff(base, 1), Duration::from_secs(1));
        // 2^1 = 2 → 2 × base.
        assert_eq!(backoff(base, 2), Duration::from_secs(2));
    }

    #[derive(Debug, Default)]
    struct CapturingMetrics(std::sync::Mutex<Vec<(String, f64)>>);

    impl BridgeMetrics for CapturingMetrics {
        fn record_attempt(&self, label: &str, latency: f64) {
            self.0.lock().unwrap().push((label.to_string(), latency));
        }
    }

    #[tokio::test]
    async fn metrics_record_each_attempt() {
        let transport = Arc::new(InMemoryTransport::new());
        transport.set_fail_then_succeed(2).await;
        let metrics = Arc::new(CapturingMetrics::default());
        let bridge = FabricBridge::with_transport(config(), transport)
            .with_metrics(metrics.clone());

        bridge
            .commit_audit_entry("evt-1", "decl-1", VALID_HASH, "2026-05-12T10:00:00Z")
            .await
            .unwrap();

        let recorded = metrics.0.lock().unwrap().clone();
        assert_eq!(recorded.len(), 3, "2 retries + 1 success");
        assert_eq!(recorded[0].0, "retried");
        assert_eq!(recorded[1].0, "retried");
        assert_eq!(recorded[2].0, "committed");
    }
}
