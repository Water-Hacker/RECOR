//! OBS-1 Prometheus metrics for the worker.
//!
//! Exposed labels follow the OBS-1 bounded-enum-label policy
//! (no high-cardinality values). Histogram buckets target sub-second
//! Fabric round-trip latencies — the audit channel is in-region.

use std::sync::Arc;

use fabric_bridge::BridgeMetrics;
use prometheus::{
    register_counter_vec_with_registry, register_histogram_with_registry, CounterVec, Histogram,
    HistogramOpts, Registry, TextEncoder,
};

#[derive(Debug)]
pub struct WorkerMetrics {
    pub registry: Registry,
    pub anchor_total: CounterVec,
    pub anchor_latency_seconds: Histogram,
    pub dlq_writes_total: CounterVec,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        let registry = Registry::new_custom(Some("worker_fabric_bridge".into()), None)
            .expect("registry");

        let anchor_total = register_counter_vec_with_registry!(
            "recor_fabric_anchor_total",
            "Number of Fabric anchor attempts, labelled by terminal result.",
            &["result"],
            registry,
        )
        .expect("metric register");

        let anchor_latency_seconds = register_histogram_with_registry!(
            HistogramOpts::new(
                "recor_fabric_anchor_latency_seconds",
                "Latency of a single Fabric anchor attempt (Bridge → Gateway shim → orderer)."
            )
            .buckets(vec![
                0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
            ]),
            registry,
        )
        .expect("metric register");

        let dlq_writes_total = register_counter_vec_with_registry!(
            "recor_fabric_dlq_writes_total",
            "Rows written to fabric_bridge_dlq, labelled by failure cause.",
            &["cause"],
            registry,
        )
        .expect("metric register");

        // Pre-initialise label values so they appear in /metrics output
        // even before the first observation. Operators rely on these
        // series existing for "absence of metric" alerting.
        for result in ["committed", "already_committed", "retried", "permanent_failure"] {
            anchor_total.with_label_values(&[result]);
        }
        for cause in ["permanent", "non_retryable", "config"] {
            dlq_writes_total.with_label_values(&[cause]);
        }

        Self {
            registry,
            anchor_total,
            anchor_latency_seconds,
            dlq_writes_total,
        }
    }

    /// Encode the registry to Prometheus text exposition format.
    pub fn encode_text(&self) -> Result<String, String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder
            .encode_to_string(&metric_families)
            .map_err(|e| e.to_string())
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter so the fabric-bridge crate can call into our metrics without
/// depending on `prometheus` directly.
#[derive(Debug)]
pub struct BridgeMetricsAdapter {
    inner: Arc<WorkerMetrics>,
}

impl BridgeMetricsAdapter {
    pub fn new(inner: Arc<WorkerMetrics>) -> Self {
        Self { inner }
    }
}

impl BridgeMetrics for BridgeMetricsAdapter {
    fn record_attempt(&self, result_label: &str, latency_seconds: f64) {
        self.inner
            .anchor_total
            .with_label_values(&[result_label])
            .inc();
        self.inner
            .anchor_latency_seconds
            .observe(latency_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_encode_to_prometheus_text() {
        let m = WorkerMetrics::new();
        m.anchor_total.with_label_values(&["committed"]).inc();
        let text = m.encode_text().unwrap();
        assert!(text.contains("recor_fabric_anchor_total"));
        assert!(text.contains("result=\"committed\""));
    }

    #[test]
    fn adapter_routes_to_correct_label() {
        let m = Arc::new(WorkerMetrics::new());
        let adapter = BridgeMetricsAdapter::new(m.clone());
        adapter.record_attempt("retried", 0.123);
        adapter.record_attempt("committed", 0.05);
        let text = m.encode_text().unwrap();
        assert!(text.contains("result=\"retried\""));
        assert!(text.contains("result=\"committed\""));
    }
}
