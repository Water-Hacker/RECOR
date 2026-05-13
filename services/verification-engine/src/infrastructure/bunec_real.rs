//! Real BUNEC adapter (R-VER-1).
//!
//! HTTP/REST client against the national identity registry, wrapped in:
//!   * A retry loop (exponential backoff, 3 attempts) for transient
//!     transport failures.
//!   * A circuit breaker (open after 5 consecutive failures, half-open
//!     after 30s) so a degraded BUNEC does not cascade into pipeline-
//!     wide latency blowup.
//!   * A fail-open / fail-closed switch via `BUNEC_FAIL_POLICY` —
//!     dev defaults to fail-open (vacuous BPA when BUNEC is unreachable),
//!     prod defaults to fail-closed (the adapter returns a transport
//!     error so the stage emits a `Fail` outcome).
//!
//! When the breaker is open, `lookup` returns
//! `BunecLookup::CircuitOpen { since }` so the stage records an explicit
//! "we could not check" rather than a false negative.
//!
//! The actual REST wire shape is approximated here from the publicly
//! described BUNEC v1 spec; the exact JSON contract is finalised in the
//! data-sharing agreement with the partner. Wire-level adjustments are
//! a config-switch follow-up; everything else (retry, breaker, metrics,
//! fail-policy) is final.
//!
//! Doctrines that bear:
//!   * D14 fail-closed at integration boundaries — the breaker default
//!     in prod is fail-closed.
//!   * D17 zero-trust — every response is shape-validated; we never
//!     trust the body without parsing into a typed struct.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU8, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::application::port::{BunecAdapter, BunecLookup, BunecLookupError};
use crate::metrics::Metrics;

/// Failure policy at the integration boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BunecFailPolicy {
    /// When the breaker is open or all retries fail, surface a transport
    /// error to the stage. The stage will emit `Fail` / vacuous depending
    /// on its own scoring rules. This is the production default.
    FailClosed,
    /// When the breaker is open or all retries fail, surface
    /// `BunecLookup::CircuitOpen` so the stage emits `InsufficientEvidence`
    /// with vacuous BPA. This is the dev default — keeps the pipeline
    /// running when partners are unreachable.
    FailOpen,
}

impl BunecFailPolicy {
    /// Parse from a config string. Accepts `fail_open` / `fail_closed`
    /// (snake_case) or `fail-open` / `fail-closed` (kebab-case),
    /// case-insensitive. Empty string → dev-default fail-open.
    pub fn parse(s: &str, environment: &str) -> Self {
        let norm = s.trim().to_ascii_lowercase().replace('-', "_");
        match norm.as_str() {
            "fail_open" => Self::FailOpen,
            "fail_closed" => Self::FailClosed,
            "" => {
                if environment == "dev" {
                    Self::FailOpen
                } else {
                    Self::FailClosed
                }
            }
            _ => {
                // Unrecognised: pick the safer (fail-closed) and log a warning.
                warn!(
                    value = %s,
                    environment = %environment,
                    "unrecognised BUNEC_FAIL_POLICY value; defaulting to fail_closed"
                );
                Self::FailClosed
            }
        }
    }
}

/// Configuration for the real BUNEC adapter.
#[derive(Debug, Clone)]
pub struct BunecRealConfig {
    pub base_url: String,
    pub api_key: SecretString,
    /// Per-call HTTP timeout (each retry attempt is bounded by this).
    pub request_timeout: Duration,
    /// Number of retry attempts within a single `lookup` call.
    /// Total attempts = `retry_attempts` (1 means no retries).
    pub retry_attempts: u32,
    /// Base for exponential backoff between retries (200ms recommended).
    pub retry_base_backoff: Duration,
    /// Open the breaker after this many consecutive failures.
    pub breaker_consecutive_failures: u32,
    /// Half-open the breaker this long after opening.
    pub breaker_half_open_after: Duration,
    /// Fail policy on open-breaker / all-retries-failed.
    pub fail_policy: BunecFailPolicy,
}

impl BunecRealConfig {
    /// Production-leaning defaults; production overrides via env vars.
    pub fn defaults(environment: &str) -> Self {
        Self {
            base_url: String::new(),
            api_key: SecretString::from(String::new()),
            request_timeout: Duration::from_secs(2),
            retry_attempts: 3,
            retry_base_backoff: Duration::from_millis(200),
            breaker_consecutive_failures: 5,
            breaker_half_open_after: Duration::from_secs(30),
            fail_policy: BunecFailPolicy::parse("", environment),
        }
    }
}

