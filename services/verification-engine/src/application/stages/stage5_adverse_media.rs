//! Stage 5 — Adverse-media screening (R-VER-4).
//!
//! For each beneficial owner:
//!   1. Resolve the person's full name via the same `NameResolver`
//!      used by Stages 3+4.
//!   2. Retrieve top-5 ICIJ Offshore Leaks candidates from
//!      `icij_persons` via the `IcijAdapter`.
//!   3. Pass owner name + entity context + retrieved snippets to the
//!      Inference Gateway with a structured-output schema asking for
//!      a verdict + confidence + evidence citations.
//!   4. Map the verdict + confidence to a BPA contribution.
//!
//! Verdict → BPA mapping:
//!   * verdict = "adverse" + confidence ≥ 0.7 → BPA(0.05, 0.80, 0.15)
//!   * verdict = "adverse" + confidence 0.4-0.7 → BPA(0.15, 0.50, 0.35)
//!   * verdict = "clear"                       → BPA(0.40, 0.05, 0.55)
//!   * verdict = "insufficient_evidence"       → vacuous BPA
//!
//! When the Inference Gateway is in fixture mode (no API key), it
//! returns `insufficient_evidence` deterministically, so this stage
//! emits vacuous BPA without any external call — preserving offline
//! test reproducibility (D14).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tracing::warn;

use recor_inference_gateway::{
    GatewayError, InferenceGateway, StructuredResponse, Tier, ToolSchema,
};

use crate::application::port::{IcijAdapter, IcijCandidate, PersonQuery};
use crate::application::stages::stage3_sanctions::{NameResolver, ResolvedName};
use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome,
    StageOutcomeKind,
};
use crate::metrics::Metrics;

pub struct AdverseMediaStage {
    icij: Arc<dyn IcijAdapter>,
    name_resolver: Arc<dyn NameResolver>,
    gateway: Arc<InferenceGateway>,
    metrics: Option<Arc<Metrics>>,
    tier: Tier,
    max_candidates: usize,
}

impl AdverseMediaStage {
    pub fn new(
        icij: Arc<dyn IcijAdapter>,
        name_resolver: Arc<dyn NameResolver>,
        gateway: Arc<InferenceGateway>,
    ) -> Self {
        Self {
            icij,
            name_resolver,
            gateway,
            metrics: None,
            tier: Tier::A,
            max_candidates: 5,
        }
    }

    pub fn with_metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }

    pub fn with_tier(mut self, t: Tier) -> Self {
        self.tier = t;
        self
    }
}

fn tool_schema() -> ToolSchema {
    ToolSchema {
        name: "adverse_media_verdict",
        description: "Return a structured adverse-media verdict for one person against retrieved evidence snippets.",
        json_schema: json!({
            "type": "object",
            "properties": {
                "verdict": {
                    "type": "string",
                    "enum": ["adverse", "clear", "insufficient_evidence"]
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0
                },
                "evidence_citations": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "rationale": { "type": "string" }
            },
            "required": ["verdict", "confidence", "evidence_citations"]
        }),
    }
}

const SYSTEM_PROMPT: &str = "\
You are RÉCOR's adverse-media reviewer. You receive ONE person's name + the entity \
they are declared as beneficial owner of + retrieved snippets from the ICIJ Offshore \
Leaks database. Decide whether the available evidence supports an adverse-media \
finding against this person. Be conservative: if the evidence is thin or ambiguous, \
return verdict=insufficient_evidence. Cite the snippet text or source you relied on.";

fn build_user_prompt(
    full_name: &str,
    entity_id: uuid::Uuid,
    candidates: &[IcijCandidate],
) -> String {
    let mut buf = format!(
        "Subject: {full_name}\nEntity context: declared beneficial owner of entity {entity_id}\n\n"
    );
    if candidates.is_empty() {
        buf.push_str("No retrieved candidates from the ICIJ index.\n");
    } else {
        buf.push_str("Retrieved candidates from ICIJ Offshore Leaks:\n");
        for (i, c) in candidates.iter().enumerate() {
            buf.push_str(&format!(
                "  [{i}] source_dataset={} node_kind={} country={} similarity={:.2} snippet={}\n",
                c.source_dataset,
                c.node_kind,
                c.country_raw.as_deref().unwrap_or("?"),
                c.similarity,
                c.snippet.as_deref().unwrap_or("(no snippet)"),
            ));
        }
    }
    buf.push_str(
        "\nReturn the structured verdict via the `adverse_media_verdict` tool. \
         If the candidates are weak or absent, prefer `insufficient_evidence`.",
    );
    buf
}

