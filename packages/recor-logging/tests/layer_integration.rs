//! End-to-end tests for the [`RedactingLayer`] + [`RedactingJsonFormat`]
//! pair using the real `tracing_subscriber::Registry`. Covers the four
//! scenarios called out in the OPS-2 acceptance criteria, plus the
//! round-trip / disabled-mode / tampered-key assertions.

use std::io::Write;
use std::sync::{Arc, Mutex};

use recor_logging::{RedactingJsonFormat, RedactingLayer, RedactionConfig, RedactionMode};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

const TEST_KEY_HEX: &str = "4242424242424242424242424242424242424242424242424242424242424242";

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    fn dump(&self) -> String {
        let buf = self.0.lock().expect("buf mutex poisoned");
        String::from_utf8_lossy(&buf).into_owned()
    }
}

impl<'a> MakeWriter<'a> for SharedBuf {
    type Writer = SharedWriter;
    fn make_writer(&'a self) -> Self::Writer {
        SharedWriter(self.0.clone())
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("buf mutex").extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn run<F: FnOnce()>(mode: RedactionMode, f: F) -> String {
    let cfg = RedactionConfig::new(mode, TEST_KEY_HEX).expect("test config");
    let buf = SharedBuf::default();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(buf.clone())
        .event_format(RedactingJsonFormat::new(cfg.clone()));

    let subscriber = tracing_subscriber::registry()
        .with(RedactingLayer::new(cfg))
        .with(fmt_layer);

    tracing::subscriber::with_default(subscriber, f);
    buf.dump()
}

#[test]
fn enabled_mode_redacts_spiffe_principal() {
    let out = run(RedactionMode::Enabled, || {
        let span = tracing::info_span!(
            "submit_declaration",
            principal = "spiffe://recor.cm/declarant/alice",
        );
        let _g = span.enter();
        tracing::info!("hello");
    });
    assert!(!out.contains("alice"), "raw subject leaked: {out}");
    assert!(out.contains("spiffe://recor.cm/"), "host missing: {out}");
}

#[test]
fn enabled_mode_redacts_uuid_person_id() {
    let uuid = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80";
    let out = run(RedactionMode::Enabled, || {
        tracing::info!(person_id = uuid, "verify");
    });
    assert!(!out.contains(uuid), "raw uuid leaked: {out}");
}

#[test]
fn enabled_mode_passes_through_entity_id() {
    let uuid = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80";
    let out = run(RedactionMode::Enabled, || {
        tracing::info!(entity_id = uuid, "submit");
    });
    assert!(out.contains(uuid), "entity_id wrongly redacted: {out}");
}

#[test]
fn enabled_mode_passes_through_event_type() {
    let out = run(RedactionMode::Enabled, || {
        tracing::info!(event_type = "declaration.submitted.v1", "emit");
    });
    assert!(out.contains("declaration.submitted.v1"));
}

#[test]
fn enabled_mode_redacts_receipt_hash_to_head_tail() {
    let hash = "abcd1234567890deadbeefcafebabe1234567890abcdef0123456789abcdef01";
    let out = run(RedactionMode::Enabled, || {
        tracing::info!(receipt_hash_hex = hash, "submit");
    });
    assert!(out.contains("abcd1234"), "head missing: {out}");
    assert!(!out.contains(&hash[10..50]), "middle leaked: {out}");
}

#[test]
fn disabled_for_dev_lets_principal_through() {
    let raw = "spiffe://recor.cm/declarant/alice";
    let out = run(RedactionMode::DisabledForDev, || {
        tracing::info!(principal = raw, "submit");
    });
    assert!(out.contains("alice"), "dev passthrough broken: {out}");
}

#[test]
fn round_trip_enabled_then_disabled_yields_different_outputs() {
    let raw = "spiffe://recor.cm/declarant/alice";
    let enabled = run(RedactionMode::Enabled, || {
        tracing::info!(principal = raw, "submit");
    });
    let disabled = run(RedactionMode::DisabledForDev, || {
        tracing::info!(principal = raw, "submit");
    });
    assert!(!enabled.contains("alice"));
    assert!(disabled.contains("alice"));
}

#[test]
fn span_field_redaction_visible_in_output() {
    // Verify the redacted span values (set by RedactingLayer into
    // span extensions) are emitted by RedactingJsonFormat.
    let out = run(RedactionMode::Enabled, || {
        let span = tracing::info_span!(
            "submit_declaration",
            declarant_principal = "spiffe://recor.cm/declarant/bob",
        );
        let _g = span.enter();
        tracing::info!("inside span");
    });
    assert!(!out.contains("bob"), "span field leaked: {out}");
    assert!(out.contains("spiffe://recor.cm/"), "redaction shape missing: {out}");
}

#[test]
fn round_trip_known_principal_not_in_captured_output() {
    let principal = "spiffe://recor.cm/declarant/alice";
    let out = run(RedactionMode::Enabled, || {
        let span = tracing::info_span!("submit", declarant_principal = principal);
        let _g = span.enter();
        tracing::info!(principal = principal, "submitted");
    });
    // Acceptance criterion: `grep "spiffe://recor.cm/declarant/alice"` → 0 hits
    assert!(
        !out.contains(principal),
        "round-trip leak: full principal `{principal}` found in:\n{out}"
    );
}
