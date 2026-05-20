//! Stage 7 — Cross-source triangulation (real implementation).
//!
//! TODO-013. FATF R.24 c.24.6 multi-pronged approach + IO.5.4
//! ("evidence MUST be cross-referenced across sources, not consumed
//! in isolation"). The stage compares the upstream outcomes from
//! Stages 3-6 — sanctions, PEP, adverse media, patterns — and the
//! declaration's own structural claims, and synthesises an
//! authenticity / risk BPA that reflects the *intersection* of
//! signals rather than any one source.
//!
//! v1 decision rules (ADR-0014):
//!
//!   - **Multi-source convergence** — when ≥2 of {Stage 3 sanctions,
//!     Stage 4 PEP, Stage 5 adverse-media, Stage 6 patterns} return
//!     `Fail`, Stage 7 returns `Fail` with risk BPA biased toward
//!     "risk: 0.8 / 0.95" and authenticity biased toward
//!     "authenticity_false: 0.6 / 0.9". The intuition: two
//!     independent sources agreeing on a red flag is strong evidence.
//!
//!   - **Single-source contradiction with structural support** —
//!     when exactly one of the upstream sources fails AND the
//!     declaration's `adequacy_claims.adequate == false` OR a BO
//!     carries `is_nominee = true` without `nominator_person_id`,
//!     Stage 7 returns `Fail` with a moderate authenticity dent.
//!
//!   - **Cascade-tier inconsistency** — when a BO carries
//!     `cascade_tier = "B"` (control-by-other-means) but
//!     `cascade_tier_b_ruled_out_evidence` is absent, Stage 7
//!     records `InsufficientEvidence` with a structural note —
//!     not a `Fail`, because the missing-evidence question is a
//!     declarant-side issue resolvable through Correct, not a
//!     verification-engine red flag in itself.
//!
//!   - **Default** — `InsufficientEvidence` with vacuous BPA when
//!     nothing of the above triggers. The orchestrator's fusion
//!     accumulator handles vacuous gracefully.
//!
//! Future expansion (TODO-013-graph follow-up):
//!   - Prior-declaration drift: query the declaration projection for
//!     prior declarations under the same `declarant_principal` and
//!     compare the BO sets. Sudden BO disappearance after a
//!     sanctions hit is a strong signal.
//!   - Cross-entity ownership graph traversal: if `person_id` X
//!     appears as a BO in `entity_id` A AND in `entity_id` B
//!     simultaneously, with both A and B drawing sanctions hits,
//!     escalate.
//!   - BUNEC corporate-register cross-reference (TODO-015): once
//!     the real BUNEC adapter is live, the declarant's entity_id is
//!     resolvable to a canonical corporate record. Stage 7 compares
//!     declared cascade tiers against the registered corporate
//!     structure.

use async_trait::async_trait;
use serde_json::json;

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome,
    StageOutcomeKind,
};

pub struct CrossSourceTriangulationStage;

impl CrossSourceTriangulationStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CrossSourceTriangulationStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for CrossSourceTriangulationStage {
    fn id(&self) -> StageId {
        StageId::CrossSourceTriangulation
    }

    /// `run` without upstream context is a degenerate path — the
    /// stage has no signal of its own. Return `InsufficientEvidence`.
    async fn run(&self, _d: &DeclarationSnapshot) -> StageOutcome {
        StageOutcome {
            stage_id: StageId::CrossSourceTriangulation,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({
                "rationale": "Stage 7 received no upstream context; orchestrator must call run_with_context",
            }),
            duration_ms: 0,
        }
    }

