//! Declaration aggregate. Event-sourced.
//!
//! `DeclarationAggregate` is the unit of consistency. Commands are
//! validated against current aggregate state; valid commands produce
//! events; events are applied to update state.
//!
//! Invariants enforced here:
//!   - At least one beneficial owner per declaration
//!   - Sum of ownership basis points across owners equals 10_000 (100%)
//!     for the first version of the aggregate (a future Amend command
//!     may relax this to allow partial-ownership declarations)
//!   - No duplicate person_id within a single declaration
//!   - effective_from is in the past 5 years and not after submitted_at
//!   - declaration_id may receive a Submit command only once
//!   - The attestation's signed_by matches the command's declarant_principal

use std::collections::HashSet;

use blake3::Hasher;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use super::command::{RecordVerificationOutcome, SubmitDeclaration};
use super::error::DomainError;
use super::event::{DeclarationEvent, DeclarationSubmittedV1, DeclarationVerifiedV1};
use super::value_object::{BeneficialOwnerClaim, DeclarationId, DeclarationState};

/// In-memory representation of a Declaration aggregate, hydrated from
/// its event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationAggregate {
    pub id: DeclarationId,
    /// Monotonic event count, used for optimistic concurrency.
    pub version: u64,
    pub state: DeclarationState,
    /// The verification case that produced the current state, if any.
    /// Set when a `declaration.verified.v1` event is applied; replays
    /// of the same case_id against the same aggregate are no-ops.
    pub verification_case_id: Option<uuid::Uuid>,
}

impl DeclarationAggregate {
    /// Construct a fresh aggregate at version 0, no events applied yet.
    pub fn fresh(id: DeclarationId) -> Self {
        Self {
            id,
            version: 0,
            state: DeclarationState::Draft, // aggregate-not-yet-emitting; "Draft" is the absent placeholder
            verification_case_id: None,
        }
    }

    /// Hydrate by replaying events in order.
    pub fn from_events(id: DeclarationId, events: &[DeclarationEvent]) -> Self {
        let mut agg = Self::fresh(id);
        for event in events {
            agg.apply(event);
        }
        agg
    }

    /// Apply an event to advance state. Pure; no I/O.
    pub fn apply(&mut self, event: &DeclarationEvent) {
        match event {
            DeclarationEvent::Submitted(_) => {
                self.state = DeclarationState::Submitted;
            }
            DeclarationEvent::Verified(p) => {
                self.state = p.lane.to_declaration_state();
                self.verification_case_id = Some(p.verification_case_id);
            }
        }
        self.version = self.version.saturating_add(1);
    }

    /// Validate a Submit command and produce the resulting event.
    /// Does NOT mutate `self`; the caller decides whether to apply
    /// after persistence succeeds.
    pub fn handle_submit(
        &self,
        cmd: SubmitDeclaration,
    ) -> Result<DeclarationEvent, DomainError> {
        if self.version > 0 {
            return Err(DomainError::AlreadySubmitted(self.id.0));
        }
        validate_command(&cmd)?;

        let receipt_hash_hex = compute_receipt_hash(&cmd);

        let payload = DeclarationSubmittedV1 {
            declaration_id: cmd.declaration_id,
            entity_id: cmd.entity_id,
            declarant_principal: cmd.declarant_principal,
            declarant_role: cmd.declarant_role,
            kind: cmd.kind,
            effective_from: cmd.effective_from,
            beneficial_owners: cmd.beneficial_owners,
            attestation: cmd.attestation,
            submitted_at: cmd.submitted_at,
            correlation_id: cmd.correlation_id,
            receipt_hash_hex,
        };

        Ok(DeclarationEvent::Submitted(payload))
    }

    /// Validate a RecordVerificationOutcome command and produce the
    /// resulting event. Idempotent at the boundary: if the same case_id
    /// has already been applied, returns `None` so the caller skips the
    /// write. A different case_id against an already-verified aggregate
    /// is a domain error — the writeback channel must not re-verify a
    /// declaration without explicit re-verification semantics, which v1
    /// does not support.
    pub fn handle_record_verification(
        &self,
        cmd: RecordVerificationOutcome,
    ) -> Result<Option<DeclarationEvent>, DomainError> {
        if self.version == 0 {
            return Err(DomainError::VerificationOutcomeBeforeSubmit(self.id.0));
        }

        validate_belief("fused_authenticity_belief", cmd.fused_authenticity_belief)?;
        validate_belief(
            "fused_authenticity_plausibility",
            cmd.fused_authenticity_plausibility,
        )?;
        validate_belief("fused_risk_belief", cmd.fused_risk_belief)?;

        if let Some(existing) = self.verification_case_id {
            return if existing == cmd.verification_case_id {
                // Replay of the same case — caller's writeback delivered twice.
                // No new event; the aggregate is already at the post-verification
                // state. Use case treats `None` as "ack and dispatch outbox".
                Ok(None)
            } else {
                Err(DomainError::VerificationCaseMismatch {
                    declaration_id: self.id.0,
                    existing_case_id: existing,
                    new_case_id: cmd.verification_case_id,
                })
            };
        }

        let payload = DeclarationVerifiedV1 {
            declaration_id: cmd.declaration_id,
            verification_case_id: cmd.verification_case_id,
            lane: cmd.lane,
            fused_authenticity_belief: cmd.fused_authenticity_belief,
            fused_authenticity_plausibility: cmd.fused_authenticity_plausibility,
            fused_risk_belief: cmd.fused_risk_belief,
            completed_at: cmd.completed_at,
            recorded_at: OffsetDateTime::now_utc(),
        };
        Ok(Some(DeclarationEvent::Verified(payload)))
    }
}

