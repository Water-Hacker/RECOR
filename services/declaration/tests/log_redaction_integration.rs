//! Integration check for OPS-2 PII redaction inside the declaration
//! service's test fixtures.
//!
//! Acceptance criterion: `grep "spiffe://recor.cm/declarant/alice"
//! <captured stdout>` returns 0 hits when redaction is enabled, and
//! returns the principal when redaction is disabled-for-dev.
//!
//! We exercise this without standing up Postgres by:
//!   1. Building the same redaction layer + JSON event formatter the
//!      declaration service installs in `observability::init`.
//!   2. Wiring it to an in-memory `Vec<u8>` buffer (the same shape
//!      stdout has; the service uses stdout in production, this test
//!      uses a buffer purely so we can grep).
//!   3. Emitting a span whose fields mirror the
//!      `submit_declaration` instrument macro: `principal`,
//!      `entity_id`, `declarant_principal`, `receipt_hash_hex`.
//!   4. Grepping the resulting buffer.

use std::io::Write;
use std::sync::{Arc, Mutex};

use recor_logging::{RedactingJsonFormat, RedactingLayer, RedactionConfig, RedactionMode};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

const TEST_KEY_HEX: &str = "4242424242424242424242424242424242424242424242424242424242424242";
const TEST_PRINCIPAL: &str = "spiffe://recor.cm/declarant/alice";
const TEST_RECEIPT_HASH: &str =
    "abcd1234567890deadbeefcafebabe1234567890abcdef0123456789abcdef01";

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

fn run_with_mode(mode: RedactionMode) -> String {
    let cfg = RedactionConfig::new(mode, TEST_KEY_HEX).expect("test config");
    let buf = SharedBuf::default();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(buf.clone())
        .event_format(RedactingJsonFormat::new(cfg.clone()));

    let subscriber = tracing_subscriber::registry()
        .with(RedactingLayer::new(cfg))
        .with(fmt_layer);

    tracing::subscriber::with_default(subscriber, || {
        // Mirror submit_declaration's instrument macro fields exactly.
        let span = tracing::info_span!(
            "submit_declaration",
            declaration_id = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80",
            entity_id = "11111111-2222-3333-4444-555555555555",
            declarant_principal = TEST_PRINCIPAL,
            correlation_id = "deadbeef-0000-0000-0000-000000000000",
        );
        let _g = span.enter();
        tracing::info!(
            principal = TEST_PRINCIPAL,
            receipt_hash_hex = TEST_RECEIPT_HASH,
            "declaration submitted"
        );
    });

    buf.dump()
}

#[test]
fn redaction_enabled_strips_spiffe_uri_from_stdout() {
    let out = run_with_mode(RedactionMode::Enabled);
    assert!(
        !out.contains("declarant/alice"),
        "raw subject path leaked: {out}"
    );
    // Acceptance criterion: zero hits for the raw principal under redaction.
    assert!(
        !out.contains(TEST_PRINCIPAL),
        "full raw SPIFFE URI leaked: {out}"
    );
    // The host part is still present (we keep host for correlation).
    assert!(
        out.contains("spiffe://recor.cm/"),
        "host preservation broken: {out}"
    );
}

#[test]
fn redaction_enabled_strips_full_receipt_hash() {
    let out = run_with_mode(RedactionMode::Enabled);
    assert!(
        !out.contains(TEST_RECEIPT_HASH),
        "full receipt hash leaked: {out}"
    );
    assert!(out.contains("abcd1234"), "head missing: {out}");
}

#[test]
fn redaction_enabled_passes_entity_id_through() {
    let out = run_with_mode(RedactionMode::Enabled);
    assert!(
        out.contains("11111111-2222-3333-4444-555555555555"),
        "entity_id wrongly redacted: {out}"
    );
}

#[test]
fn disabled_for_dev_leaks_principal_intentionally() {
    let out = run_with_mode(RedactionMode::DisabledForDev);
    assert!(
        out.contains(TEST_PRINCIPAL),
        "dev passthrough broken: {out}"
    );
}
