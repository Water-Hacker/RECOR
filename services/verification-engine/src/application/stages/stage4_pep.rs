//! Stage 4 — PEP screening (R-VER-3).
//!
//! Sibling of `stage3_sanctions`. Consumes the same `NameResolver` and
//! a `PepAdapter` over the `peps` table. Distinguishes confirmed PEPs
//! from associates per the OpenSanctions `relationships` field.
//!
//! BPA scoring (per the ticket):
//!   * Confirmed PEP, tier Certain/Near → BPA(0.20, 0.50, 0.30)
//!   * Associate of PEP                 → BPA(0.30, 0.30, 0.40)
//!   * Weak match only                  → BPA(0.30, 0.10, 0.60)
//!   * No match                         → vacuous
//!   * Backend error                    → InsufficientEvidence (vacuous)
//!
//! Note: PEP exposure is not a false-authenticity signal — a senior
//! official can legitimately own assets — but it IS a risk signal.
//! Hence the `risk_bpa` carries the bulk of the False/True mass,
//! while `authenticity_bpa` skews uncertain.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tracing::warn;

use crate::application::port::{PepAdapter, PepMatch, PersonQuery};
use crate::application::stages::stage3_sanctions::{NameResolver, ResolvedName};
use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome,
    StageOutcomeKind,
};
use crate::metrics::Metrics;

pub struct PepStage {
    adapter: Arc<dyn PepAdapter>,
    name_resolver: Arc<dyn NameResolver>,
    metrics: Option<Arc<Metrics>>,
    max_candidates: usize,
}

impl PepStage {
    pub fn new(
        adapter: Arc<dyn PepAdapter>,
        name_resolver: Arc<dyn NameResolver>,
    ) -> Self {
        Self { adapter, name_resolver, metrics: None, max_candidates: 5 }
    }

    pub fn with_metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }
}

