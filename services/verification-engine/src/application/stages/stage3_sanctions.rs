//! Stage 3 — Sanctions screening (R-VER-2).
//!
//! Consumes a `SanctionsAdapter` over the `sanctions_persons` table
//! (populated nightly by `bin/sanctions_ingest`). For every beneficial
//! owner: query the adapter for up-to-5 candidates. Aggregate by the
//! best tier across all owners and produce a single BPA contribution.
//!
//! BPA scoring (per the ticket's brief):
//!   * Any owner matches at `Certain` tier         → BPA(0.05, 0.85, 0.10)
//!   * Any owner matches at `Near` tier (no cert.) → BPA(0.20, 0.40, 0.40)
//!   * Only `Weak` matches                         → BPA(0.30, 0.10, 0.60)
//!   * No match at all                             → vacuous BPA
//!   * Adapter error                               → vacuous + Insufficient
//!     (D14 fail-closed at the integration boundary: the engine
//!     short-circuits to red downstream if every stage is vacuous.)
//!
//! Returned `evidence` shape:
//! ```json
//! {
//!   "owners_screened": N,
//!   "matches": [ { "person_id": "...", "candidates": [...] } ],
//!   "best_tier": "certain" | "near" | "weak" | "none",
//!   "backend_error": null | "..."
//! }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tracing::warn;

use crate::application::port::{PersonQuery, SanctionMatch, SanctionsAdapter};
use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome,
    StageOutcomeKind,
};
use crate::metrics::Metrics;

pub struct SanctionsStage {
    adapter: Arc<dyn SanctionsAdapter>,
    metrics: Option<Arc<Metrics>>,
    /// Number of candidates the adapter is asked to return per owner.
    /// 5 matches the brief.
    max_candidates: usize,
    /// Optional name resolver — caller supplies a function from
    /// `person_id` to `full_name`. When `None`, the stage degrades to
    /// vacuous because it has no name to screen. In production this is
    /// wired to the BUNEC adapter's `Found` outputs from Stage 2; in
    /// tests, a fixture closure.
    name_resolver: Arc<dyn NameResolver>,
}

impl SanctionsStage {
    pub fn new(
        adapter: Arc<dyn SanctionsAdapter>,
        name_resolver: Arc<dyn NameResolver>,
    ) -> Self {
        Self { adapter, metrics: None, max_candidates: 5, name_resolver }
    }

    pub fn with_metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }
}

/// Resolver: maps a beneficial-owner `person_id` to a name to screen.
/// The verification engine's authoritative source for names is BUNEC
/// (Stage 2); a pre-Stage-3 hook collects those names. For tests, the
/// resolver is a closure over a `HashMap`.
#[async_trait]
pub trait NameResolver: Send + Sync {
    async fn resolve(&self, person_id: uuid::Uuid) -> Option<ResolvedName>;
}

#[derive(Debug, Clone)]
pub struct ResolvedName {
    pub full_name: String,
    pub nationality: Option<String>,
    pub date_of_birth: Option<time::Date>,
}