fn validate_belief(field: &'static str, value: f64) -> Result<(), DomainError> {
    if !(0.0..=1.0).contains(&value) || !value.is_finite() {
        return Err(DomainError::FusedBeliefOutOfRange { field, value });
    }
    Ok(())
}

fn validate_command(cmd: &SubmitDeclaration) -> Result<(), DomainError> {
    if cmd.declarant_principal.trim().is_empty() {
        return Err(DomainError::EmptyDeclarantPrincipal);
    }
    if cmd.attestation.signed_by != cmd.declarant_principal {
        return Err(DomainError::AttestationPrincipalMismatch {
            expected: cmd.declarant_principal.clone(),
            actual: cmd.attestation.signed_by.clone(),
        });
    }
    validate_beneficial_owners(&cmd.beneficial_owners)?;
    validate_effective_from(cmd.effective_from, cmd.submitted_at)?;
    Ok(())
}

fn validate_beneficial_owners(owners: &[BeneficialOwnerClaim]) -> Result<(), DomainError> {
    if owners.is_empty() {
        return Err(DomainError::NoBeneficialOwners);
    }
    let mut seen = HashSet::new();
    let mut sum: u32 = 0;
    for owner in owners {
        if !seen.insert(owner.person_id) {
            return Err(DomainError::DuplicateBeneficialOwner(owner.person_id.0));
        }
        sum = sum.saturating_add(owner.ownership_basis_points.as_basis_points());
    }
    if sum != 10_000 {
        return Err(DomainError::OwnershipSumInvariant { sum });
    }
    Ok(())
}

fn validate_effective_from(
    effective_from: time::Date,
    submitted_at: OffsetDateTime,
) -> Result<(), DomainError> {
    let submitted_date = submitted_at.date();
    if effective_from > submitted_date {
        return Err(DomainError::EffectiveFromInFuture {
            effective_from,
            submitted_at: submitted_date,
        });
    }
    let five_years_ago = submitted_at - Duration::days(365 * 5);
    if OffsetDateTime::new_utc(effective_from, time::Time::MIDNIGHT) < five_years_ago {
        return Err(DomainError::EffectiveFromTooOld {
            effective_from,
            submitted_at: submitted_date,
        });
    }
    Ok(())
}