    async fn run_with_context(
        &self,
        d: &DeclarationSnapshot,
        upstream: &[StageOutcome],
    ) -> StageOutcome {
        let start = std::time::Instant::now();

        // ─── Count upstream Fails by source ─────────────────────────
        let mut fail_sources: Vec<&'static str> = Vec::new();
        for outcome in upstream {
            if !matches!(outcome.kind, StageOutcomeKind::Fail) {
                continue;
            }
            match outcome.stage_id {
                StageId::SanctionsScreening => fail_sources.push("sanctions"),
                StageId::PoliticallyExposedPersons => fail_sources.push("pep"),
                StageId::AdverseMedia => fail_sources.push("adverse_media"),
                StageId::PatternDetection => fail_sources.push("patterns"),
                _ => {}
            }
        }
        let fail_count = fail_sources.len();

        // ─── Structural signals from the declaration itself ────────
        let adequate_false = d
            .adequacy_claims
            .as_ref()
            .map(|c| !c.adequate)
            .unwrap_or(false);
        let nominee_without_nominator = d
            .beneficial_owners
            .iter()
            .any(|o| o.is_nominee == Some(true) && o.nominator_person_id.is_none());
        let cascade_b_without_evidence = d
            .beneficial_owners
            .iter()
            .any(|o| {
                o.cascade_tier.as_deref().map(|t| t.eq_ignore_ascii_case("B"))
                    == Some(true)
                    && o.cascade_tier_b_ruled_out_evidence.is_none()
            });

        // ─── Decision rules (ADR-0014) ──────────────────────────────
        // Rule 1: multi-source convergence.
        if fail_count >= 2 {
            let evidence = json!({
                "rule": "multi_source_convergence",
                "fail_sources": fail_sources,
                "fail_count": fail_count,
                "structural": {
                    "adequate_false": adequate_false,
                    "nominee_without_nominator": nominee_without_nominator,
                    "cascade_b_without_evidence": cascade_b_without_evidence,
                },
            });
            // authenticity: strong support for "false" (declaration is
            // NOT what it claims). Mass: 0.05 true / 0.60 false / 0.35
            // uncertain — the uncertain band reserves room for the
            // fusion stage to combine with other evidence.
            let authenticity_bpa = BasicProbabilityAssignment::new(0.05, 0.60, 0.35)
                .expect("BPA mass sums to 1.0");
            // risk: strong support for "risky" (= true on the risk
            // axis). Mass: 0.80 true / 0.05 false / 0.15 uncertain.
            let risk_bpa = BasicProbabilityAssignment::new(0.80, 0.05, 0.15)
                .expect("BPA mass sums to 1.0");
            return StageOutcome {
                stage_id: StageId::CrossSourceTriangulation,
                kind: StageOutcomeKind::Fail,
                authenticity_bpa,
                risk_bpa,
                evidence,
                duration_ms: duration_ms(start),
            };
        }

        // Rule 2: single-source contradiction with structural support.
        if fail_count == 1
            && (adequate_false || nominee_without_nominator)
        {
            let evidence = json!({
                "rule": "single_source_with_structural_support",
                "fail_sources": fail_sources,
                "structural": {
                    "adequate_false": adequate_false,
                    "nominee_without_nominator": nominee_without_nominator,
                },
            });
            // Moderate fail — single source is weaker evidence than
            // convergence. Mass: 0.20 / 0.40 / 0.40 (authenticity) and
            // 0.55 / 0.15 / 0.30 (risk).
            let authenticity_bpa = BasicProbabilityAssignment::new(0.20, 0.40, 0.40)
                .expect("BPA mass sums to 1.0");
            let risk_bpa = BasicProbabilityAssignment::new(0.55, 0.15, 0.30)
                .expect("BPA mass sums to 1.0");
            return StageOutcome {
                stage_id: StageId::CrossSourceTriangulation,
                kind: StageOutcomeKind::Fail,
                authenticity_bpa,
                risk_bpa,
                evidence,
                duration_ms: duration_ms(start),
            };
        }

        // Rule 3: cascade-tier-B missing evidence → InsufficientEvidence
        // (NOT Fail, per ADR-0014 — this is a declarant-side correctable).
        if cascade_b_without_evidence {
            let evidence = json!({
                "rule": "cascade_b_without_evidence",
                "fail_sources": fail_sources,
                "note": "BO with cascade_tier=B is missing cascade_tier_b_ruled_out_evidence; declarant should Correct",
            });
            return StageOutcome {
                stage_id: StageId::CrossSourceTriangulation,
                kind: StageOutcomeKind::InsufficientEvidence,
                authenticity_bpa: BasicProbabilityAssignment::vacuous(),
                risk_bpa: BasicProbabilityAssignment::vacuous(),
                evidence,
                duration_ms: duration_ms(start),
            };
        }

        // Default — nothing to triangulate against.
        StageOutcome {
            stage_id: StageId::CrossSourceTriangulation,
            kind: StageOutcomeKind::InsufficientEvidence,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: json!({
                "rule": "default_no_convergence",
                "fail_sources": fail_sources,
            }),
            duration_ms: duration_ms(start),
        }
    }
}

