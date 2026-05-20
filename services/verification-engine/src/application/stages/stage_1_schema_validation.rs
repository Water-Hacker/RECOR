//! Stage 1 — Schema and format validation.
//!
//! Deterministic, no I/O. Validates the declaration shape against the
//! invariants the verification engine relies on. Failures here
//! short-circuit the pipeline (`ShortCircuitFailClosed`); later stages
//! do not run.

use async_trait::async_trait;
use serde_json::json;
use time::OffsetDateTime;

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome, StageOutcomeKind,
};

pub struct SchemaValidationStage;

impl SchemaValidationStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SchemaValidationStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for SchemaValidationStage {
    fn id(&self) -> StageId {
        StageId::SchemaValidation
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();
        let mut errors = Vec::new();

        // 1. Beneficial owner list non-empty.
        if declaration.beneficial_owners.is_empty() {
            errors.push("no beneficial owners declared".to_string());
        }

        // 2. Ownership sums to exactly 10_000 basis points (100%).
        let sum: u32 = declaration
            .beneficial_owners
            .iter()
            .map(|o| o.ownership_basis_points)
            .sum();
        if sum != 10_000 && !declaration.beneficial_owners.is_empty() {
            errors.push(format!("ownership basis points sum to {sum}, expected 10_000"));
        }

        // 3. No duplicate person_id within the declaration.
        let mut seen = std::collections::HashSet::new();
        for owner in &declaration.beneficial_owners {
            if !seen.insert(owner.person_id) {
                errors.push(format!(
                    "duplicate beneficial owner person_id within declaration: {}",
                    owner.person_id
                ));
            }
        }

        // 4. Effective-from is not in the future.
        let today = OffsetDateTime::now_utc().date();
        if declaration.effective_from > today {
            errors.push(format!(
                "effective_from {} is after today's date {today}",
                declaration.effective_from
            ));
        }

        // 5. Attestation signature is well-formed hex (the API layer
        // already verified the signature against the canonical bytes;
        // here we just check structural well-formedness as a defence
        // in depth).
        if hex::decode(&declaration.attestation_signature_hex)
            .ok()
            .filter(|v| v.len() == 64)
            .is_none()
        {
            errors.push("attestation signature is not a 64-byte hex string".to_string());
        }
        if hex::decode(&declaration.attestation_public_key_hex)
            .ok()
            .filter(|v| v.len() == 32)
            .is_none()
        {
            errors.push("attestation public_key is not a 32-byte hex string".to_string());
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        if errors.is_empty() {
            StageOutcome {
                stage_id: StageId::SchemaValidation,
                kind: StageOutcomeKind::Pass,
                // A passing schema validation gives moderate evidence
                // FOR authenticity. It does not prove authenticity (a
                // skilled adversary can produce well-formed false
                // claims) but it rules out the trivial-failure class.
                authenticity_bpa: BasicProbabilityAssignment::new(0.30, 0.0, 0.70)
                    .expect("constant valid"),
                risk_bpa: BasicProbabilityAssignment::vacuous(),
                evidence: json!({ "checks_performed": 5, "errors": [] }),
                duration_ms,
            }
        } else {
            StageOutcome {
                stage_id: StageId::SchemaValidation,
                kind: StageOutcomeKind::ShortCircuitFailClosed,
                authenticity_bpa: BasicProbabilityAssignment::certain_false(),
                risk_bpa: BasicProbabilityAssignment::certain_true(),
                evidence: json!({ "checks_performed": 5, "errors": errors }),
                duration_ms,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use uuid::Uuid;

    use crate::domain::declaration_snapshot::OwnerSnapshot;

    use super::*;

    fn make_snapshot(owners: Vec<OwnerSnapshot>) -> DeclarationSnapshot {
        let key = SigningKey::from_bytes(&[1u8; 32]);
        DeclarationSnapshot {
            declaration_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            declarant_principal: "spiffe://recor.cm/test".into(),
            declarant_role: "self".into(),
            kind: "incorporation".into(),
            effective_from: time::macros::date!(2026 - 01 - 01),
            beneficial_owners: owners,
            attestation_signed_by: "spiffe://recor.cm/test".into(),
            attestation_signature_hex: hex::encode([0u8; 64]),
            attestation_public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            receipt_hash_hex: hex::encode([0u8; 32]),
            correlation_id: Uuid::now_v7(),
            submitted_at: OffsetDateTime::now_utc(),
            adequacy_claims: None,
        }
    }

    fn owner(percent_basis_points: u32) -> OwnerSnapshot {
        OwnerSnapshot {
            person_id: Uuid::now_v7(),
            ownership_basis_points: percent_basis_points,
            interest_kind: "equity".into(),
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        }
    }

    #[tokio::test]
    async fn valid_declaration_passes() {
        let stage = SchemaValidationStage::new();
        let s = make_snapshot(vec![owner(10_000)]);
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.stage_id, StageId::SchemaValidation);
        assert_eq!(outcome.kind, StageOutcomeKind::Pass);
    }

    #[tokio::test]
    async fn no_owners_short_circuits() {
        let stage = SchemaValidationStage::new();
        let s = make_snapshot(vec![]);
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed);
    }

    #[tokio::test]
    async fn bad_sum_short_circuits() {
        let stage = SchemaValidationStage::new();
        let s = make_snapshot(vec![owner(5_000), owner(4_000)]);
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed);
    }

    #[tokio::test]
    async fn duplicate_owner_short_circuits() {
        let stage = SchemaValidationStage::new();
        let person = Uuid::now_v7();
        let dup = |bp: u32| OwnerSnapshot {
            person_id: person,
            ownership_basis_points: bp,
            interest_kind: "equity".into(),
                    cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
};
        let s = make_snapshot(vec![dup(5_000), dup(5_000)]);
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed);
    }

    #[tokio::test]
    async fn future_effective_from_short_circuits() {
        let stage = SchemaValidationStage::new();
        let mut s = make_snapshot(vec![owner(10_000)]);
        s.effective_from = time::macros::date!(2099 - 12 - 31);
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed);
    }

    #[tokio::test]
    async fn malformed_signature_short_circuits() {
        let stage = SchemaValidationStage::new();
        let mut s = make_snapshot(vec![owner(10_000)]);
        s.attestation_signature_hex = "deadbeef".into();
        let outcome = stage.run(&s).await;
        assert_eq!(outcome.kind, StageOutcomeKind::ShortCircuitFailClosed);
    }
}
