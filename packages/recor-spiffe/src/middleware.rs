//! Peer-SPIFFE-ID extraction + allowlist gating.
//!
//! ## How this integrates with axum / tower
//!
//! The host wiring puts the TLS-verified peer certificate on the
//! request as an extension; this module provides:
//!
//! 1. [`PeerSpiffeId`] — the marker type the middleware inserts.
//! 2. [`extract_peer_spiffe_id`] — pure function: take a peer
//!    certificate, return either a `PeerSpiffeId` (and bump the
//!    `peer_verify_total{result=success}` counter) or an error (and
//!    bump the matching `missing` / `malformed` counter).
//! 3. [`enforce_peer_id`] — the allowlist gate. Returns `Ok(())` if
//!    the verified peer matches `expected`; otherwise increments
//!    `peer_verify_total{result=denied}` and returns
//!    [`crate::SpiffeError::PeerNotAllowed`].
//!
//! Wiring sketch (host side, `services/declaration/src/main.rs`):
//!
//! ```ignore
//! let tls_cfg = spiffe.server_config("spiffe://recor.cm/declaration").await?;
//! let acceptor = axum_server::tls_rustls::RustlsAcceptor::new(tls_cfg);
//! // ... acceptor pumps peer-cert extension into requests ...
//! let app = router.layer(axum::middleware::from_fn_with_state(
//!     spiffe_metrics.clone(),
//!     |State(metrics): State<_>, mut req: Request, next: Next| async move {
//!         let cert = req
//!             .extensions()
//!             .get::<Arc<rustls::pki_types::CertificateDer<'static>>>()
//!             .cloned();
//!         match extract_peer_spiffe_id(cert.as_deref(), Some(&metrics)) {
//!             Ok(id) => { req.extensions_mut().insert(id); }
//!             Err(_) => { /* metric already bumped; let the handler 403 */ }
//!         }
//!         next.run(req).await
//!     },
//! ));
//! ```
//!
//! ## Why not a `tower::Layer` here?
//!
//! Axum's `Request` / `Response` types are part of the host crate's
//! public surface, not ours. Putting a typed middleware in this
//! crate would force every consumer onto a specific axum/tower
//! version. The pure-function shape leaves the host wiring free.

use crate::{SpiffeError, SpiffeId, SpiffeMetrics};

/// A verified peer SPIFFE ID extracted from a TLS connection's peer
/// certificate. Inserted into `request.extensions` by the host's TLS
/// middleware after [`extract_peer_spiffe_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerSpiffeId(pub SpiffeId);

impl PeerSpiffeId {
    /// Return the inner SPIFFE ID as a string slice (zero-copy).
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Pure helper: extract a peer SPIFFE ID from an optional verified
/// peer certificate (as handed up by the TLS adapter) and increment
/// the matching `peer_verify_total{result=...}` counter.
///
/// Outcomes:
/// - `Some(cert)` + URI SAN parses → `Ok(PeerSpiffeId)` + `success`.
/// - `Some(cert)` but no URI SAN  → `Err(PeerHasNoSpiffeId)` + `missing`.
/// - `Some(cert)` + malformed URI → `Err(PeerHasNoSpiffeId)` + `malformed`.
/// - `None`                        → `Err(PeerHasNoSpiffeId)` + `missing`.
pub fn extract_peer_spiffe_id(
    peer_cert: Option<&rustls::pki_types::CertificateDer<'_>>,
    metrics: Option<&SpiffeMetrics>,
) -> Result<PeerSpiffeId, SpiffeError> {
    let cert = match peer_cert {
        Some(c) => c,
        None => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["missing"])
                    .inc();
            }
            return Err(SpiffeError::PeerHasNoSpiffeId(
                "TLS layer did not surface a peer certificate".into(),
            ));
        }
    };
    match crate::peer_spiffe_id_from_cert(cert) {
        Ok(id) => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["success"])
                    .inc();
            }
            Ok(PeerSpiffeId(id))
        }
        Err(e @ SpiffeError::PeerHasNoSpiffeId(_)) => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["missing"])
                    .inc();
            }
            Err(e)
        }
        Err(e @ SpiffeError::MalformedSvid(_)) => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["malformed"])
                    .inc();
            }
            Err(e)
        }
        Err(e) => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["malformed"])
                    .inc();
            }
            Err(e)
        }
    }
}

/// Drop-in middleware helper. Documentation-only at this point — see
/// the doc comment at the top of this module for the wiring sketch.
///
/// This re-exports [`extract_peer_spiffe_id`] under a name that
/// signals "this is the thing you call from your axum middleware
/// closure"; it has no special behaviour beyond the underlying
/// extractor.
pub fn mtls_middleware(
    peer_cert: Option<&rustls::pki_types::CertificateDer<'_>>,
    metrics: Option<&SpiffeMetrics>,
) -> Result<PeerSpiffeId, SpiffeError> {
    extract_peer_spiffe_id(peer_cert, metrics)
}