fn duration_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::declaration_snapshot::{AdequacyClaimsSnapshot, OwnerSnapshot};
    use time::macros::date;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn snapshot_with(owners: Vec<OwnerSnapshot>, adequate: Option<bool>) -> DeclarationSnapshot {
        DeclarationSnapshot {
            declaration_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            declarant_principal: "spiffe://recor.cm/test".into(),
            declarant_role: "self_declaration".into(),
            kind: "incorporation".into(),
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: owners,
            attestation_signed_by: "spiffe://recor.cm/test".into(),
            attestation_signature_hex: "00".repeat(64),
            attestation_public_key_hex: "11".repeat(32),
            receipt_hash_hex: "ab".repeat(32),
            correlation_id: Uuid::now_v7(),
            submitted_at: OffsetDateTime::now_utc(),
            adequacy_claims: adequate.map(|a| AdequacyClaimsSnapshot {
                adequate: a,
                accurate: true,
                up_to_date_as_of: OffsetDateTime::now_utc(),
                legal_basis: "FATF c.24.8".into(),
            }),
        }
    }

    fn owner_default() -> OwnerSnapshot {
        OwnerSnapshot {
            person_id: Uuid::now_v7(),
            ownership_basis_points: 10_000,
            interest_kind: "equity".into(),
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        }
    }

    fn fail_outcome(stage: StageId) -> StageOutcome {
        StageOutcome {
            stage_id: stage,
            kind: StageOutcomeKind::Fail,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: serde_json::json!({}),
            duration_ms: 0,
        }
    }

    fn pass_outcome(stage: StageId) -> StageOutcome {
        StageOutcome {
            stage_id: stage,
            kind: StageOutcomeKind::Pass,
            authenticity_bpa: BasicProbabilityAssignment::vacuous(),
            risk_bpa: BasicProbabilityAssignment::vacuous(),
            evidence: serde_json::json!({}),
            duration_ms: 0,
        }
    }

    #[tokio::test]
    async fn no_upstream_returns_insufficient_evidence() {
        let s = CrossSourceTriangulationStage::new();
        let outcome = s.run(&snapshot_with(vec![owner_default()], None)).await;
        assert!(matches!(
            outcome.kind,
            StageOutcomeKind::InsufficientEvidence
        ));
    }

    #[tokio::test]
    async fn multi_source_convergence_fails() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![
            fail_outcome(StageId::SanctionsScreening),
            fail_outcome(StageId::PoliticallyExposedPersons),
        ];
        let outcome = s
            .run_with_context(&snapshot_with(vec![owner_default()], None), &outcomes)
            .await;
        assert!(matches!(outcome.kind, StageOutcomeKind::Fail));
        let evidence = outcome.evidence.as_object().unwrap();
        assert_eq!(evidence["rule"], "multi_source_convergence");
        // Risk BPA should be biased high. m_true=0.80 directly.
        assert!(outcome.risk_bpa.belief_true() >= 0.7);
    }

    #[tokio::test]
    async fn single_source_with_inadequate_claim_fails() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![fail_outcome(StageId::AdverseMedia)];
        let outcome = s
            .run_with_context(
                &snapshot_with(vec![owner_default()], Some(false)),
                &outcomes,
            )
            .await;
        assert!(matches!(outcome.kind, StageOutcomeKind::Fail));
        let evidence = outcome.evidence.as_object().unwrap();
        assert_eq!(
            evidence["rule"],
            "single_source_with_structural_support"
        );
    }

    #[tokio::test]
    async fn single_source_with_nominee_no_nominator_fails() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![fail_outcome(StageId::SanctionsScreening)];
        let mut owner = owner_default();
        owner.is_nominee = Some(true);
        owner.nominator_person_id = None;
        let outcome = s
            .run_with_context(&snapshot_with(vec![owner], None), &outcomes)
            .await;
        assert!(matches!(outcome.kind, StageOutcomeKind::Fail));
    }

    #[tokio::test]
    async fn single_source_no_structural_signal_is_insufficient() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![fail_outcome(StageId::SanctionsScreening)];
        let outcome = s
            .run_with_context(&snapshot_with(vec![owner_default()], None), &outcomes)
            .await;
        assert!(matches!(
            outcome.kind,
            StageOutcomeKind::InsufficientEvidence
        ));
    }

    #[tokio::test]
    async fn cascade_b_without_evidence_is_insufficient() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![pass_outcome(StageId::SanctionsScreening)];
        let mut owner = owner_default();
        owner.cascade_tier = Some("B".into());
        owner.cascade_tier_b_ruled_out_evidence = None;
        let outcome = s
            .run_with_context(&snapshot_with(vec![owner], None), &outcomes)
            .await;
        assert!(matches!(
            outcome.kind,
            StageOutcomeKind::InsufficientEvidence
        ));
        let evidence = outcome.evidence.as_object().unwrap();
        assert_eq!(evidence["rule"], "cascade_b_without_evidence");
    }

    #[tokio::test]
    async fn all_passes_is_insufficient_evidence_default() {
        let s = CrossSourceTriangulationStage::new();
        let outcomes = vec![
            pass_outcome(StageId::SanctionsScreening),
            pass_outcome(StageId::PoliticallyExposedPersons),
            pass_outcome(StageId::AdverseMedia),
            pass_outcome(StageId::PatternDetection),
        ];
        let outcome = s
            .run_with_context(&snapshot_with(vec![owner_default()], Some(true)), &outcomes)
            .await;
        assert!(matches!(
            outcome.kind,
            StageOutcomeKind::InsufficientEvidence
        ));
        let evidence = outcome.evidence.as_object().unwrap();
        assert_eq!(evidence["rule"], "default_no_convergence");
    }
}
