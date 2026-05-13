//! Tracing + OpenTelemetry initialisation. Console output always on;
//! OTLP export configured when `otlp_endpoint` is non-empty.
//!
//! OPS-2: PII-redacting layer from `recor-logging` is installed first
//! in the layer stack. For the Entity service the redaction posture is
//! more relaxed than for the Declaration / (future) Person services
//! because legal entities are not natural persons — see
//! `docs/compliance/data-classification.md` § entities — but the
//! redaction layer is wired identically so the policy is uniform
//! across services.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use secrecy::ExposeSecret;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;

pub fn init(cfg: &Config) -> Result<TracingGuard, ObservabilityError> {
    let filter = EnvFilter::try_new(&cfg.log_filter).unwrap_or_else(|_| EnvFilter::new("info"));

    let redaction = build_redaction(cfg)?;
    let fmt_layer = tracing_subscriber::fmt::layer()
        .event_format(recor_logging::RedactingJsonFormat::new(redaction.clone()));
    let redacting_layer = recor_logging::RedactingLayer::new(redaction);

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(redacting_layer)
        .with(fmt_layer);

    if cfg.otlp_endpoint.is_empty() {
        registry.init();
        emit_startup_warnings(cfg);
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
        .build()
        .map_err(ObservabilityError::Otlp)?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    let tracer = provider.tracer("recor-entity-service");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    registry.with(otel_layer).init();
    emit_startup_warnings(cfg);
    Ok(TracingGuard {
        provider: Some(provider),
    })
}

fn build_redaction(cfg: &Config) -> Result<recor_logging::RedactionConfig, ObservabilityError> {
    let is_dev = cfg.is_dev();
    let mode_str = cfg.log_redaction.trim();
    let mode = recor_logging::RedactionMode::from_env(
        if mode_str.is_empty() { None } else { Some(mode_str) },
        is_dev,
    )
    .map_err(ObservabilityError::Redaction)?;

    let key_hex = cfg.log_redaction_key.expose_secret();
    if key_hex.is_empty() {
        if !is_dev && mode == recor_logging::RedactionMode::Enabled {
            return Err(ObservabilityError::RedactionKeyRequired);
        }
        let random = generate_dev_key();
        return recor_logging::RedactionConfig::new(mode, &random)
            .map_err(ObservabilityError::Redaction);
    }
    recor_logging::RedactionConfig::new(mode, key_hex).map_err(ObservabilityError::Redaction)
}

fn generate_dev_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let mut seed = Vec::with_capacity(24);
    seed.extend_from_slice(&nanos.to_le_bytes());
    seed.extend_from_slice(&pid.to_le_bytes());
    hex::encode(blake3::hash(&seed).as_bytes())
}

fn emit_startup_warnings(cfg: &Config) {
    let mode_str = cfg.log_redaction.trim();
    let is_dev = cfg.is_dev();
    let resolved = recor_logging::RedactionMode::from_env(
        if mode_str.is_empty() { None } else { Some(mode_str) },
        is_dev,
    )
    .unwrap_or(recor_logging::RedactionMode::Enabled);
    match resolved {
        recor_logging::RedactionMode::Enabled => {
            tracing::info!(event = "log_redaction_enabled", environment = %cfg.environment, "PII redaction active on tracing spans");
        }
        recor_logging::RedactionMode::DisabledForDev => {
            tracing::info!(event = "log_redaction_disabled_for_dev", environment = %cfg.environment, "PII redaction disabled (dev) — span field values pass through");
        }
        recor_logging::RedactionMode::Disabled => {
            tracing::warn!(event = "log_redaction_disabled_explicit", environment = %cfg.environment, "LOG_REDACTION=disabled — values WILL appear in logs; do not use in production");
        }
    }
    if cfg.log_redaction_key.expose_secret().is_empty() && is_dev {
        tracing::warn!(event = "log_redaction_dev_key_generated", "LOG_REDACTION_KEY not set; generated a random per-process key");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("OTLP exporter init failure: {0}")]
    Otlp(#[source] opentelemetry::trace::TraceError),
    #[error("log redaction config invalid: {0}")]
    Redaction(#[source] recor_logging::RedactionConfigError),
    #[error("LOG_REDACTION_KEY is required outside dev (64 hex chars / 32 bytes); refusing to start")]
    RedactionKeyRequired,
}

pub struct TracingGuard {
    provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            let _ = provider.shutdown();
        }
    }
}