#[async_trait]
impl Stage for AdverseMediaStage {
    fn id(&self) -> StageId {
        StageId::AdverseMedia
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();
        let mut per_owner: Vec<PerOwnerAdverse> = Vec::with_capacity(declaration.beneficial_owners.len());
        let mut worst = Verdict::Unknown;
        let mut worst_confidence: f64 = 0.0;
        let mut any_error = false;
        let schema = tool_schema();

        for owner in &declaration.beneficial_owners {
            let resolved = match self.name_resolver.resolve(owner.person_id).await {
                Some(r) => r,
                None => {
                    per_owner.push(PerOwnerAdverse {
                        person_id: owner.person_id,
                        full_name: None,
                        candidates: vec![],
                        verdict: "insufficient_evidence".into(),
                        confidence: 0.0,
                        rationale: Some("name not resolved".into()),
                        evidence_citations: vec![],
                        error: None,
                    });
                    continue;
                }
            };
            let query = PersonQuery {
                person_id: owner.person_id,
                full_name: resolved.full_name.clone(),
                nationality: resolved.nationality.clone(),
                date_of_birth: resolved.date_of_birth,
            };
            let candidates = match self.icij.retrieve(&query, self.max_candidates).await {
                Ok(c) => c,
                Err(e) => {
                    any_error = true;
                    let msg = e.to_string();
                    warn!(error = %msg, "ICIJ retrieve failed");
                    per_owner.push(PerOwnerAdverse {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        candidates: vec![],
                        verdict: "insufficient_evidence".into(),
                        confidence: 0.0,
                        rationale: None,
                        evidence_citations: vec![],
                        error: Some(msg),
                    });
                    continue;
                }
            };

            let prompt = build_user_prompt(&resolved.full_name, declaration.entity_id, &candidates);
            let model_response = self
                .gateway
                .messages("adverse_media", self.tier, SYSTEM_PROMPT, &prompt, &schema)
                .await;
            match model_response {
                Ok(resp) => {
                    let parsed = parse_response(&resp);
                    let promote = match (worst, parsed.verdict) {
                        (Verdict::Unknown, _) => true,
                        (Verdict::Clear, Verdict::Adverse) => true,
                        (Verdict::Clear, Verdict::Clear) => parsed.confidence > worst_confidence,
                        (Verdict::Adverse, Verdict::Adverse) => parsed.confidence > worst_confidence,
                        _ => false,
                    };
                    if promote {
                        worst = parsed.verdict;
                        worst_confidence = parsed.confidence;
                    }
                    per_owner.push(PerOwnerAdverse {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        candidates,
                        verdict: parsed.verdict.as_str().to_string(),
                        confidence: parsed.confidence,
                        rationale: parsed.rationale.clone(),
                        evidence_citations: parsed.evidence_citations.clone(),
                        error: None,
                    });
                }
                Err(e) => {
                    any_error = true;
                    let msg = e_msg(&e);
                    warn!(error = %msg, "inference gateway error");
                    per_owner.push(PerOwnerAdverse {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        candidates,
                        verdict: "insufficient_evidence".into(),
                        confidence: 0.0,
                        rationale: None,
                        evidence_citations: vec![],
                        error: Some(msg),
                    });
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let (kind, authenticity_bpa, risk_bpa, result_label) = bpa_for(worst, worst_confidence, any_error);

        if let Some(m) = &self.metrics {
            m.adverse_media_calls_total.with_label_values(&[result_label]).inc();
            m.adverse_media_latency_seconds
                .with_label_values(&[result_label])
                .observe(start.elapsed().as_secs_f64());
            // Token usage attribution.
            for (purpose, model, tokens) in self.gateway.budget().snapshot() {
                if tokens > 0 {
                    m.inference_tokens_used_total
                        .with_label_values(&[&purpose, &model])
                        .inc_by(0); // ensure label exists; per-call .inc_by occurs inside gateway in v2
                    let _ = (purpose, model, tokens);
                }
            }
        }

        StageOutcome {
            stage_id: StageId::AdverseMedia,
            kind,
            authenticity_bpa,
            risk_bpa,
            evidence: json!({
                "owners_screened": per_owner.len(),
                "per_owner": per_owner,
                "worst_verdict": worst.as_str(),
                "worst_confidence": worst_confidence,
                "any_error": any_error,
            }),
            duration_ms,
        }
    }
}

fn e_msg(e: &GatewayError) -> String {
    e.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Adverse,
    Clear,
    Unknown,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Adverse => "adverse",
            Self::Clear => "clear",
            Self::Unknown => "insufficient_evidence",
        }
    }
}

struct ParsedResponse {
    verdict: Verdict,
    confidence: f64,
    rationale: Option<String>,
    evidence_citations: Vec<String>,
}

fn parse_response(resp: &StructuredResponse) -> ParsedResponse {
    let v = resp.tool_input.get("verdict").and_then(|v| v.as_str()).unwrap_or("insufficient_evidence");
    let conf = resp
        .tool_input
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let rationale = resp
        .tool_input
        .get("rationale")
        .and_then(|v| v.as_str())
        .map(String::from);
    let citations: Vec<String> = resp
        .tool_input
        .get("evidence_citations")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let verdict = match v {
        "adverse" => Verdict::Adverse,
        "clear" => Verdict::Clear,
        _ => Verdict::Unknown,
    };
    ParsedResponse { verdict, confidence: conf, rationale, evidence_citations: citations }
}

fn bpa_for(
    v: Verdict,
    confidence: f64,
    error: bool,
) -> (StageOutcomeKind, BasicProbabilityAssignment, BasicProbabilityAssignment, &'static str) {
    if error {
        return (
            StageOutcomeKind::InsufficientEvidence,
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
            "error",
        );
    }
    match v {
        Verdict::Adverse if confidence >= 0.7 => (
            StageOutcomeKind::Fail,
            BasicProbabilityAssignment::new(0.05, 0.80, 0.15).expect("constant valid"),
            BasicProbabilityAssignment::new(0.80, 0.05, 0.15).expect("constant valid"),
            "match",
        ),
        Verdict::Adverse => (
            StageOutcomeKind::Fail,
            BasicProbabilityAssignment::new(0.15, 0.50, 0.35).expect("constant valid"),
            BasicProbabilityAssignment::new(0.50, 0.15, 0.35).expect("constant valid"),
            "match",
        ),
        Verdict::Clear => (
            StageOutcomeKind::Pass,
            BasicProbabilityAssignment::new(0.40, 0.05, 0.55).expect("constant valid"),
            BasicProbabilityAssignment::new(0.05, 0.40, 0.55).expect("constant valid"),
            "none",
        ),
        Verdict::Unknown => (
            StageOutcomeKind::InsufficientEvidence,
            BasicProbabilityAssignment::vacuous(),
            BasicProbabilityAssignment::vacuous(),
            "fixture",
        ),
    }
}

#[derive(Debug, Serialize)]
struct PerOwnerAdverse {
    person_id: uuid::Uuid,
    full_name: Option<String>,
    candidates: Vec<IcijCandidate>,
    verdict: String,
    confidence: f64,
    rationale: Option<String>,
    evidence_citations: Vec<String>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use secrecy::SecretString;
    use uuid::Uuid;

