//! Stage 2 — Identity authentication against BUNEC.
//!
//! For every declared beneficial owner, query BUNEC. Aggregate the
//! per-owner outcomes into a single stage outcome with structured
//! evidence enumerating which owners resolved and which did not.
//!
//! Authenticity BPA logic:
//!   * All owners found     → high belief in True (0.6 to True, 0.4 ignorance)
//!   * Some owners found    → moderate belief in False (0.4 to False, 0.6 ignorance)
//!   * No owners found      → strong belief in False (0.85 to False, 0.15 ignorance)
//!   * BUNEC backend down   → vacuous (insufficient evidence)
//!
//! These weights are reasonable defaults; the calibration ceremony
//! (operational concern) tunes them against the adversarial corpus.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;

use crate::application::port::{BunecAdapter, BunecLookup};
use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind,
};

pub struct IdentityAuthenticationStage {
    bunec: Arc<dyn BunecAdapter>,
}

impl IdentityAuthenticationStage {
    pub fn new(bunec: Arc<dyn BunecAdapter>) -> Self {
        Self { bunec }
    }
}

#[async_trait]
impl Stage for IdentityAuthenticationStage {
    fn id(&self) -> StageId {
        StageId::IdentityAuthentication
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();

        let mut results: Vec<PerOwnerEvidence> = Vec::with_capacity(declaration.beneficial_owners.len());
        let mut backend_failed = false;
        let mut circuit_open = false;

        for owner in &declaration.beneficial_owners {
            match self.bunec.lookup(owner.person_id).await {
                Ok(BunecLookup::Found {
                    person_id,
                    canonical_full_name,
                    nationality,
                }) => results.push(PerOwnerEvidence {
                    person_id,
                    found: true,
                    canonical_full_name: Some(canonical_full_name),
                    nationality: Some(nationality),
                    error: None,
                }),
                Ok(BunecLookup::NotFound { person_id }) => results.push(PerOwnerEvidence {
                    person_id,
                    found: false,
                    canonical_full_name: None,
                    nationality: None,
                    error: None,
                }),
                Ok(BunecLookup::CircuitOpen { since }) => {
                    circuit_open = true;
                    results.push(PerOwnerEvidence {
                        person_id: owner.person_id,
                        found: false,
                        canonical_full_name: None,
                        nationality: None,
                        error: Some(format!("bunec circuit open at {since}")),
                    });
                }
                Err(e) => {
                    backend_failed = true;
                    results.push(PerOwnerEvidence {
                        person_id: owner.person_id,
                        found: false,
                        canonical_full_name: None,
                        nationality: None,
                        error: Some(format!("{e}")),
                    });
                }
            }
        }

        let total = results.len();
        let found = results.iter().filter(|r| r.found).count();
        let duration_ms = start.elapsed().as_millis() as u64;

        let (kind, authenticity_bpa) = if backend_failed || circuit_open {
            (
                StageOutcomeKind::InsufficientEvidence,
                BasicProbabilityAssignment::vacuous(),
            )
        } else if total == 0 {
            (
                StageOutcomeKind::InsufficientEvidence,
                BasicProbabilityAssignment::vacuous(),
            )
        } else if found == total {
            // All owners resolved at BUNEC → supports authenticity.
            (
                StageOutcomeKind::Pass,
                BasicProbabilityAssignment::new(0.6, 0.0, 0.4).expect("constant valid"),
            )
        } else if found == 0 {
            // No owners resolved → strong fail signal.
            (
                StageOutcomeKind::Fail,
                BasicProbabilityAssignment::new(0.0, 0.85, 0.15).expect("constant valid"),
            )
        } else {
            // Mixed result → moderate fail signal.
            (
                StageOutcomeKind::Fail,
                BasicProbabilityAssignment::new(0.0, 0.4, 0.6).expect("constant valid"),
            )
        };

        StageOutcome {
            stage_id: StageId::IdentityAuthentication,
            kind,
            authenticity_bpa,
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({
                "owners_total": total,
                "owners_found": found,
                "backend_failed": backend_failed,
                "circuit_open": circuit_open,
                "per_owner": results,
            }),
            duration_ms,
        }
    }
}

