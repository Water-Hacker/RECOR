//! Prometheus metrics for the audit reconciler.

use prometheus::{IntCounterVec, IntGauge, Registry};

pub struct ReconcilerMetrics {
    pub registry: Registry,
    /// Number of reconciliation passes by outcome. Labels:
    ///   - `outcome = "ok"`: pass completed, possibly with
    ///     divergences (count separately on `divergence_total`).
    ///   - `outcome = "gateway_error"`: the Fabric gateway returned
    ///     non-success; the pass was aborted (D14 fail-closed).
    ///   - `outcome = "db_error"`: the local event-log query failed.
    pub runs_total: IntCounterVec,
    /// Number of event_ids found in the local event log but absent
    /// from the Fabric chaincode for the given declaration. Labels:
    ///   - `event_type`: one of the declaration event kinds
    ///     (`declaration.submitted.v1`, `.amended.v1`, etc.) so
    ///     operators can tell which event kinds are getting lost.
    /// Each divergence triggers a structured WARN line at the same
    /// instant — the counter is the alertable surface.
    pub divergence_total: IntCounterVec,
    /// Last observed divergence count from the most recent pass.
    /// Mirrors `divergence_total` deltas but as an absolute value so
    /// the dashboard can show "current outstanding divergences"
    /// without a difference query.
    pub last_run_divergence_count: IntGauge,
}

impl ReconcilerMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let runs_total = IntCounterVec::new(
            prometheus::Opts::new(
                "recor_audit_reconciliation_runs_total",
                "Total number of audit-reconciliation passes, by terminal outcome.",
            ),
            &["outcome"],
        )?;
        registry.register(Box::new(runs_total.clone()))?;

        let divergence_total = IntCounterVec::new(
            prometheus::Opts::new(
                "recor_audit_reconciliation_divergence_total",
                "Total number of events found in declaration_events but missing from the Fabric audit channel, by event_type.",
            ),
            &["event_type"],
        )?;
        registry.register(Box::new(divergence_total.clone()))?;

        let last_run_divergence_count = IntGauge::new(
            "recor_audit_reconciliation_last_run_divergence_count",
            "Number of divergences observed on the most recent pass.",
        )?;
        registry.register(Box::new(last_run_divergence_count.clone()))?;

        Ok(Self {
            registry,
            runs_total,
            divergence_total,
            last_run_divergence_count,
        })
    }

    pub fn encode_text(&self) -> Result<String, prometheus::Error> {
        use prometheus::Encoder;
        let mut buf = Vec::new();
        let encoder = prometheus::TextEncoder::new();
        encoder.encode(&self.registry.gather(), &mut buf)?;
        Ok(String::from_utf8(buf).unwrap_or_default())
    }
}
