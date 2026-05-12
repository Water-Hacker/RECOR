//! Renders a small set of log lines under both `enabled` and
//! `disabled-for-dev` modes so reviewers can eyeball the redaction
//! shape. Used as the "actual sanitised log output" sample in the
//! OPS-2 PR description.
//!
//! Run with:
//!
//!     cargo run -p recor-logging --example sample_output

use std::io::Write;
use std::sync::{Arc, Mutex};

use recor_logging::{RedactingJsonFormat, RedactingLayer, RedactionConfig, RedactionMode};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

const KEY: &str = "4242424242424242424242424242424242424242424242424242424242424242";

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Buf {
    fn drain(&self) -> String {
        let mut g = self.0.lock().unwrap();
        let out = String::from_utf8_lossy(&g).into_owned();
        g.clear();
        out
    }
}

impl<'a> MakeWriter<'a> for Buf {
    type Writer = W;
    fn make_writer(&'a self) -> W {
        W(self.0.clone())
    }
}

struct W(Arc<Mutex<Vec<u8>>>);

impl Write for W {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn emit_under(mode: RedactionMode) -> String {
    let cfg = RedactionConfig::new(mode, KEY).unwrap();
    let buf = Buf::default();
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(buf.clone())
        .event_format(RedactingJsonFormat::new(cfg.clone()));
    let sub = tracing_subscriber::registry()
        .with(RedactingLayer::new(cfg))
        .with(layer);
    tracing::subscriber::with_default(sub, || {
        let span = tracing::info_span!(
            "submit_declaration",
            declaration_id = "9c5b94b1-35ad-49bb-b118-8e8fc24abf80",
            entity_id = "11111111-2222-3333-4444-555555555555",
            declarant_principal = "spiffe://recor.cm/declarant/alice",
            correlation_id = "deadbeef-0000-0000-0000-000000000000",
        );
        let _g = span.enter();
        tracing::info!(
            principal = "spiffe://recor.cm/declarant/alice",
            receipt_hash_hex =
                "abcd1234567890deadbeefcafebabe1234567890abcdef0123456789abcdef01",
            event_type = "declaration.submitted.v1",
            duration_ms = 42,
            "declaration submitted"
        );
        tracing::info!(
            person_id = "7a2c5b94-d5ad-49bb-b118-8e8fc24abf80",
            entity_id = "11111111-2222-3333-4444-555555555555",
            "beneficial owner recorded"
        );
        tracing::warn!(
            principal = "spiffe://recor.cm/operator/charlie",
            "admin override used"
        );
    });
    buf.drain()
}

fn main() {
    println!("=== LOG_REDACTION=enabled ===");
    print!("{}", emit_under(RedactionMode::Enabled));
    println!("\n=== LOG_REDACTION=disabled-for-dev ===");
    print!("{}", emit_under(RedactionMode::DisabledForDev));
}
