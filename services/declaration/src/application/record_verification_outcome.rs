//! Record-verification-outcome use case.
//!
//! Called by the internal /v1/internal/verification-outcomes endpoint
//! after HMAC verification. Hydrates the aggregate, asks it to handle
//! the RecordVerificationOutcome command, and — if the command emits a
//! new event — persists it through the same atomic event + projection
//! + outbox path used by submission.
//!
//! Idempotency: the aggregate's `handle_record_verification` returns
//! `Ok(None)` when the same case_id has already been applied. The use
//! case turns this into a successful no-op ACK so the verification
//! engine's outbox relay marks the row dispatched on a replay.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{Instrument, info_span};

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    DeclarationAggregate, DeclarationId, DomainError, RecordVerificationOutcome,
    VerificationLane,
};

/// Result returned to the API layer on a successful (or replayed) write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordVerificationReceipt {
    pub declaration_id: DeclarationId,
    pub verification_case_id: uuid::Uuid,
    pub lane: VerificationLane,
    /// Whether this call wrote a new event (true) or recognised a replay
    /// of an already-applied case (false). The API layer uses this to
    /// pick 201 vs 200.
    pub recorded_new_event: bool,
}

#[derive(Debug, Error)]
pub enum RecordVerificationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct RecordVerificationOutcomeUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl RecordVerificationOutcomeUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            declaration_id = %command.declaration_id,
            verification_case_id = %command.verification_case_id,
            lane = command.lane.as_str(),
        )
    )]
    pub async fn execute(
        &self,
        command: RecordVerificationOutcome,
    ) -> Result<RecordVerificationReceipt, RecordVerificationError> {
        let id = command.declaration_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        let aggregate = DeclarationAggregate::from_events(id, &events);

        let case_id = command.verification_case_id;
        let lane = command.lane;
        let maybe_event = aggregate.handle_record_verification(command)?;

        match maybe_event {
            Some(event) => {
                self.repository
                    .save_event(&event, aggregate.version)
                    .instrument(info_span!("save_event"))
                    .await?;
                Ok(RecordVerificationReceipt {
                    declaration_id: id,
                    verification_case_id: case_id,
                    lane,
                    recorded_new_event: true,
                })
            }
            None => Ok(RecordVerificationReceipt {
                declaration_id: id,
                verification_case_id: case_id,
                lane,
                recorded_new_event: false,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ed25519_dalek::{Signer, SigningKey};
    use time::macros::date;
    use time::OffsetDateTime;
    use uuid::Uuid;

    use crate::application::DeclarationProjection;
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::InterestKind;
    use crate::domain::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationEvent, DeclarationKind, EntityId,
        OwnershipBasisPoints, PersonId, SubmitDeclaration,
    };

    use super::*;

    #[derive(Default)]
    struct InMemoryRepo {
        events: Mutex<HashMap<Uuid, Vec<DeclarationEvent>>>,
    }

    #[async_trait]
    impl DeclarationRepository for InMemoryRepo {
        async fn load_events(
            &self,
            id: DeclarationId,
        ) -> Result<Vec<DeclarationEvent>, RepositoryError> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .get(&id.0)
                .cloned()
                .unwrap_or_default())
        }

        async fn save_event(
            &self,
            event: &DeclarationEvent,
            expected_version: u64,
        ) -> Result<(), RepositoryError> {
            let id = event.declaration_id();
            let mut guard = self.events.lock().unwrap();
            let stream = guard.entry(id.0).or_default();
            let current = stream.len() as u64;
            if current != expected_version {
                return Err(RepositoryError::Conflict {
                    expected: expected_version,
                    found: current,
                });
            }
            stream.push(event.clone());
            Ok(())
        }

        async fn load_projection(
            &self,
            _id: DeclarationId,
        ) -> Result<Option<DeclarationProjection>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_principal(
            &self,
            _principal: &str,
        ) -> Result<Vec<DeclarationProjection>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn save_supersede(
            &self,
            _new_event: &DeclarationEvent,
            _new_expected_version: u64,
            _old_id: DeclarationId,
            _old_event: &DeclarationEvent,
            _old_expected_version: u64,
        ) -> Result<(), RepositoryError> {
            unimplemented!("record_verification_outcome tests don't exercise supersede")
        }
    }

    fn submit_cmd(id: DeclarationId) -> SubmitDeclaration {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let sig = key.sign(b"x");
        SubmitDeclaration {
            declaration_id: id,
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: "spiffe://recor.cm/t".into(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![BeneficialOwnerClaim {
                person_id: PersonId(Uuid::now_v7()),
                ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(10_000)
                    .unwrap(),
                interest_kind: InterestKind::Equity,
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
            }],
            attestation: CryptographicAttestation {
                signed_by: "spiffe://recor.cm/t".into(),
                signature_algorithm: SignatureAlgorithm::Ed25519,
                signature_hex: hex::encode(sig.to_bytes()),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                nonce_hex: hex::encode([0u8; 16]),
            },
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            adequacy_claims: None,
            last_event_observed_at: None,
        }
    }

    async fn submitted_aggregate(repo: &Arc<InMemoryRepo>, id: DeclarationId) {
        use crate::application::SubmitDeclarationUseCase;
        let usecase = SubmitDeclarationUseCase::new(repo.clone());
        usecase.execute(submit_cmd(id)).await.unwrap();
    }

    fn verify_cmd(id: DeclarationId, lane: VerificationLane) -> RecordVerificationOutcome {
        RecordVerificationOutcome {
            declaration_id: id,
            verification_case_id: Uuid::now_v7(),
            lane,
            fused_authenticity_belief: 0.92,
            fused_authenticity_plausibility: 0.97,
            fused_risk_belief: 0.05,
            completed_at: OffsetDateTime::now_utc(),
            // TODO-050 — these legacy use-case-level tests pre-date the
            // correlation_id cross-check; nil is treated as a legacy
            // envelope and the cross-check is skipped, keeping them
            // green. The matching / mismatching paths are covered in
            // `domain::aggregate::tests` and `api::internal::tests`.
            correlation_id: Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn writeback_records_event_on_first_call() {
        let repo = Arc::new(InMemoryRepo::default());
        let id = DeclarationId::new();
        submitted_aggregate(&repo, id).await;

        let usecase = RecordVerificationOutcomeUseCase::new(repo.clone());
        let cmd = verify_cmd(id, VerificationLane::Green);
        let receipt = usecase.execute(cmd).await.unwrap();
        assert!(receipt.recorded_new_event);
        assert_eq!(receipt.lane, VerificationLane::Green);
        let stored = repo.events.lock().unwrap();
        assert_eq!(stored.get(&id.0).unwrap().len(), 2); // submitted + verified
    }

    #[tokio::test]
    async fn replay_same_case_id_is_idempotent_ack() {
        let repo = Arc::new(InMemoryRepo::default());
        let id = DeclarationId::new();
        submitted_aggregate(&repo, id).await;

        let usecase = RecordVerificationOutcomeUseCase::new(repo.clone());
        let cmd = verify_cmd(id, VerificationLane::Green);
        let case_id = cmd.verification_case_id;
        usecase.execute(cmd.clone()).await.unwrap();

        let mut replay = cmd;
        replay.verification_case_id = case_id;
        let receipt = usecase.execute(replay).await.unwrap();
        assert!(!receipt.recorded_new_event);
        let stored = repo.events.lock().unwrap();
        assert_eq!(stored.get(&id.0).unwrap().len(), 2); // no third event
    }

    #[tokio::test]
    async fn writeback_without_prior_submit_rejects() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = RecordVerificationOutcomeUseCase::new(repo);
        let cmd = verify_cmd(DeclarationId::new(), VerificationLane::Green);
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(
            err,
            RecordVerificationError::Domain(DomainError::VerificationOutcomeBeforeSubmit(_))
        ));
    }

    #[tokio::test]
    async fn different_case_after_verified_rejects() {
        let repo = Arc::new(InMemoryRepo::default());
        let id = DeclarationId::new();
        submitted_aggregate(&repo, id).await;
        let usecase = RecordVerificationOutcomeUseCase::new(repo);
        usecase
            .execute(verify_cmd(id, VerificationLane::Green))
            .await
            .unwrap();
        let err = usecase
            .execute(verify_cmd(id, VerificationLane::Red))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            RecordVerificationError::Domain(DomainError::VerificationCaseMismatch { .. })
        ));
    }
}