#[async_trait]
impl Stage for PepStage {
    fn id(&self) -> StageId {
        StageId::PoliticallyExposedPersons
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();
        let mut per_owner: Vec<PerOwnerPep> = Vec::with_capacity(declaration.beneficial_owners.len());
        let mut best = PepClassification::None;
        let mut backend_error: Option<String> = None;

        for owner in &declaration.beneficial_owners {
            let resolved = match self.name_resolver.resolve(owner.person_id).await {
                Some(r) => r,
                None => {
                    per_owner.push(PerOwnerPep {
                        person_id: owner.person_id,
                        full_name: None,
                        candidates: vec![],
                        classification: "none".into(),
                        error: Some("name not resolved".into()),
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
            match self.adapter.screen(&query, self.max_candidates).await {
                Ok(candidates) => {
                    let cls = classify(&candidates);
                    best.absorb(cls);
                    per_owner.push(PerOwnerPep {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        classification: cls.as_str().to_string(),
                        candidates,
                        error: None,
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    warn!(error = %msg, person_id = %owner.person_id, "pep screen failed");
                    backend_error = Some(msg.clone());
                    per_owner.push(PerOwnerPep {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        candidates: vec![],
                        classification: "error".into(),
                        error: Some(msg),
                    });
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let (kind, authenticity_bpa, risk_bpa, result_label) = if backend_error.is_some() {
            (
                StageOutcomeKind::InsufficientEvidence,
                BasicProbabilityAssignment::vacuous(),
                BasicProbabilityAssignment::vacuous(),
                "error",
            )
        } else {
            match best {
                PepClassification::ConfirmedPep => (
                    StageOutcomeKind::Fail,
                    BasicProbabilityAssignment::new(0.20, 0.50, 0.30).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.50, 0.20, 0.30).expect("constant valid"),
                    "confirmed",
                ),
                PepClassification::Associate => (
                    StageOutcomeKind::Fail,
                    BasicProbabilityAssignment::new(0.30, 0.30, 0.40).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.30, 0.30, 0.40).expect("constant valid"),
                    "associate",
                ),
                PepClassification::Weak => (
                    StageOutcomeKind::InsufficientEvidence,
                    BasicProbabilityAssignment::new(0.30, 0.10, 0.60).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.10, 0.30, 0.60).expect("constant valid"),
                    "associate",
                ),
                PepClassification::None => (
                    StageOutcomeKind::Pass,
                    BasicProbabilityAssignment::vacuous(),
                    BasicProbabilityAssignment::vacuous(),
                    "none",
                ),
            }
        };

        if let Some(m) = &self.metrics {
            m.pep_screen_total.with_label_values(&[result_label]).inc();
            m.pep_screen_latency_seconds
                .with_label_values(&[result_label])
                .observe(start.elapsed().as_secs_f64());
        }

        StageOutcome {
            stage_id: StageId::PoliticallyExposedPersons,
            kind,
            authenticity_bpa,
            risk_bpa,
            evidence: json!({
                "owners_screened": per_owner.len(),
                "matches": per_owner,
                "classification": best.as_str(),
                "backend_error": backend_error,
            }),
            duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PepClassification {
    ConfirmedPep,
    Associate,
    Weak,
    None,
}

impl PepClassification {
    fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmedPep => "confirmed_pep",
            Self::Associate => "associate_of_pep",
            Self::Weak => "weak",
            Self::None => "none",
        }
    }
    fn absorb(&mut self, incoming: Self) {
        // Worst-case ordering: ConfirmedPep > Associate > Weak > None
        let promote = matches!(
            (*self, incoming),
            (Self::None, Self::Weak)
                | (Self::None, Self::Associate)
                | (Self::None, Self::ConfirmedPep)
                | (Self::Weak, Self::Associate)
                | (Self::Weak, Self::ConfirmedPep)
                | (Self::Associate, Self::ConfirmedPep)
        );
        if promote {
            *self = incoming;
        }
    }
}

fn classify(candidates: &[PepMatch]) -> PepClassification {
    let mut t = PepClassification::None;
    for c in candidates {
        let weak_or_better = matches!(c.tier.as_str(), "certain" | "near" | "weak");
        if !weak_or_better {
            continue;
        }
        let incoming = if matches!(c.tier.as_str(), "certain" | "near")
            && c.relationship_kind == "confirmed"
        {
            PepClassification::ConfirmedPep
        } else if matches!(c.tier.as_str(), "certain" | "near")
            && c.relationship_kind == "associate"
        {
            PepClassification::Associate
        } else {
            PepClassification::Weak
        };
        t.absorb(incoming);
    }
    t
}

#[derive(Debug, Serialize)]
struct PerOwnerPep {
    person_id: uuid::Uuid,
    full_name: Option<String>,
    candidates: Vec<PepMatch>,
    classification: String,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use uuid::Uuid;

    use crate::application::port::AdapterError;
    use crate::domain::declaration_snapshot::OwnerSnapshot;

    use super::*;

    struct FakePep {
        hits: HashMap<String, Vec<PepMatch>>,
        fail: bool,
    }

    #[async_trait]
    impl PepAdapter for FakePep {
        async fn screen(
            &self,
            query: &PersonQuery,
            _max: usize,
        ) -> Result<Vec<PepMatch>, AdapterError> {
            if self.fail {
                return Err(AdapterError::Backend("simulated".into()));
            }
            Ok(self.hits.get(&query.full_name).cloned().unwrap_or_default())
        }
        async fn index_rows(&self) -> Result<i64, AdapterError> {
            Ok(0)
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

    fn make_match(rel: &str, tier: &str, sim: f64) -> PepMatch {
        PepMatch {
            list_entry_id: Uuid::now_v7(),
            source: "opensanctions_pep".into(),
            canonical_full_name: "any".into(),
            position: Some("Minister".into()),
            country: Some("CM".into()),
            is_current: true,
            relationship_kind: rel.into(),
            similarity: sim,
            tier: tier.into(),
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
                                    cascade_tier: None,
                    control_basis: None,
                    cascade_tier_b_ruled_out_evidence: None,
                    is_nominee: None,
                    nominator_person_id: None,
})
                .collect(),
            attestation_signed_by: "spiffe://recor.cm/test".into(),
            attestation_signature_hex: hex::encode([0u8; 64]),
            attestation_public_key_hex: hex::encode([0u8; 32]),
            receipt_hash_hex: hex::encode([0u8; 32]),
            correlation_id: Uuid::now_v7(),
            submitted_at: time::OffsetDateTime::now_utc(),
            adequacy_claims: None,
        }
    }

    #[tokio::test]
    async fn confirmed_pep_emits_bpa_02_05_03() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Minister X".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert(
            "Minister X".to_string(),
            vec![make_match("confirmed", "certain", 0.95)],
        );
        let stage = PepStage::new(
            Arc::new(FakePep { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        let bpa = outcome.authenticity_bpa;
        assert!((bpa.m_true - 0.20).abs() < 1e-6);
        assert!((bpa.m_false - 0.50).abs() < 1e-6);
        assert!((bpa.m_uncertain - 0.30).abs() < 1e-6);
    }

    #[tokio::test]
    async fn associate_of_pep_emits_bpa_03_03_04() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Friend Y".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert(
            "Friend Y".to_string(),
            vec![make_match("associate", "near", 0.78)],
        );
        let stage = PepStage::new(
            Arc::new(FakePep { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        let bpa = outcome.authenticity_bpa;
        assert!((bpa.m_true - 0.30).abs() < 1e-6);
        assert!((bpa.m_false - 0.30).abs() < 1e-6);
        assert!((bpa.m_uncertain - 0.40).abs() < 1e-6);
    }

    #[tokio::test]
    async fn no_match_is_vacuous() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Anyone Else".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let stage = PepStage::new(
            Arc::new(FakePep { hits: HashMap::new(), fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Pass);
        assert_eq!(outcome.authenticity_bpa.m_uncertain, 1.0);
    }

    #[tokio::test]
    async fn confirmed_overrides_associate() {
        let p1 = Uuid::now_v7();
        let p2 = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            p1,
            ResolvedName {
                full_name: "Friend".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        names.insert(
            p2,
            ResolvedName {
                full_name: "Minister".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert(
            "Friend".to_string(),
            vec![make_match("associate", "near", 0.78)],
        );
        hits.insert(
            "Minister".to_string(),
            vec![make_match("confirmed", "certain", 0.95)],
        );
        let stage = PepStage::new(
            Arc::new(FakePep { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![p1, p2])).await;
        let bpa = outcome.authenticity_bpa;
        assert!((bpa.m_false - 0.50).abs() < 1e-6); // confirmed-level
    }
}