/// Tri-state circuit breaker. Closed → traffic flows; Open → traffic
/// short-circuits; HalfOpen → a single probe is allowed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakerState {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

impl BreakerState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Open,
            2 => Self::HalfOpen,
            _ => Self::Closed,
        }
    }
}

/// In-process circuit breaker. One per adapter instance. Atomics keep
/// the breaker lock-free on the hot path.
#[derive(Debug)]
struct CircuitBreaker {
    state: AtomicU8,
    consecutive_failures: AtomicU32,
    /// Unix-millis of when the breaker entered the Open state.
    opened_at_ms: AtomicI64,
    threshold: u32,
    half_open_after: Duration,
}

impl CircuitBreaker {
    fn new(threshold: u32, half_open_after: Duration) -> Self {
        Self {
            state: AtomicU8::new(BreakerState::Closed as u8),
            consecutive_failures: AtomicU32::new(0),
            opened_at_ms: AtomicI64::new(0),
            threshold,
            half_open_after,
        }
    }

    /// Snapshot the current state. May transition Open → HalfOpen if
    /// the cool-down has elapsed.
    fn current(&self) -> BreakerState {
        let st = BreakerState::from_u8(self.state.load(Ordering::Acquire));
        if st != BreakerState::Open {
            return st;
        }
        let opened_at_ms = self.opened_at_ms.load(Ordering::Acquire);
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let elapsed_ms = (now_ms as i64) - opened_at_ms;
        if elapsed_ms >= self.half_open_after.as_millis() as i64 {
            // Try to atomically transition; if a concurrent caller already
            // took the half-open slot, fall back to Open.
            let _ = self.state.compare_exchange(
                BreakerState::Open as u8,
                BreakerState::HalfOpen as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            BreakerState::from_u8(self.state.load(Ordering::Acquire))
        } else {
            BreakerState::Open
        }
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Release);
        self.state.store(BreakerState::Closed as u8, Ordering::Release);
        self.opened_at_ms.store(0, Ordering::Release);
    }

    fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        // From HalfOpen, a single failure re-opens the breaker.
        let in_half_open =
            BreakerState::from_u8(self.state.load(Ordering::Acquire)) == BreakerState::HalfOpen;
        if failures >= self.threshold || in_half_open {
            let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
            self.opened_at_ms.store(now_ms as i64, Ordering::Release);
            self.state.store(BreakerState::Open as u8, Ordering::Release);
        }
    }

    fn opened_at(&self) -> Option<OffsetDateTime> {
        let ms = self.opened_at_ms.load(Ordering::Acquire);
        if ms == 0 {
            None
        } else {
            OffsetDateTime::from_unix_timestamp_nanos((ms as i128) * 1_000_000).ok()
        }
    }
}

/// The real BUNEC adapter. Holds a `reqwest::Client` + a circuit breaker.
pub struct RealBunecAdapter {
    client: Client,
    config: BunecRealConfig,
    breaker: Arc<CircuitBreaker>,
    metrics: Option<Arc<Metrics>>,
}

impl RealBunecAdapter {
    pub fn new(config: BunecRealConfig) -> Result<Self, BunecAdapterError> {
        let client = Client::builder()
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| BunecAdapterError::ClientInit(e.to_string()))?;
        let breaker = Arc::new(CircuitBreaker::new(
            config.breaker_consecutive_failures,
            config.breaker_half_open_after,
        ));
        Ok(Self { client, config, breaker, metrics: None })
    }

    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Test-only constructor that lets the caller pass an arbitrary
    /// `reqwest::Client` (e.g. one pointed at a local axum test server).
    #[doc(hidden)]
    pub fn with_client(client: Client, config: BunecRealConfig) -> Self {
        let breaker = Arc::new(CircuitBreaker::new(
            config.breaker_consecutive_failures,
            config.breaker_half_open_after,
        ));
        Self { client, config, breaker, metrics: None }
    }

    fn observe_result(&self, label: &'static str, started: std::time::Instant) {
        if let Some(m) = &self.metrics {
            m.bunec_calls_total.with_label_values(&[label]).inc();
            m.bunec_call_latency_seconds
                .with_label_values(&[label])
                .observe(started.elapsed().as_secs_f64());
        }
    }

    /// One HTTP attempt against `GET {base_url}/v1/persons/{person_id}`.
    /// Returns:
    ///   Ok(Some(record)) — 200, found
    ///   Ok(None) — 404, not found
    ///   Err(transient) — to be retried
    async fn attempt(&self, person_id: Uuid) -> Result<Option<BunecRecord>, AttemptError> {
        let url = format!(
            "{}/v1/persons/{}",
            self.config.base_url.trim_end_matches('/'),
            person_id
        );
        let bearer = format!("Bearer {}", self.config.api_key.expose_secret());
        let resp = self
            .client
            .get(&url)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, bearer)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| AttemptError::Transport(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::OK {
            // D17: parse strictly; never trust the body shape.
            let body: BunecRecord = resp
                .json()
                .await
                .map_err(|e| AttemptError::Decode(e.to_string()))?;
            Ok(Some(body))
        } else if status == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            Err(AttemptError::Transport(format!("upstream status {status}")))
        } else {
            // 4xx other than 404 — surface as a non-retryable backend error.
            Err(AttemptError::NonRetryable(format!(
                "BUNEC returned non-retryable status {status}"
            )))
        }
    }
}

