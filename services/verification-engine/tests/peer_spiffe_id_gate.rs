//! FIND-017 integration test — peer-SPIFFE-ID gate end-to-end.
//!
//! The audit catalogue marks this gate as "runs in an outer tower
//! layer that isn't covered by an assertion. A future refactor could
//! silently disable the gate." This file is the assertion.
//!
//! We mount the EXACT same middleware structure
//! `recor_spiffe::middleware`'s top-of-module doc sketch describes
//! (and that the R-LOOP-3-followup wiring in `main.rs` will adopt):
//! a `from_fn_with_state` closure that reads
//! `Extension<PeerSpiffeId>` from the request and calls
//! `enforce_peer_id`. The test substitutes a manual extension-injector
//! for the rustls peer-cert extractor so we don't need a live SPIRE.
//!
//! The four scenarios are:
//!   - matching peer SPIFFE ID  → handler reached, 200
//!   - mismatching peer SPIFFE ID → middleware returns 403
//!   - no peer SPIFFE ID (extension absent) → middleware returns 403
//!   - the `recor_spiffe_peer_verify_total{result="denied"}` counter
//!     increments on every refusal so an operator alert can fire
//!
//! Run with:
//!   cargo test -p recor-verification-engine --test peer_spiffe_id_gate
//!
//! Requires nothing — pure axum + the shared `recor-spiffe` crate.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use prometheus::Registry;
use recor_spiffe::middleware::{enforce_peer_id, PeerSpiffeId};
use recor_spiffe::{SpiffeId, SpiffeMetrics};
use tower::ServiceExt;

/// Shared middleware state: the expected peer SPIFFE ID + the SPIFFE
/// metrics handle. The composition root will build the equivalent
/// from `cfg.spiffe_id_peer` and the OBS-1 registry.
#[derive(Clone)]
struct PeerSpiffeGateState {
    expected: String,
    metrics: Arc<SpiffeMetrics>,
}

/// The production middleware (the closure `main.rs` will mount). Reads
/// the verified `PeerSpiffeId` extension placed on the request by the
/// rustls TLS layer; refuses with 403 if missing or mismatched.
async fn peer_spiffe_gate_middleware(
    State(state): State<PeerSpiffeGateState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let peer = req.extensions().get::<PeerSpiffeId>().cloned();
    if let Err(e) = enforce_peer_id(peer.as_ref(), &state.expected, Some(&state.metrics))
    {
        tracing::warn!(error = %e, expected = %state.expected, "peer-SPIFFE-ID gate refused request");
        return (StatusCode::FORBIDDEN, "peer not allowed").into_response();
    }
    next.run(req).await
}

/// Build the test router: a single protected route guarded by the
/// peer-SPIFFE-ID gate. A second middleware (`inject_peer_extension`)
/// stands in for the rustls TLS layer in production — it places a
/// `PeerSpiffeId` extension on the request from the
/// `X-Test-Peer-Spiffe-Id` header when the header is present.
fn build_app(state: PeerSpiffeGateState) -> Router {
    Router::new()
        .route("/protected", get(protected_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            peer_spiffe_gate_middleware,
        ))
        // The inject-peer middleware MUST be outside the gate so the
        // extension lands BEFORE the gate runs. In production the
        // rustls TLS layer plays this role.
        .layer(axum::middleware::from_fn(inject_peer_extension))
}

async fn protected_handler(
    peer: Option<Extension<PeerSpiffeId>>,
) -> Response {
    match peer {
        Some(Extension(p)) => {
            (StatusCode::OK, format!("hello, {}", p.as_str())).into_response()
        }
        None => (StatusCode::OK, "hello, anon").into_response(),
    }
}

async fn inject_peer_extension(mut req: Request<Body>, next: Next) -> Response {
    if let Some(hdr) = req.headers().get("x-test-peer-spiffe-id") {
        if let Ok(s) = hdr.to_str() {
            if let Ok(id) = SpiffeId::parse(s) {
                req.extensions_mut().insert(PeerSpiffeId(id));
            }
        }
    }
    next.run(req).await
}

fn state(expected: &str) -> PeerSpiffeGateState {
    let registry = Registry::new();
    let metrics =
        Arc::new(SpiffeMetrics::register(&registry).expect("register spiffe metrics"));
    PeerSpiffeGateState {
        expected: expected.to_string(),
        metrics,
    }
}

#[tokio::test]
async fn matching_peer_passes_the_gate() {
    let app = build_app(state("spiffe://recor.cm/declaration"));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("x-test-peer-spiffe-id", "spiffe://recor.cm/declaration")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn mismatched_peer_is_refused_with_403() {
    // The audit's worst-case scenario: a peer presents a valid SPIFFE
    // ID but it's not the one we expect. The gate MUST refuse.
    let app = build_app(state("spiffe://recor.cm/declaration"));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("x-test-peer-spiffe-id", "spiffe://recor.cm/attacker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn missing_peer_extension_is_refused_with_403() {
    // No `PeerSpiffeId` extension at all — equivalent to a TLS layer
    // that failed to surface a verified peer cert. The gate must
    // still refuse, never "silently accept" (the failure mode the
    // audit explicitly calls out).
    let app = build_app(state("spiffe://recor.cm/declaration"));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn malformed_peer_spiffe_id_is_refused_with_403() {
    // A garbage value in the test header → SpiffeId::parse fails →
    // no extension injected → gate refuses. Production equivalent:
    // the rustls layer surfaced a peer cert whose URI SAN is not a
    // valid SPIFFE ID.
    let app = build_app(state("spiffe://recor.cm/declaration"));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("x-test-peer-spiffe-id", "not-a-spiffe-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn denied_counter_increments_on_each_refusal() {
    // Build a state we control so we can read its metrics afterwards.
    let registry = Registry::new();
    let metrics =
        Arc::new(SpiffeMetrics::register(&registry).expect("register spiffe metrics"));
    let gate_state = PeerSpiffeGateState {
        expected: "spiffe://recor.cm/declaration".to_string(),
        metrics: metrics.clone(),
    };
    let app = Router::new()
        .route("/protected", get(protected_handler))
        .layer(axum::middleware::from_fn_with_state(
            gate_state,
            peer_spiffe_gate_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_peer_extension));

    // Three refused requests: mismatch, missing, mismatch again.
    for header in [
        Some("spiffe://recor.cm/attacker"),
        None,
        Some("spiffe://recor.cm/other-attacker"),
    ] {
        let mut req = Request::builder().uri("/protected");
        if let Some(h) = header {
            req = req.header("x-test-peer-spiffe-id", h);
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .expect("router responds");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // The `denied` AND `missing` counters between them must equal 3.
    let families = registry.gather();
    let counter = families
        .iter()
        .find(|f| f.name() == "recor_spiffe_peer_verify_total")
        .expect("counter family present");
    let refusal_total: u64 = counter
        .get_metric()
        .iter()
        .filter(|m| {
            m.get_label()
                .iter()
                .any(|l| l.value() == "denied" || l.value() == "missing")
        })
        .map(|m| m.get_counter().value() as u64)
        .sum();
    assert_eq!(
        refusal_total, 3,
        "expected three refusals across the denied/missing labels; got {refusal_total}"
    );
}