/// Enforce that a verified peer SPIFFE ID matches `expected`.
///
/// On a mismatch / missing peer ID this returns
/// `Err(SpiffeError::PeerNotAllowed)` and increments the
/// `peer_verify_total{result=denied}` counter. Callers typically map
/// the error to an HTTP 403 — the application-layer leg of the D17
/// zero-trust gate.
pub fn enforce_peer_id(
    peer: Option<&PeerSpiffeId>,
    expected: &str,
    metrics: Option<&SpiffeMetrics>,
) -> Result<(), SpiffeError> {
    match peer {
        Some(p) if p.0.as_str() == expected => Ok(()),
        Some(p) => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["denied"])
                    .inc();
            }
            Err(SpiffeError::PeerNotAllowed {
                actual: p.0.as_str().to_string(),
                expected: expected.to_string(),
            })
        }
        None => {
            if let Some(m) = metrics {
                m.peer_verify_total
                    .with_label_values(&["missing"])
                    .inc();
            }
            Err(SpiffeError::PeerNotAllowed {
                actual: "<no-peer-svid>".to_string(),
                expected: expected.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn enforce_peer_id_passes_on_match() {
        let id = SpiffeId::parse("spiffe://recor.cm/declaration").unwrap();
        let peer = PeerSpiffeId(id);
        let r = Registry::new();
        let metrics = SpiffeMetrics::register(&r).unwrap();
        assert!(enforce_peer_id(
            Some(&peer),
            "spiffe://recor.cm/declaration",
            Some(&metrics)
        )
        .is_ok());
    }

    #[test]
    fn enforce_peer_id_denies_on_mismatch() {
        let id = SpiffeId::parse("spiffe://recor.cm/portal").unwrap();
        let peer = PeerSpiffeId(id);
        let r = Registry::new();
        let metrics = SpiffeMetrics::register(&r).unwrap();
        let result = enforce_peer_id(
            Some(&peer),
            "spiffe://recor.cm/declaration",
            Some(&metrics),
        );
        match result {
            Err(SpiffeError::PeerNotAllowed { actual, expected }) => {
                assert_eq!(actual, "spiffe://recor.cm/portal");
                assert_eq!(expected, "spiffe://recor.cm/declaration");
            }
            _ => panic!("expected PeerNotAllowed, got {result:?}"),
        }
    }

    #[test]
    fn enforce_peer_id_denies_on_missing_peer() {
        let r = Registry::new();
        let metrics = SpiffeMetrics::register(&r).unwrap();
        let result =
            enforce_peer_id(None, "spiffe://recor.cm/declaration", Some(&metrics));
        match result {
            Err(SpiffeError::PeerNotAllowed { actual, .. }) => {
                assert_eq!(actual, "<no-peer-svid>");
            }
            _ => panic!("expected PeerNotAllowed for missing peer"),
        }
    }

    #[test]
    fn enforce_peer_id_increments_denied_counter() {
        let r = Registry::new();
        let metrics = SpiffeMetrics::register(&r).unwrap();
        let id = SpiffeId::parse("spiffe://recor.cm/portal").unwrap();
        let peer = PeerSpiffeId(id);
        let _ = enforce_peer_id(
            Some(&peer),
            "spiffe://recor.cm/declaration",
            Some(&metrics),
        );
        let families = r.gather();
        let denied = families
            .iter()
            .find(|f| f.name() == "recor_spiffe_peer_verify_total")
            .expect("counter family present");
        let total: u64 = denied
            .get_metric()
            .iter()
            .filter(|m| m.get_label().iter().any(|l| l.value() == "denied"))
            .map(|m| m.get_counter().value() as u64)
            .sum();
        assert_eq!(total, 1, "denied label should have incremented once");
    }

    #[test]
    fn enforce_peer_id_with_no_metrics_works() {
        // Tests outside the host service may not register metrics —
        // the function should still gate correctly without them.
        let id = SpiffeId::parse("spiffe://recor.cm/portal").unwrap();
        let peer = PeerSpiffeId(id);
        let r =
            enforce_peer_id(Some(&peer), "spiffe://recor.cm/declaration", None);
        assert!(matches!(r, Err(SpiffeError::PeerNotAllowed { .. })));
    }

    #[test]
    fn extract_peer_spiffe_id_increments_missing_on_none() {
        let r = Registry::new();
        let metrics = SpiffeMetrics::register(&r).unwrap();
        let result = extract_peer_spiffe_id(None, Some(&metrics));
        assert!(matches!(result, Err(SpiffeError::PeerHasNoSpiffeId(_))));
        let families = r.gather();
        let denied = families
            .iter()
            .find(|f| f.name() == "recor_spiffe_peer_verify_total")
            .expect("counter family present");
        let total: u64 = denied
            .get_metric()
            .iter()
            .filter(|m| m.get_label().iter().any(|l| l.value() == "missing"))
            .map(|m| m.get_counter().value() as u64)
            .sum();
        assert_eq!(total, 1);
    }
}