/// BLAKE3 hash of the canonical form of the command. Used as the
/// receipt the API returns to the declarant.
fn compute_receipt_hash(cmd: &SubmitDeclaration) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        declaration_id: super::value_object::DeclarationId,
        entity_id: super::value_object::EntityId,
        declarant_principal: &'a str,
        declarant_role: &'static str,
        kind: &'static str,
        effective_from: &'a time::Date,
        beneficial_owners: &'a [BeneficialOwnerClaim],
        signature_hex: &'a str,
        nonce_hex: &'a str,
    }

    let canonical = Canonical {
        declaration_id: cmd.declaration_id,
        entity_id: cmd.entity_id,
        declarant_principal: &cmd.declarant_principal,
        declarant_role: cmd.declarant_role.as_str(),
        kind: cmd.kind.as_str(),
        effective_from: &cmd.effective_from,
        beneficial_owners: &cmd.beneficial_owners,
        signature_hex: &cmd.attestation.signature_hex,
        nonce_hex: &cmd.attestation.nonce_hex,
    };
    // serde_json serialises object keys in insertion order from the
    // derive macro; the field order above is the canonical order.
    let bytes = serde_json::to_vec(&canonical).expect("canonical fields are all serialisable");
    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use time::macros::date;
    use uuid::Uuid;

    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationKind, EntityId, InterestKind,
        OwnershipBasisPoints, PersonId,
    };

    use super::*;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[1u8; 32])
    }

    fn attestation_for(principal: &str) -> CryptographicAttestation {
        let key = signing_key();
        let payload = b"any payload - the attestation byte-payload check is at the API";
        let signature = key.sign(payload);
        CryptographicAttestation {
            signed_by: principal.to_string(),
            signature_algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: hex::encode(signature.to_bytes()),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            nonce_hex: hex::encode([7u8; 16]),
        }
    }

    fn submit_command(
        declaration_id: DeclarationId,
        owners: Vec<BeneficialOwnerClaim>,
    ) -> SubmitDeclaration {
        SubmitDeclaration {
            declaration_id,
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: "spiffe://recor.cm/test-declarant".to_string(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: owners,
            attestation: attestation_for("spiffe://recor.cm/test-declarant"),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn owner(percent_basis_points: u32) -> BeneficialOwnerClaim {
        BeneficialOwnerClaim {
            person_id: PersonId(Uuid::now_v7()),
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(
                percent_basis_points,
            )
            .unwrap(),
            interest_kind: InterestKind::Equity,
        }
    }

    #[test]
    fn fresh_aggregate_at_version_zero() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        assert_eq!(agg.version, 0);
    }

    #[test]
    fn submit_increments_version() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd).expect("valid submit");
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.version, 1);
        assert_eq!(agg.state, DeclarationState::Submitted);
    }

    #[test]
    fn submit_twice_is_rejected() {
        let mut agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd.clone()).unwrap();
        agg.apply(&event);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::AlreadySubmitted(_)));
    }

    #[test]
    fn no_beneficial_owners_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![]);
        assert_eq!(agg.handle_submit(cmd).unwrap_err(), DomainError::NoBeneficialOwners);
    }

    #[test]
    fn ownership_sum_must_equal_10000() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(5_000), owner(4_000)]);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert_eq!(err, DomainError::OwnershipSumInvariant { sum: 9_000 });
    }

    #[test]
    fn duplicate_owner_rejects() {
        let person = PersonId(Uuid::now_v7());
        let dup = |percent_basis_points: u32| BeneficialOwnerClaim {
            person_id: person,
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(
                percent_basis_points,
            )
            .unwrap(),
            interest_kind: InterestKind::Equity,
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![dup(5_000), dup(5_000)]);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::DuplicateBeneficialOwner(_)));
    }

    #[test]
    fn future_effective_from_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.effective_from = date!(2099 - 12 - 31);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::EffectiveFromInFuture { .. }));
    }

    #[test]
    fn attestation_principal_must_match() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.attestation = attestation_for("spiffe://recor.cm/different-principal");
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::AttestationPrincipalMismatch { .. }));
    }

    #[test]
    fn receipt_hash_is_stable_for_same_inputs() {
        let id = DeclarationId::new();
        // Hold submitted_at + correlation_id + nonce constant by reusing the
        // command across calls.
        let cmd = submit_command(id, vec![owner(10_000)]);
        let h1 = compute_receipt_hash(&cmd);
        let h2 = compute_receipt_hash(&cmd);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // BLAKE3 default = 32 bytes = 64 hex chars
    }

    fn verify_command(
        agg: &DeclarationAggregate,
        lane: crate::domain::value_object::VerificationLane,
    ) -> RecordVerificationOutcome {
        RecordVerificationOutcome {
            declaration_id: agg.id,
            verification_case_id: Uuid::now_v7(),
            lane,
            fused_authenticity_belief: 0.92,
            fused_authenticity_plausibility: 0.97,
            fused_risk_belief: 0.05,
            completed_at: OffsetDateTime::now_utc(),
        }
    }

    fn submitted_aggregate() -> DeclarationAggregate {
        let mut agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd).unwrap();
        agg.apply(&event);
        agg
    }

    #[test]
    fn record_verification_before_submit_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::VerificationOutcomeBeforeSubmit(_)));
    }

    #[test]
    fn green_lane_transitions_to_accepted() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let event = agg
            .handle_record_verification(cmd)
            .unwrap()
            .expect("first verification emits an event");
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::Accepted);
        assert!(agg.verification_case_id.is_some());
    }

    #[test]
    fn yellow_lane_transitions_to_in_verification() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Yellow);
        let event = agg.handle_record_verification(cmd).unwrap().unwrap();
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::InVerification);
    }

    #[test]
    fn red_lane_transitions_to_rejected() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Red);
        let event = agg.handle_record_verification(cmd).unwrap().unwrap();
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::Rejected);
    }

    #[test]
    fn replay_same_case_is_noop() {
        let mut agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let case_id = cmd.verification_case_id;
        let event = agg.handle_record_verification(cmd.clone()).unwrap().unwrap();
        agg.apply(&event);
        // Same case_id replayed
        let mut replay = cmd;
        replay.verification_case_id = case_id;
        let result = agg.handle_record_verification(replay).unwrap();
        assert!(result.is_none(), "replay must not produce a second event");
    }

    #[test]
    fn different_case_after_verified_rejects() {
        let mut agg = submitted_aggregate();
        let cmd1 = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let event = agg.handle_record_verification(cmd1).unwrap().unwrap();
        agg.apply(&event);
        let cmd2 = verify_command(&agg, crate::domain::value_object::VerificationLane::Red);
        let err = agg.handle_record_verification(cmd2).unwrap_err();
        assert!(matches!(err, DomainError::VerificationCaseMismatch { .. }));
    }

    #[test]
    fn out_of_range_belief_rejects() {
        let agg = submitted_aggregate();
        let mut cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        cmd.fused_authenticity_belief = 1.5;
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::FusedBeliefOutOfRange { .. }));
    }

    #[test]
    fn nan_belief_rejects() {
        let agg = submitted_aggregate();
        let mut cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        cmd.fused_risk_belief = f64::NAN;
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::FusedBeliefOutOfRange { .. }));
    }
}