    use recor_inference_gateway::GatewayConfig;

    use crate::application::port::AdapterError;
    use crate::domain::declaration_snapshot::OwnerSnapshot;

    use super::*;

    struct FakeIcij {
        hits: HashMap<String, Vec<IcijCandidate>>,
    }
    #[async_trait]
    impl IcijAdapter for FakeIcij {
        async fn retrieve(
            &self,
            query: &PersonQuery,
            _max: usize,
        ) -> Result<Vec<IcijCandidate>, AdapterError> {
            Ok(self.hits.get(&query.full_name).cloned().unwrap_or_default())
        }
    }

    struct FakeResolver {
        names: HashMap<Uuid, ResolvedName>,
    }
    #[async_trait]
    impl NameResolver for FakeResolver {
        async fn resolve(&self, person_id: Uuid) -> Option<ResolvedName> {
            self.names.get(&person_id).cloned()
        }
    }

    fn icij(name: &str, sim: f64, snippet: &str) -> IcijCandidate {
        IcijCandidate {
            id: Uuid::now_v7(),
            node_kind: "person".into(),
            source_dataset: "panama".into(),
            canonical_full_name: name.into(),
            country_raw: Some("CM".into()),
            snippet: Some(snippet.into()),
            similarity: sim,
            tier: "certain".into(),
        }
    }

