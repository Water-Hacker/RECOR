//! Print the OpenAPI 3.1 spec for `services/person-service` to stdout
//! as pretty-printed JSON. Pure: no DB, no network. Used by the
//! drift-check CI step to regenerate the committed snapshot at
//! `docs/openapi/person-service.json` and diff it.

use std::process::ExitCode;

fn main() -> ExitCode {
    let spec = recor_person_service::api::build_openapi();
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
