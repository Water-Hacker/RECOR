//! TODO-059 — `cargo fuzz` target for `SubmitDeclarationRequest`.
//!
//! Fuzzes the public JSON deserialisation entry point for
//! `SubmitDeclarationRequest` — the outer request body the REST API accepts
//! on `POST /v1/declarations`. Any input that causes a panic, an
//! out-of-bounds read, a stack overflow, or a hang (libFuzzer detects all
//! four via sanitisers + a timeout) will be captured as a crash.
//!
//! This target deliberately does NOT attempt a full service integration
//! (no DB, no tokio runtime). It exercises only the serde + validation
//! boundary — the layer most reachable from an unauthenticated HTTP path.
//!
//! ## Running locally (requires Rust nightly + cargo-fuzz)
//!
//!   cargo +nightly fuzz run canonical_payload_deserialise \
//!     -- -max_total_time=300
//!
//! ## CI job (see .github/workflows/fuzz.yml or the Justfile target)
//!
//!   cargo +nightly fuzz run canonical_payload_deserialise \
//!     -- -max_total_time=300 -jobs=2
//!
//! A 5-minute budget (`-max_total_time=300`) covers ≥1 M iterations on a
//! typical CI runner given the target's throughput (~4 k exec/s).
//!
//! ## Corpus
//!
//! A seed corpus lives alongside this file in
//! `fuzz/corpus/canonical_payload_deserialise/`. Valid minimal inputs
//! accelerate coverage discovery on the first run.

#![no_main]

use libfuzzer_sys::fuzz_target;

// The target we are fuzzing: the DTO that handles inbound JSON.
use recor_declaration::api::dto::SubmitDeclarationRequest;

fuzz_target!(|data: &[u8]| {
    // Guard 1: libFuzzer can generate inputs larger than any realistic
    // HTTP body. Discard very large inputs early so the target stays
    // fast enough for 1 M iterations in 5 minutes.
    if data.len() > 65_536 {
        return;
    }

    // Guard 2: inputs that are not valid UTF-8 will fail at the JSON
    // layer; we still let serde see them (it converts from a byte slice
    // internally) to exercise the codec boundary without panicking.
    let _ = serde_json::from_slice::<SubmitDeclarationRequest>(data);

    // Guard 3: try treating the bytes as a UTF-8 string first so
    // libFuzzer can generate human-readable mutations more easily.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<SubmitDeclarationRequest>(s);
    }

    // Guard 4: also exercise the domain-level `into_command_strict`
    // path for any request body that deserialises successfully.
    if let Ok(req) = serde_json::from_slice::<SubmitDeclarationRequest>(data) {
        let principal = "spiffe://recor.cm/fuzz-principal";
        let correlation_id = uuid::Uuid::nil();
        // The method may return Ok or Err; we only care that it
        // does not panic or abort.
        let _ = req.into_command_strict(principal.to_string(), correlation_id);
    }
});