/// Wire-level decode shape for a BUNEC person record. Kept narrow on
/// purpose: only the fields the verification engine consumes.
#[derive(Debug, Deserialize)]
struct BunecRecord {
    person_id: Uuid,
    canonical_full_name: String,
    nationality: String,
}

#[derive(Debug, thiserror::Error)]
enum AttemptError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("non-retryable: {0}")]
    NonRetryable(String),
}

#[derive(Debug, thiserror::Error)]
pub enum BunecAdapterError {
    #[error("HTTP client init failure: {0}")]
    ClientInit(String),
}

#[async_trait]
impl BunecAdapter for RealBunecAdapter {
    #[instrument(skip(self), fields(person_id = %person_id))]
    async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError> {
        let started = std::time::Instant::now();

        // Short-circuit on open breaker.
        let breaker_state = self.breaker.current();
        if matches!(breaker_state, BreakerState::Open) {
            warn!("BUNEC circuit open — short-circuiting lookup");
            self.observe_result("circuit_open", started);
            return self.apply_fail_policy_open();
        }

        // Retry loop.
        let mut last_err: Option<String> = None;
        for attempt in 1..=self.config.retry_attempts {
            match self.attempt(person_id).await {
                Ok(Some(rec)) => {
                    // D17: verify the returned person_id matches what we asked for.
                    if rec.person_id != person_id {
                        warn!(
                            requested = %person_id,
                            returned = %rec.person_id,
                            "BUNEC returned a different person_id than requested; treating as not-found"
                        );
                        self.breaker.record_success();
                        self.observe_result("mismatch", started);
                        return Ok(BunecLookup::NotFound { person_id });
                    }
                    self.breaker.record_success();
                    self.observe_result("found", started);
                    return Ok(BunecLookup::Found {
                        person_id: rec.person_id,
                        canonical_full_name: rec.canonical_full_name,
                        nationality: rec.nationality,
                    });
                }
                Ok(None) => {
                    self.breaker.record_success();
                    self.observe_result("not_found", started);
                    return Ok(BunecLookup::NotFound { person_id });
                }
                Err(AttemptError::NonRetryable(msg)) => {
                    error!(error = %msg, "BUNEC non-retryable error");
                    self.breaker.record_failure();
                    self.observe_result("non_retryable", started);
                    return Err(BunecLookupError::Backend(msg));
                }
                Err(e) => {
                    let msg = e.to_string();
                    debug!(attempt, %msg, "BUNEC attempt failed; will back off");
                    last_err = Some(msg);
                    if attempt < self.config.retry_attempts {
                        let backoff = self.config.retry_base_backoff
                            * 2u32.pow(attempt.saturating_sub(1));
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        // All retries exhausted. Record one failure on the breaker (not
        // one per attempt — a single user request maps to a single
        // "failure" from the breaker's standpoint).
        self.breaker.record_failure();
        info!(
            attempts = self.config.retry_attempts,
            "BUNEC retries exhausted"
        );
        self.observe_result("retries_exhausted", started);

        // If the breaker just opened, apply the fail policy. Otherwise
        // surface a transport error.
        if matches!(self.breaker.current(), BreakerState::Open) {
            self.apply_fail_policy_open()
        } else {
            match self.config.fail_policy {
                BunecFailPolicy::FailOpen => Ok(BunecLookup::NotFound { person_id }),
                BunecFailPolicy::FailClosed => {
                    let msg = last_err.unwrap_or_else(|| "unknown transport error".into());
                    Err(BunecLookupError::Backend(msg))
                }
            }
        }
    }
}

impl RealBunecAdapter {
    fn apply_fail_policy_open(&self) -> Result<BunecLookup, BunecLookupError> {
        let since = self
            .breaker
            .opened_at()
            .map(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "unknown".into())
            })
            .unwrap_or_else(|| "unknown".into());
        match self.config.fail_policy {
            BunecFailPolicy::FailOpen => Ok(BunecLookup::CircuitOpen { since }),
            BunecFailPolicy::FailClosed => Err(BunecLookupError::Backend(format!(
                "bunec circuit open at {since}; fail-closed"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Json;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::routing::get;
    use reqwest::Client;
    use serde_json::json;
    use tokio::net::TcpListener;

    use super::*;

    /// Server state: configurable status code + body for a given probe.
    #[derive(Clone)]
    struct ServerState {
        mode: Arc<std::sync::Mutex<ServerMode>>,
        hits: Arc<std::sync::atomic::AtomicU32>,
    }

    #[derive(Clone)]
    enum ServerMode {
        AlwaysFound(BunecRecordValue),
        AlwaysNotFound,
        Always500,
        FailNThenOk { failures_remaining: u32, on_success: BunecRecordValue },
    }

    #[derive(Clone)]
    struct BunecRecordValue {
        person_id: Uuid,
        canonical_full_name: String,
        nationality: String,
    }

    async fn handler(
        State(state): State<ServerState>,
        Path(person_id): Path<Uuid>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        state.hits.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let mut mode = state.mode.lock().unwrap();
        match &mut *mode {
            ServerMode::AlwaysFound(rec) => (
                StatusCode::OK,
                Json(json!({
                    "person_id": rec.person_id,
                    "canonical_full_name": rec.canonical_full_name,
                    "nationality": rec.nationality,
                })),
            ),
            ServerMode::AlwaysNotFound => (StatusCode::NOT_FOUND, Json(json!({}))),
            ServerMode::Always500 => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "simulated"})),
            ),
            ServerMode::FailNThenOk { failures_remaining, on_success } => {
                if *failures_remaining > 0 {
                    *failures_remaining -= 1;
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "transient"})),
                    )
                } else {
                    let body = json!({
                        "person_id": person_id,
                        "canonical_full_name": on_success.canonical_full_name,
                        "nationality": on_success.nationality,
                    });
                    (StatusCode::OK, Json(body))
                }
            }
        }
    }

    async fn spawn_server(initial: ServerMode) -> (String, ServerState) {
        let state = ServerState {
            mode: Arc::new(std::sync::Mutex::new(initial)),
            hits: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        let app = axum::Router::new()
            .route("/v1/persons/{person_id}", get(handler))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), state)
    }

    fn cfg(base_url: String) -> BunecRealConfig {
        BunecRealConfig {
            base_url,
            api_key: SecretString::from("test-key".to_string()),
            request_timeout: Duration::from_millis(500),
            retry_attempts: 3,
            retry_base_backoff: Duration::from_millis(5),
            breaker_consecutive_failures: 5,
            breaker_half_open_after: Duration::from_millis(50),
            fail_policy: BunecFailPolicy::FailOpen,
        }
    }

    fn client() -> Client {
        Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn happy_path_found() {
        let pid = Uuid::now_v7();
        let (url, _state) = spawn_server(ServerMode::AlwaysFound(BunecRecordValue {
            person_id: pid,
            canonical_full_name: "Aïssa Ngo Bidoung".into(),
            nationality: "CM".into(),
        }))
        .await;
        let adapter = RealBunecAdapter::with_client(client(), cfg(url));
        let r = adapter.lookup(pid).await.unwrap();
        match r {
            BunecLookup::Found { canonical_full_name, nationality, .. } => {
                assert_eq!(canonical_full_name, "Aïssa Ngo Bidoung");
                assert_eq!(nationality, "CM");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn not_found_is_not_found() {
        let (url, _) = spawn_server(ServerMode::AlwaysNotFound).await;
        let adapter = RealBunecAdapter::with_client(client(), cfg(url));
        let r = adapter.lookup(Uuid::now_v7()).await.unwrap();
        assert!(matches!(r, BunecLookup::NotFound { .. }));
    }

    #[tokio::test]
    async fn retries_eventually_succeed() {
        let pid = Uuid::now_v7();
        let (url, state) = spawn_server(ServerMode::FailNThenOk {
            failures_remaining: 2,
            on_success: BunecRecordValue {
                person_id: pid,
                canonical_full_name: "Recovered".into(),
                nationality: "CM".into(),
            },
        })
        .await;
        let adapter = RealBunecAdapter::with_client(client(), cfg(url));
        let r = adapter.lookup(pid).await.unwrap();
        assert!(matches!(r, BunecLookup::Found { .. }));
        // 3 attempts hit the server: 2 failures + 1 success.
        assert_eq!(state.hits.load(std::sync::atomic::Ordering::Acquire), 3);
    }

    #[tokio::test]
    async fn fail_open_returns_circuit_open() {
        let (url, _) = spawn_server(ServerMode::Always500).await;
        let mut c = cfg(url);
        c.fail_policy = BunecFailPolicy::FailOpen;
        c.breaker_consecutive_failures = 1; // Open immediately.
        let adapter = RealBunecAdapter::with_client(client(), c);
        // First call: retries exhausted, breaker opens, fail policy applies.
        let r = adapter.lookup(Uuid::now_v7()).await.unwrap();
        assert!(matches!(r, BunecLookup::CircuitOpen { .. }));
        // Second call: breaker still open → short-circuits.
        let r2 = adapter.lookup(Uuid::now_v7()).await.unwrap();
        assert!(matches!(r2, BunecLookup::CircuitOpen { .. }));
    }

    #[tokio::test]
    async fn fail_closed_surfaces_error() {
        let (url, _) = spawn_server(ServerMode::Always500).await;
        let mut c = cfg(url);
        c.fail_policy = BunecFailPolicy::FailClosed;
        c.breaker_consecutive_failures = 1;
        let adapter = RealBunecAdapter::with_client(client(), c);
        let r = adapter.lookup(Uuid::now_v7()).await;
        assert!(matches!(r, Err(BunecLookupError::Backend(_))));
    }

    #[tokio::test]
    async fn breaker_half_opens_after_cooldown() {
        // Server returns 500, breaker opens, we sleep past the cool-down,
        // then the server flips to AlwaysFound and the breaker recovers.
        let pid = Uuid::now_v7();
        let (url, state) = spawn_server(ServerMode::Always500).await;
        let mut c = cfg(url);
        c.breaker_consecutive_failures = 1;
        c.breaker_half_open_after = Duration::from_millis(20);
        c.retry_attempts = 1;
        let adapter = RealBunecAdapter::with_client(client(), c);

        // Open the breaker.
        let _ = adapter.lookup(pid).await.unwrap();
        // Now flip the server to AlwaysFound.
        {
            let mut mode = state.mode.lock().unwrap();
            *mode = ServerMode::AlwaysFound(BunecRecordValue {
                person_id: pid,
                canonical_full_name: "Recovered".into(),
                nationality: "CM".into(),
            });
        }
        // Wait past the cooldown.
        tokio::time::sleep(Duration::from_millis(35)).await;
        // Half-open probe succeeds → breaker closes.
        let r = adapter.lookup(pid).await.unwrap();
        assert!(matches!(r, BunecLookup::Found { .. }));
    }

    #[test]
    fn fail_policy_parser_picks_dev_open() {
        assert_eq!(BunecFailPolicy::parse("", "dev"), BunecFailPolicy::FailOpen);
        assert_eq!(BunecFailPolicy::parse("", "prod"), BunecFailPolicy::FailClosed);
        assert_eq!(BunecFailPolicy::parse("fail_open", "prod"), BunecFailPolicy::FailOpen);
        assert_eq!(BunecFailPolicy::parse("Fail-Closed", "dev"), BunecFailPolicy::FailClosed);
        assert_eq!(BunecFailPolicy::parse("garbage", "dev"), BunecFailPolicy::FailClosed);
    }

    #[test]
    fn breaker_records_state_transitions() {
        let b = CircuitBreaker::new(3, Duration::from_millis(10));
        assert_eq!(b.current(), BreakerState::Closed);
        b.record_failure();
        b.record_failure();
        assert_eq!(b.current(), BreakerState::Closed);
        b.record_failure();
        assert_eq!(b.current(), BreakerState::Open);
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(b.current(), BreakerState::HalfOpen);
        b.record_success();
        assert_eq!(b.current(), BreakerState::Closed);
    }
}