    fn snap(owners: Vec<Uuid>) -> DeclarationSnapshot {
        DeclarationSnapshot {
            declaration_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            declarant_principal: "spiffe://recor.cm/test".into(),
            declarant_role: "self".into(),
            kind: "incorporation".into(),
            effective_from: time::macros::date!(2026 - 01 - 01),
            beneficial_owners: owners
                .into_iter()
                .map(|id| OwnerSnapshot {
                    person_id: id,
                    ownership_basis_points: 10_000,
                    interest_kind: "equity".into(),
                })
                .collect(),
            attestation_signed_by: "spiffe://recor.cm/test".into(),
            attestation_signature_hex: hex::encode([0u8; 64]),
            attestation_public_key_hex: hex::encode([0u8; 32]),
            receipt_hash_hex: hex::encode([0u8; 32]),
            correlation_id: Uuid::now_v7(),
            submitted_at: time::OffsetDateTime::now_utc(),
        }
    }

    fn fixture_gateway() -> Arc<InferenceGateway> {
        // No API key → fixture mode → deterministic vacuous response.
        let cfg = GatewayConfig {
            api_key: SecretString::from(String::new()),
            base_url: "https://example.test".to_string(),
            default_tier: Tier::A,
            request_timeout: Duration::from_secs(1),
            session_token_ceiling: None,
        };
        Arc::new(InferenceGateway::new(cfg).unwrap())
    }

    #[tokio::test]
    async fn fixture_mode_yields_vacuous_stage() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Anyone".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let stage = AdverseMediaStage::new(
            Arc::new(FakeIcij { hits: HashMap::new() }),
            Arc::new(FakeResolver { names }),
            fixture_gateway(),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::InsufficientEvidence);
        assert_eq!(outcome.authenticity_bpa.m_uncertain, 1.0);
    }

    #[test]
    fn high_confidence_adverse_maps_to_high_false_mass() {
        let (kind, auth, _risk, _label) = bpa_for(Verdict::Adverse, 0.9, false);
        assert_eq!(kind, StageOutcomeKind::Fail);
        assert!(auth.m_false > 0.7);
    }

    #[test]
    fn mid_confidence_adverse_maps_to_moderate_false_mass() {
        let (_kind, auth, _risk, _) = bpa_for(Verdict::Adverse, 0.5, false);
        assert!((auth.m_false - 0.50).abs() < 1e-6);
    }

    #[test]
    fn clear_verdict_supports_authenticity() {
        let (kind, auth, _, _) = bpa_for(Verdict::Clear, 0.0, false);
        assert_eq!(kind, StageOutcomeKind::Pass);
        assert!(auth.m_true > 0.3);
    }

    #[test]
    fn parse_response_handles_missing_fields() {
        let r = StructuredResponse {
            tool_input: json!({"verdict": "adverse"}),
            stop_reason: "tool_use".into(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            model: "claude-opus-4-7".into(),
        };
        let p = parse_response(&r);
        assert_eq!(p.verdict, Verdict::Adverse);
        assert_eq!(p.confidence, 0.0);
    }

    #[test]
    fn user_prompt_includes_subject_and_snippets() {
        let pid = Uuid::now_v7();
        let prompt = build_user_prompt(
            "Anyone",
            pid,
            &[icij("Anyone", 0.9, "named in 2016 Panama Papers")],
        );
        assert!(prompt.contains("Anyone"));
        assert!(prompt.contains("Panama Papers"));
    }
}