#[derive(Debug, Serialize)]
struct PerOwnerEvidence {
    person_id: uuid::Uuid,
    found: bool,
    canonical_full_name: Option<String>,
    nationality: Option<String>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::application::port::{BunecAdapter, BunecLookup, BunecLookupError};
    use crate::domain::declaration_snapshot::OwnerSnapshot;

    use super::*;

    struct FakeBunec {
        records: HashMap<Uuid, BunecLookup>,
        backend_should_fail: Mutex<bool>,
    }

    #[async_trait]
    impl BunecAdapter for FakeBunec {
        async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError> {
            if *self.backend_should_fail.lock().unwrap() {
                return Err(BunecLookupError::Backend("simulated outage".into()));
            }
            Ok(self
                .records
                .get(&person_id)
                .cloned()
                .unwrap_or(BunecLookup::NotFound { person_id }))
        }
    }

    fn snap(owner_ids: Vec<Uuid>) -> DeclarationSnapshot {
        DeclarationSnapshot {
            declaration_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            declarant_principal: "spiffe://recor.cm/test".into(),
            declarant_role: "self".into(),
            kind: "incorporation".into(),
            effective_from: time::macros::date!(2026 - 01 - 01),
            beneficial_owners: owner_ids
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
        }
            adequacy_claims: None,
}

    #[tokio::test]
    async fn all_owners_found_passes() {
        let id1 = Uuid::now_v7();
        let bunec = FakeBunec {
            records: HashMap::from([(
                id1,
                BunecLookup::Found {
                    person_id: id1,
                    canonical_full_name: "Aïssa Ngo Bidoung".into(),
                    nationality: "CM".into(),
                },
            )]),
            backend_should_fail: Mutex::new(false),
        };
        let stage = IdentityAuthenticationStage::new(Arc::new(bunec));
        let outcome = stage.run(&snap(vec![id1])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Pass);
        assert!(outcome.authenticity_bpa.belief_true() > 0.5);
    }

    #[tokio::test]
    async fn no_owners_found_fails() {
        let bunec = FakeBunec {
            records: HashMap::new(),
            backend_should_fail: Mutex::new(false),
        };
        let stage = IdentityAuthenticationStage::new(Arc::new(bunec));
        let outcome = stage.run(&snap(vec![Uuid::now_v7(), Uuid::now_v7()])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Fail);
        assert!(outcome.authenticity_bpa.belief_false() > 0.8);
    }

    #[tokio::test]
    async fn mixed_outcome_moderate_fail() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let bunec = FakeBunec {
            records: HashMap::from([(
                id1,
                BunecLookup::Found {
                    person_id: id1,
                    canonical_full_name: "Found Person".into(),
                    nationality: "CM".into(),
                },
            )]),
            backend_should_fail: Mutex::new(false),
        };
        let stage = IdentityAuthenticationStage::new(Arc::new(bunec));
        let outcome = stage.run(&snap(vec![id1, id2])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::Fail);
        // Moderate fail: 0.4 on False, 0.6 ignorance.
        assert!((outcome.authenticity_bpa.belief_false() - 0.4).abs() < 0.01);
    }

    #[tokio::test]
    async fn backend_failure_is_insufficient_evidence() {
        let bunec = FakeBunec {
            records: HashMap::new(),
            backend_should_fail: Mutex::new(true),
        };
        let stage = IdentityAuthenticationStage::new(Arc::new(bunec));
        let outcome = stage.run(&snap(vec![Uuid::now_v7()])).await;
        assert_eq!(outcome.kind, StageOutcomeKind::InsufficientEvidence);
        assert_eq!(outcome.authenticity_bpa.m_uncertain, 1.0);
    }
}
