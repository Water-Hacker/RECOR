//! Print the OpenAPI 3.1 spec for `services/declaration` to stdout
//! as pretty-printed JSON. Pure: no DB, no network. Used by
//! `tools/ci/check-openapi-drift.sh` to regenerate the committed
//! snapshot and diff it against `docs/openapi/declaration.json`.
//!
//! Usage: `cargo run -p recor-declaration --bin dump-openapi --quiet`
//!
//! The output trails with a single newline so the file round-trips
//! cleanly under `git diff --check`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let spec = recor_declaration::api::build_openapi();
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
