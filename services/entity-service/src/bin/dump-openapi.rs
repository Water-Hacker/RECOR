//! Print the OpenAPI 3.1 spec for `services/entity-service` to stdout
//! as pretty-printed JSON. Pure: no DB, no network. Used by
//! `tools/ci/check-openapi-drift.sh` to regenerate the committed
//! snapshot and diff it against `docs/openapi/entity-service.json`.
//!
//! Usage: `cargo run -p recor-entity-service --bin dump-openapi --quiet`

use std::process::ExitCode;

fn main() -> ExitCode {
    let spec = recor_entity_service::api::build_openapi();
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
