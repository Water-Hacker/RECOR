//! Deterministic fixture responses for the "no API key" path.
//!
//! D14 fail-closed: when `ANTHROPIC_API_KEY` is unset, the gateway
//! returns this fixture instead of crashing or pretending to call out.
//! Stage 5 maps the fixture to a vacuous BPA, so the pipeline still
//! adjudicates but records the gap explicitly.

use serde_json::{Value, json};

use crate::prompt::{StructuredResponse, ToolSchema};

#[derive(Debug, Clone)]
pub struct FixtureResponse {
    pub purpose: String,
    pub model: String,
}

impl FixtureResponse {
    pub fn vacuous(purpose: &str, model: &str) -> Self {
        Self {
            purpose: purpose.to_string(),
            model: model.to_string(),
        }
    }

    /// Build a `StructuredResponse` from this fixture. The fixture's
    /// `tool_input` carries the conventional "insufficient_evidence"
    /// verdict; the caller's schema validation will treat it the same
    /// way it would a real response missing a hit.
    pub fn into_structured(self, _schema: &ToolSchema) -> StructuredResponse {
        let tool_input: Value = json!({
            "verdict": "insufficient_evidence",
            "confidence": 0.0,
            "evidence_citations": [],
            "rationale": format!(
                "fixture-mode response: ANTHROPIC_API_KEY unset for purpose `{}`",
                self.purpose
            ),
        });
        StructuredResponse {
            tool_input,
            stop_reason: "fixture".to_string(),
            usage_input_tokens: Some(0),
            usage_output_tokens: Some(0),
            model: self.model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_deterministic() {
        let schema = ToolSchema {
            name: "x",
            description: "x",
            json_schema: json!({"type": "object"}),
        };
        let a = FixtureResponse::vacuous("p", "m").into_structured(&schema);
        let b = FixtureResponse::vacuous("p", "m").into_structured(&schema);
        assert_eq!(a.tool_input, b.tool_input);
    }

    #[test]
    fn fixture_verdict_is_insufficient() {
        let schema = ToolSchema {
            name: "x",
            description: "x",
            json_schema: json!({"type": "object"}),
        };
        let r = FixtureResponse::vacuous("p", "m").into_structured(&schema);
        assert_eq!(r.tool_input["verdict"], "insufficient_evidence");
        assert_eq!(r.tool_input["confidence"], 0.0);
    }
}
