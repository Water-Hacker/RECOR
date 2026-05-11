//! Tracing + OpenTelemetry initialisation. Console output always on;
//! OTLP export configured when `otlp_endpoint` is non-empty.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;

/// Initialise tracing. Returns a guard that, when dropped, flushes
/// remaining spans to the OTLP exporter.
pub fn init(cfg: &Config) -> Result<TracingGuard, opentelemetry::trace::TraceError> {
    let filter = EnvFilter::try_new(&cfg.log_filter)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(false)
        .with_current_span(true)
        .with_span_list(false);

    let registry = tracing_subscriber::registry().with(filter).with(fmt_layer);

    if cfg.otlp_endpoint.is_empty() {
        registry.init();
        return Ok(TracingGuard { provider: None });
    }

    // Semantic-convention constant names changed across opentelemetry-* minor
    // versions; use the string keys directly to remain stable across crate
    // version bumps. The keys are the OTel resource semantic-convention
    // identifiers.
    let resource = Resource::new(vec![
        KeyValue::new("service.name", cfg.service_name.clone()),
        KeyValue::new("service.namespace", "recor"),
        KeyValue::new("deployment.environment", cfg.environment.clone()),
    ]);

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(cfg.otlp_endpoint.clone())
        .build()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    let tracer = provider.tracer("recor-declaration");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    registry.with(otel_layer).init();
    Ok(TracingGuard {
        provider: Some(provider),
    })
}

pub struct TracingGuard {
    provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            // Best-effort flush; ignore shutdown errors.
            let _ = provider.shutdown();
        }
    }
}
