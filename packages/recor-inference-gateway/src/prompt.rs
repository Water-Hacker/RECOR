//! Structured-output schema + response types.
//!
//! Anthropic's tool-use is the chosen mechanism for forcing JSON
//! schema compliance: we declare one tool with `input_schema`, set
//! `tool_choice = { type: tool, name }`, and parse the resulting
//! `tool_use` block.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::GatewayError;

/// A tool schema. Caller declares the name + a JSON-schema-shaped
/// `json_schema` object. The gateway forwards this verbatim to the
/// Anthropic API and validates the returned `input` against it.
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: &'static str,
    pub description: &'static str,
    pub json_schema: Value,
}

/// Parsed, schema-validated response from the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredResponse {
    /// The tool input as a JSON Value. The shape matches `schema.json_schema`.
    pub tool_input: Value,
    pub stop_reason: String,
    pub usage_input_tokens: Option<u64>,
    pub usage_output_tokens: Option<u64>,
    pub model: String,
}

/// Minimal schema-validation helper: ensures every property listed in
/// `schema.json_schema.required` is present in the response. The
/// gateway intentionally does not do full JSON-Schema validation
/// (would require pulling another crate); the small contract we DO
/// check is the one our pipeline depends on (D17 — never trust the
/// model's output without verifying it).
pub(crate) fn validate_against_schema(
    schema: &ToolSchema,
    input: &Value,
) -> Result<(), GatewayError> {
    let object = input.as_object().ok_or_else(|| {
        GatewayError::SchemaValidation(format!(
            "tool {} returned non-object",
            schema.name
        ))
    })?;
    if let Some(required) = schema
        .json_schema
        .get("required")
        .and_then(|r| r.as_array())
    {
        for field in required {
            let Some(name) = field.as_str() else {
                continue;
            };
            if !object.contains_key(name) {
                return Err(GatewayError::SchemaValidation(format!(
                    "tool {} response missing required field `{}`",
                    schema.name, name
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_passes_when_required_fields_present() {
        let schema = ToolSchema {
            name: "test",
            description: "x",
            json_schema: serde_json::json!({
                "type": "object",
                "properties": { "a": { "type": "string" } },
                "required": ["a"]
            }),
        };
        let v = serde_json::json!({"a": "ok"});
        assert!(validate_against_schema(&schema, &v).is_ok());
    }

    #[test]
    fn validate_fails_on_missing_required() {
        let schema = ToolSchema {
            name: "test",
            description: "x",
            json_schema: serde_json::json!({
                "type": "object",
                "required": ["a", "b"]
            }),
        };
        let v = serde_json::json!({"a": 1});
        assert!(matches!(
            validate_against_schema(&schema, &v),
            Err(GatewayError::SchemaValidation(_))
        ));
    }

    #[test]
    fn validate_rejects_non_object() {
        let schema = ToolSchema {
            name: "t",
            description: "x",
            json_schema: serde_json::json!({"type": "object"}),
        };
        let v = serde_json::json!("a string");
        assert!(matches!(
            validate_against_schema(&schema, &v),
            Err(GatewayError::SchemaValidation(_))
        ));
    }
}