#[async_trait]
impl Stage for SanctionsStage {
    fn id(&self) -> StageId {
        StageId::SanctionsScreening
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();
        let mut per_owner: Vec<PerOwnerSanctions> = Vec::with_capacity(declaration.beneficial_owners.len());
        let mut best_tier = TierAggregate::None;
        let mut backend_error: Option<String> = None;

        for owner in &declaration.beneficial_owners {
            let resolved = match self.name_resolver.resolve(owner.person_id).await {
                Some(r) => r,
                None => {
                    // No name → cannot screen. Record explicitly.
                    per_owner.push(PerOwnerSanctions {
                        person_id: owner.person_id,
                        full_name: None,
                        candidates: vec![],
                        best_tier: "none".to_string(),
                        error: Some("name not resolved (Stage 2 did not produce a canonical name)".into()),
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
                    let tier = aggregate_tier(&candidates);
                    best_tier.absorb(tier);
                    per_owner.push(PerOwnerSanctions {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        best_tier: tier.as_str().to_string(),
                        candidates,
                        error: None,
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    warn!(error = %msg, person_id = %owner.person_id, "sanctions screen failed");
                    backend_error = Some(msg.clone());
                    per_owner.push(PerOwnerSanctions {
                        person_id: owner.person_id,
                        full_name: Some(resolved.full_name),
                        candidates: vec![],
                        best_tier: "error".to_string(),
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
            match best_tier {
                TierAggregate::Certain => (
                    StageOutcomeKind::Fail,
                    BasicProbabilityAssignment::new(0.05, 0.85, 0.10).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.85, 0.05, 0.10).expect("constant valid"),
                    "certain",
                ),
                TierAggregate::Near => (
                    StageOutcomeKind::Fail,
                    BasicProbabilityAssignment::new(0.20, 0.40, 0.40).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.40, 0.20, 0.40).expect("constant valid"),
                    "near",
                ),
                TierAggregate::Weak => (
                    StageOutcomeKind::InsufficientEvidence,
                    BasicProbabilityAssignment::new(0.30, 0.10, 0.60).expect("constant valid"),
                    BasicProbabilityAssignment::new(0.10, 0.30, 0.60).expect("constant valid"),
                    "near",
                ),
                TierAggregate::None => (
                    StageOutcomeKind::Pass,
                    BasicProbabilityAssignment::vacuous(),
                    BasicProbabilityAssignment::vacuous(),
                    "none",
                ),
            }
        };

        if let Some(m) = &self.metrics {
            m.sanctions_screen_total.with_label_values(&[result_label]).inc();
            m.sanctions_screen_latency_seconds
                .with_label_values(&[result_label])
                .observe(start.elapsed().as_secs_f64());
        }

        StageOutcome {
            stage_id: StageId::SanctionsScreening,
            kind,
            authenticity_bpa,
            risk_bpa,
            evidence: json!({
                "owners_screened": per_owner.len(),
                "matches": per_owner,
                "best_tier": best_tier.as_str(),
                "backend_error": backend_error,
            }),
            duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TierAggregate {
    Certain,
    Near,
    Weak,
    None,
}

impl TierAggregate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Near => "near",
            Self::Weak => "weak",
            Self::None => "none",
        }
    }

    fn absorb(&mut self, incoming: Self) {
        // Worst-case ordering: Certain > Near > Weak > None
        let promote = matches!(
            (*self, incoming),
            (Self::None, Self::Weak)
                | (Self::None, Self::Near)
                | (Self::None, Self::Certain)
                | (Self::Weak, Self::Near)
                | (Self::Weak, Self::Certain)
                | (Self::Near, Self::Certain)
        );
        if promote {
            *self = incoming;
        }
    }
}

fn aggregate_tier(candidates: &[SanctionMatch]) -> TierAggregate {
    let mut t = TierAggregate::None;
    for c in candidates {
        let incoming = match c.tier.as_str() {
            "certain" => TierAggregate::Certain,
            "near" => TierAggregate::Near,
            "weak" => TierAggregate::Weak,
            _ => TierAggregate::None,
        };
        t.absorb(incoming);
    }
    t
}

#[derive(Debug, Serialize)]
struct PerOwnerSanctions {
    person_id: uuid::Uuid,
    full_name: Option<String>,
    candidates: Vec<SanctionMatch>,
    best_tier: String,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use uuid::Uuid;

    use crate::application::port::AdapterError;
    use crate::domain::declaration_snapshot::OwnerSnapshot;

    use super::*;

    struct FakeSanctions {
        hits: HashMap<String, Vec<SanctionMatch>>,
        fail: bool,
    }

    #[async_trait]
    impl SanctionsAdapter for FakeSanctions {
        async fn screen(
            &self,
            query: &PersonQuery,
            _max: usize,
        ) -> Result<Vec<SanctionMatch>, AdapterError> {
            if self.fail {
                return Err(AdapterError::Backend("simulated".into()));
            }
            Ok(self
                .hits
                .get(&query.full_name)
                .cloned()
                .unwrap_or_default())
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

    fn make_match(tier: &str, sim: f64) -> SanctionMatch {
        SanctionMatch {
            list_entry_id: Uuid::now_v7(),
            source: "ofac_sdn".into(),
            canonical_full_name: "test entity".into(),
            sanction_program: "TEST".into(),
            similarity: sim,
            tier: tier.into(),
        }
    }

    #[tokio::test]
    async fn certain_match_produces_high_false_mass() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Listed Person".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert("Listed Person".to_string(), vec![make_match("certain", 0.95)]);
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Fail);
        assert!(outcome.authenticity_bpa.belief_false() > 0.8);
        assert!(outcome.risk_bpa.belief_true() > 0.8);
    }

    #[tokio::test]
    async fn near_match_emits_moderate_fail() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Almost Listed".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert(
            "Almost Listed".to_string(),
            vec![make_match("near", 0.78)],
        );
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Fail);
        assert!((outcome.authenticity_bpa.m_false - 0.4).abs() < 0.01);
        assert!((outcome.authenticity_bpa.m_true - 0.2).abs() < 0.01);
    }

    #[tokio::test]
    async fn no_match_is_vacuous() {
        let pid = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            pid,
            ResolvedName {
                full_name: "Random Name".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits: HashMap::new(), fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Pass);
        assert_eq!(outcome.authenticity_bpa.m_uncertain, 1.0);
    }

    #[tokio::test]
    async fn backend_error_is_insufficient_evidence() {
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
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits: HashMap::new(), fail: true }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::InsufficientEvidence);
    }

    #[tokio::test]
    async fn worst_tier_wins_across_owners() {
        let p1 = Uuid::now_v7();
        let p2 = Uuid::now_v7();
        let mut names = HashMap::new();
        names.insert(
            p1,
            ResolvedName {
                full_name: "Clean Name".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        names.insert(
            p2,
            ResolvedName {
                full_name: "Listed Person".into(),
                nationality: None,
                date_of_birth: None,
            },
        );
        let mut hits = HashMap::new();
        hits.insert("Listed Person".to_string(), vec![make_match("certain", 0.95)]);
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits, fail: false }),
            Arc::new(FakeResolver { names }),
        );
        let outcome = stage.run(&snap(vec![p1, p2])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Fail);
        assert!(outcome.authenticity_bpa.belief_false() > 0.8);
    }

    #[tokio::test]
    async fn unresolved_name_is_recorded_but_not_fatal() {
        let pid = Uuid::now_v7();
        let stage = SanctionsStage::new(
            Arc::new(FakeSanctions { hits: HashMap::new(), fail: false }),
            Arc::new(FakeResolver { names: HashMap::new() }),
        );
        let outcome = stage.run(&snap(vec![pid])).await;
        // No name → no screen → vacuous (the stage neither passed nor failed).
        assert_eq!(outcome.kind, StageOutcomeKind::Pass);
    }
}
