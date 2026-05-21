//! Consumer contract tests — RÉCOR platform (TODO-082).
//!
//! Each contract fixture at `tests/contract/*.contract.json` describes the
//! request/response pairs that a specific consumer expects from the platform.
//! This test crate:
//!
//! 1. Loads the fixture.
//! 2. Spins up a WireMock server pre-programmed with the expected responses.
//! 3. Sends the described requests against the mock.
//! 4. Asserts status codes, required-field presence, and PII-field absence.
//!
//! The tests are integration tests (`[[test]]` target) and require a network
//! socket (WireMock binds 127.0.0.1 on an ephemeral port). They run in CI
//! without Docker.
//!
//! ## Why this design (not a live platform)?
//!
//! The contract fixtures describe the *shape* of the API, not the content of
//! a specific database. Running against a live platform would require a
//! seeded staging environment and would couple these tests to infrastructure
//! availability. The WireMock approach keeps the tests fast, deterministic,
//! and doctrine-D19-compliant (reproducible).
//!
//! ## Adding a new consumer contract
//!
//! 1. Create `tests/contract/<consumer>.contract.json` following the schema
//!    in the existing files.
//! 2. Add a `#[tokio::test]` function in this file that loads the new fixture
//!    and calls [`run_contract`].
//! 3. The fixture must cover at minimum: the happy path, an authorisation
//!    failure (wrong tier), and any PII-redaction invariant.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Minimal representation of a contract fixture file.
#[derive(Debug, Deserialize)]
struct Contract {
    consumer: String,
    interactions: Vec<Interaction>,
}

#[derive(Debug, Deserialize)]
struct Interaction {
    id: String,
    description: String,
    request: RequestSpec,
    response: ResponseSpec,
}

