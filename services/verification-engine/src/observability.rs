//! Tracing + OTel initialisation. Mirror of services/declaration.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;

pub fn init(cfg: &Config) -> Result<TracingGuard, opentelemetry::trace::TraceError> {
    let filter = EnvFilter::try_new(&cfg.log_filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().json().with_target(true);
    let registry = tracing_subscriber::registry().with(filter).with(fmt_layer);

    if cfg.otlp_endpoint.is_empty() {
        registry.init();
        return Ok(TracingGuard { provider: None });
    }

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
    let tracer = provider.tracer("recor-verification-engine");
    registry.with(tracing_opentelemetry::layer().with_tracer(tracer)).init();
    Ok(TracingGuard { provider: Some(provider) })
}

pub struct TracingGuard {
    provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(p) = self.provider.take() {
            let _ = p.shutdown();
        }
    }
}
