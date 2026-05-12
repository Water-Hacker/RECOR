//! Tracing + OpenTelemetry initialisation. Console output always on;
//! OTLP export configured when `otlp_endpoint` is non-empty.
//!
//! The PII-redacting layer from `recor-logging` is installed first in
//! the layer stack so its side effects (overwriting span field values
//! in the registry) are visible to the downstream fmt + OTel layers
//! that actually emit log lines.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use secrecy::ExposeSecret;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;

/// Initialise tracing. Returns a guard that, when dropped, flushes
/// remaining spans to the OTLP exporter.
///
/// Fail-closed behaviour (OPS-2):
///   - In non-dev environments, `log_redaction_key` MUST be set when
///     redaction is enabled; missing key → [`ObservabilityError::RedactionKeyRequired`].
///   - A malformed key (not 64 hex chars) is also rejected.
///   - Dev fallback: missing key auto-generates a random one and
///     emits a `warn!` so the operator notices.
pub fn init(cfg: &Config) -> Result<TracingGuard, ObservabilityError> {
    let filter = EnvFilter::try_new(&cfg.log_filter)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Build redaction config from the typed service config (not from
    // env directly — the service Config layer is the single source of
    // truth, OPS-2 wires it through).
    let redaction = build_redaction(cfg)?;

    // The format layer emits redacted JSON to stdout; the registry
    // layer makes the redacted span field values available to any
    // downstream consumer that looks at the registry's span extensions
    // (e.g. the OTLP exporter below).
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
        .build()
        .map_err(ObservabilityError::Otlp)?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    let tracer = provider.tracer("recor-declaration");
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
        // Dev or disabled-* mode: generate a random key. Defer to the
        // crate's env-loading shim by constructing a RedactionConfig
        // directly with a fresh random hex string.
        let random = generate_dev_key();
        return recor_logging::RedactionConfig::new(mode, &random)
            .map_err(ObservabilityError::Redaction);
    }

    recor_logging::RedactionConfig::new(mode, key_hex).map_err(ObservabilityError::Redaction)
}

/// Build a random 64-hex key for dev fallback. The key is never used
/// as a secret in any security-critical sense — only as a stable
/// per-process MAC key for log correlation.
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
            tracing::info!(
                event = "log_redaction_enabled",
                environment = %cfg.environment,
                "PII redaction active on tracing spans"
            );
        }
        recor_logging::RedactionMode::DisabledForDev => {
            tracing::info!(
                event = "log_redaction_disabled_for_dev",
                environment = %cfg.environment,
                "PII redaction disabled (dev) — span field values pass through"
            );
        }
        recor_logging::RedactionMode::Disabled => {
            tracing::warn!(
                event = "log_redaction_disabled_explicit",
                environment = %cfg.environment,
                "LOG_REDACTION=disabled — PII values WILL appear in logs; do not use in production"
            );
        }
    }
    // Surface dev key fallback explicitly so the operator notices.
    if cfg.log_redaction_key.expose_secret().is_empty() && is_dev {
        tracing::warn!(
            event = "log_redaction_dev_key_generated",
            "LOG_REDACTION_KEY not set; generated a random per-process key (rotates each restart)"
        );
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("OTLP exporter init failure: {0}")]
    Otlp(#[source] opentelemetry::trace::TraceError),
    #[error("log redaction config invalid: {0}")]
    Redaction(#[source] recor_logging::RedactionConfigError),
    #[error(
        "LOG_REDACTION_KEY is required outside dev (64 hex chars / 32 bytes); refusing to start"
    )]
    RedactionKeyRequired,
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

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn base_cfg() -> Config {
        // Build a minimal Config via deserialisation so we don't have
        // to keep the field list in sync here.
        let json = serde_json::json!({
            "bind_addr": "0.0.0.0:0",
            "database_url": "postgres://test",
            "environment": "dev",
        });
        serde_json::from_value(json).expect("config minimal")
    }

    #[test]
    fn dev_with_empty_key_uses_fallback() {
        let cfg = base_cfg();
        let r = build_redaction(&cfg).expect("dev fallback");
        assert_eq!(r.mode, recor_logging::RedactionMode::DisabledForDev);
    }

    #[test]
    fn non_dev_enabled_requires_key() {
        let mut cfg = base_cfg();
        cfg.environment = "prod".to_string();
        cfg.log_redaction = "enabled".to_string();
        let err = build_redaction(&cfg).expect_err("must reject");
        assert!(matches!(err, ObservabilityError::RedactionKeyRequired));
    }

    #[test]
    fn non_dev_with_valid_key_succeeds() {
        let mut cfg = base_cfg();
        cfg.environment = "prod".to_string();
        cfg.log_redaction = "enabled".to_string();
        cfg.log_redaction_key = SecretString::from(
            "4242424242424242424242424242424242424242424242424242424242424242".to_string(),
        );
        let r = build_redaction(&cfg).expect("valid prod config");
        assert_eq!(r.mode, recor_logging::RedactionMode::Enabled);
    }

    #[test]
    fn non_dev_disabled_for_dev_does_not_require_key() {
        // Edge case: an operator explicitly sets disabled-for-dev in a
        // non-dev environment. We respect their choice (and the
        // emit_startup_warnings will still scream about it) but we
        // don't refuse to start.
        let mut cfg = base_cfg();
        cfg.environment = "prod".to_string();
        cfg.log_redaction = "disabled-for-dev".to_string();
        let r = build_redaction(&cfg).expect("explicit dev passthrough allowed");
        assert_eq!(r.mode, recor_logging::RedactionMode::DisabledForDev);
    }

    #[test]
    fn malformed_key_rejected() {
        let mut cfg = base_cfg();
        cfg.environment = "prod".to_string();
        cfg.log_redaction = "enabled".to_string();
        cfg.log_redaction_key = SecretString::from("deadbeef".to_string());
        let err = build_redaction(&cfg).expect_err("short key rejected");
        assert!(matches!(err, ObservabilityError::Redaction(_)));
    }
}
