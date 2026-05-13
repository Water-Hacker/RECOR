//! OBS-1 — Prometheus collectors owned by `recor-spiffe`.
//!
//! Two counters, both labelled by `result`:
//!
//! - `recor_spiffe_svid_fetch_total{result=success|failure|mismatch}` —
//!   incremented in [`crate::SpiffeClient::bootstrap`] for every
//!   Workload API call. `mismatch` is the "agent issued a different
//!   SPIFFE ID than I asked for" case.
//! - `recor_spiffe_peer_verify_total{result=success|missing|denied|malformed}` —
//!   incremented by the tower middleware in
//!   [`crate::mtls_middleware`] for every inbound TLS connection that
//!   carries a peer certificate. `denied` is the "verified peer is not
//!   in the allowlist" outcome — the D17 zero-trust gate.
//!
//! Both metrics are registered against a Prometheus `Registry` passed
//! by the host service (typically the per-service `Metrics` registry).
//! Cardinality is bounded by a compiled-in label table — D18.

use prometheus::{IntCounterVec, Opts, Registry};

/// Bundle of the two SPIFFE-related counters. Construct once at
/// startup and pass to [`crate::SpiffeClient::new`] +
/// [`crate::mtls_middleware`].
#[derive(Clone)]
pub struct SpiffeMetrics {
    /// `recor_spiffe_svid_fetch_total{result}` — labelled by:
    /// - `success`  — agent returned an SVID we accepted
    /// - `failure`  — agent unreachable / returned malformed material
    /// - `mismatch` — agent returned an SVID for the wrong SPIFFE ID
    pub svid_fetch_total: IntCounterVec,
    /// `recor_spiffe_peer_verify_total{result}` — labelled by:
    /// - `success`   — peer presented a verified SPIFFE ID in the allowlist
    /// - `missing`   — peer presented no SAN (no SPIFFE ID extractable)
    /// - `malformed` — peer's URI SAN did not parse as a SPIFFE ID
    /// - `denied`    — peer SPIFFE ID not in the allowlist
    pub peer_verify_total: IntCounterVec,
}

impl SpiffeMetrics {
    /// Register both counters against `registry`. Fails only if the
    /// registry already has a collector with one of these names —
    /// which would indicate a programming error in the consumer.
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let svid_fetch_total = IntCounterVec::new(
            Opts::new(
                "recor_spiffe_svid_fetch_total",
                "SPIFFE Workload API SVID-fetch outcomes. result=success: agent issued an SVID we accepted; result=failure: agent unreachable / malformed payload; result=mismatch: agent issued an SVID for a different SPIFFE ID than this workload requested.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(svid_fetch_total.clone()))?;

        let peer_verify_total = IntCounterVec::new(
            Opts::new(
                "recor_spiffe_peer_verify_total",
                "Per-connection inbound peer-SPIFFE-ID verification outcomes. result=success: peer presented an allowlisted SPIFFE ID; result=missing: peer presented no URI SAN; result=malformed: URI SAN did not parse as a SPIFFE ID; result=denied: SPIFFE ID parsed but was not in the allowlist.",
            ),
            &["result"],
        )?;
        registry.register(Box::new(peer_verify_total.clone()))?;

        Ok(Self {
            svid_fetch_total,
            peer_verify_total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_both_counters() {
        let r = Registry::new();
        let m = SpiffeMetrics::register(&r).expect("registers");
        m.svid_fetch_total.with_label_values(&["success"]).inc();
        m.peer_verify_total.with_label_values(&["denied"]).inc();
        let families = r.gather();
        let names: Vec<_> = families.iter().map(|f| f.name().to_string()).collect();
        assert!(names.contains(&"recor_spiffe_svid_fetch_total".to_string()));
        assert!(names.contains(&"recor_spiffe_peer_verify_total".to_string()));
    }
}
