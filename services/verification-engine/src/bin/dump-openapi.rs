//! Print the OpenAPI 3.1 spec for `services/verification-engine` to
//! stdout as pretty-printed JSON. Pure: no DB, no network. Used by
//! `tools/ci/check-openapi-drift.sh` to regenerate the committed
//! snapshot and diff it against `docs/openapi/verification-engine.json`.
//!
//! Usage:
//!   cargo run -p recor-verification-engine \
//!     --bin dump-openapi-verification-engine --quiet
//!
//! The output trails with a single newline so the file round-trips
//! cleanly under `git diff --check`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let spec = recor_verification_engine::api::build_openapi();
    match serde_json::to_string_pretty(&spec) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dump-openapi: failed to serialise spec: {e}");
            ExitCode::FAILURE
        }
    }
}