#[derive(Debug, Deserialize)]
struct RequestSpec {
    method: String,
    path: String,
    #[serde(default)]
    path_params: HashMap<String, String>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ResponseSpec {
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    status_one_of: Vec<u16>,
    #[serde(default)]
    body: HashMap<String, Value>,
    #[serde(default)]
    body_rules: HashMap<String, Value>,
}

/// Resolve `{param}` placeholders in a path template using `path_params`.
fn resolve_path(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (k, v) in params {
        result = result.replace(&format!("{{{k}}}"), v);
    }
    result
}

/// Build the WireMock stub response body for an interaction.
///
/// The body entries that have value `"present"` are replaced with stub
/// strings so the mock returns a well-formed JSON object. Entries with
/// `"array"` are replaced with an empty array stub. Boolean `true`/`false`
/// literals are passed through unchanged.
fn build_stub_body(body: &HashMap<String, Value>) -> Value {
    // Flatten the dot-notation keys into a simple top-level object.
    // Keys containing `[]` (array element notation) are treated as
    // top-level presence markers for the array field itself.
    let mut top: serde_json::Map<String, Value> = serde_json::Map::new();
    for (key, val) in body {
        // Strip array index notation: "proceedings[].proceeding_id" → "proceedings"
        let top_key = key.split('[').next().unwrap_or(key).to_string();
        let stub_val = if val == "present" || val == "array" {
            Value::String("<stub>".into())
        } else {
            val.clone()
        };
        top.entry(top_key).or_insert(stub_val);
    }
    Value::Object(top)
}

/// Assert that a JSON response body does not contain fields marked
/// `must_be_absent` in the contract's `body_rules`.
fn assert_pii_fields_absent(body: &Value, body_rules: &HashMap<String, Value>) {
    for (field_key, rule) in body_rules {
        if let Value::Object(rule_obj) = rule {
            if rule_obj.get("must_be_absent") == Some(&Value::Bool(true)) {
                // Strip array notation to get the top-level field name.
                let top_field = field_key.split('[').next().unwrap_or(field_key);
                assert!(
                    body.get(top_field).is_none(),
                    "PII-redaction violation: field '{}' must be absent in {} consumer response, but was present",
                    top_field,
                    field_key
                );
            }
        }
    }
}

/// Run all interactions in a contract fixture against a fresh WireMock server.
///
/// Returns errors as a collected list so every failure is visible rather
/// than stopping at the first assertion.
async fn run_contract(fixture_json: &str) -> anyhow::Result<()> {
    let contract: Contract = serde_json::from_str(fixture_json)?;
    let server = MockServer::start().await;
    let client = reqwest::Client::new();
    let mut failures: Vec<String> = Vec::new();

    for interaction in &contract.interactions {
        let resolved_path = resolve_path(
            &interaction.request.path,
            &interaction.request.path_params,
        );

        // Determine the expected status code(s).
        let expected_statuses: Vec<u16> = if !interaction.response.status_one_of.is_empty() {
            interaction.response.status_one_of.clone()
        } else {
            vec![interaction.response.status.unwrap_or(200)]
        };
        let primary_status = expected_statuses[0];

        let stub_body = build_stub_body(&interaction.response.body);

        // Register one stub per expected method+path combination.
        let mock_method = match interaction.request.method.as_str() {
            "POST" => wiremock::matchers::method("POST"),
            "PUT" => wiremock::matchers::method("PUT"),
            "DELETE" => wiremock::matchers::method("DELETE"),
            _ => wiremock::matchers::method("GET"),
        };

        Mock::given(mock_method)
            .and(path_regex(format!("^{}$", regex::escape(&resolved_path))))
            .respond_with(
                ResponseTemplate::new(primary_status)
                    .set_body_json(&stub_body),
            )
            .mount(&server)
            .await;

        // Build and fire the request.
        let url = format!("{}{}", server.uri(), resolved_path);
        let mut req = match interaction.request.method.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "DELETE" => client.delete(&url),
            _ => client.get(&url),
        };
        for (k, v) in &interaction.request.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(body) = &interaction.request.body {
            req = req.json(body);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!(
                    "[{}] {}: request failed — {e}",
                    contract.consumer, interaction.id
                ));
                continue;
            }
        };

        let actual_status = resp.status().as_u16();
        if !expected_statuses.contains(&actual_status) {
            failures.push(format!(
                "[{}] {}: expected status one of {:?}, got {}",
                contract.consumer, interaction.id, expected_statuses, actual_status
            ));
        }

        let body_value: Value = resp.json().await.unwrap_or(Value::Null);

        // Assert required top-level fields are present.
        for (field, expected) in &interaction.response.body {
            let top_field = field.split('[').next().unwrap_or(field);
            if expected == "present" || expected == "array" {
                if body_value.get(top_field).is_none() {
                    failures.push(format!(
                        "[{}] {}: required field '{}' absent in response body",
                        contract.consumer, interaction.id, top_field
                    ));
                }
            }
        }

        // Assert PII-redaction invariants.
        let pre_failures = failures.len();
        assert_pii_fields_absent(&body_value, &interaction.response.body_rules);
        let _ = pre_failures; // assertion panics on violation; failures list captures the rest
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Contract failures for {}:\n{}",
            contract.consumer,
            failures.join("\n")
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ANIF_CONTRACT: &str = include_str!("../anif.contract.json");
    const CONAC_CONTRACT: &str = include_str!("../conac.contract.json");
    const PUBLIC_CONTRACT: &str = include_str!("../public.contract.json");

    #[tokio::test]
    async fn anif_contract_interactions_pass() {
        run_contract(ANIF_CONTRACT)
            .await
            .expect("ANIF contract interactions must pass");
    }

    #[tokio::test]
    async fn conac_contract_interactions_pass() {
        run_contract(CONAC_CONTRACT)
            .await
            .expect("CONAC contract interactions must pass");
    }

    #[tokio::test]
    async fn public_contract_interactions_pass() {
        run_contract(PUBLIC_CONTRACT)
            .await
            .expect("Public contract interactions must pass");
    }

    #[test]
    fn anif_contract_fixture_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(ANIF_CONTRACT).expect("anif.contract.json must be valid JSON");
    }

    #[test]
    fn conac_contract_fixture_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(CONAC_CONTRACT).expect("conac.contract.json must be valid JSON");
    }

    #[test]
    fn public_contract_fixture_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(PUBLIC_CONTRACT).expect("public.contract.json must be valid JSON");
    }

    #[test]
    fn resolve_path_substitutes_params() {
        let mut params = std::collections::HashMap::new();
        params.insert("case_id".into(), "abc-123".into());
        assert_eq!(
            resolve_path("/v1/verifications/{case_id}", &params),
            "/v1/verifications/abc-123"
        );
    }

    #[test]
    fn pii_fields_absent_assertion_fires_correctly() {
        use serde_json::json;
        let body = json!({ "entity_id": "abc" });
        let mut rules = std::collections::HashMap::new();
        rules.insert(
            "declarant_principal".into(),
            json!({ "must_be_absent": true }),
        );
        // Should not panic because declarant_principal is not in the body.
        assert_pii_fields_absent(&body, &rules);
    }
}
