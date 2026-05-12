//! Supersede-declaration use case.
//!
//! Atomic two-aggregate write:
//!   1. The OLD declaration emits `DeclarationSupersededV1` (state →
//!      Superseded; `superseded_by_declaration_id` recorded).
//!   2. The NEW declaration emits `DeclarationSubmittedV1` (fresh
//!      aggregate, state → Submitted; `supersedes_declaration_id`
//!      recorded against its projection row).
//!
//! Both events + both projection writes + both outbox rows land in the
//! same Postgres transaction. Either both succeed or neither does;
//! consumers never see a half-superseded chain.
//!
//! Authorisation: the declarant principal on the supersede request
//! must match the declarant_principal stored on the OLD aggregate.
//! Cross-principal supersedes (e.g., a notary updating someone else's
//! declaration) are a future capability — they'd carry a
//! `supersede_authorisation` token signed by both parties; out of
//! scope for the R-DECL-3 slice.
//!
//! Entity invariant: the NEW declaration's `entity_id` must equal the
//! OLD declaration's `entity_id`. A declaration about entity A cannot
//! supersede a declaration about entity B.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{Instrument, info_span};

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    DeclarationAggregate, DeclarationEvent, DeclarationId, DomainError, SubmitDeclaration,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupersedeReceipt {
    pub new_declaration_id: DeclarationId,
    pub superseded_declaration_id: DeclarationId,
    pub receipt_hash_hex: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: time::OffsetDateTime,
    pub state: String,
}

#[derive(Debug, Error)]
pub enum SupersedeError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("declaration {0} not found; cannot supersede an aggregate with no events")]
    OldDeclarationNotFound(DeclarationId),
}

pub struct SupersedeDeclarationUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl SupersedeDeclarationUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            new_declaration_id = %new_command.declaration_id,
            superseded_declaration_id = %superseded_declaration_id,
            entity_id = %new_command.entity_id,
            declarant_principal = %new_command.declarant_principal,
            correlation_id = %new_command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        superseded_declaration_id: DeclarationId,
        new_command: SubmitDeclaration,
    ) -> Result<SupersedeReceipt, SupersedeError> {
        // 1. Load the OLD aggregate and verify it exists.
        let old_events = self
            .repository
            .load_events(superseded_declaration_id)
            .instrument(info_span!("load_old_events"))
            .await?;
        if old_events.is_empty() {
            return Err(SupersedeError::OldDeclarationNotFound(
                superseded_declaration_id,
            ));
        }
        let old_aggregate =
            DeclarationAggregate::from_events(superseded_declaration_id, &old_events);

        // 2. Authorise — the principal initiating the supersede must
        //    own the OLD declaration.
        let expected_owner = old_aggregate.declarant_principal.clone().ok_or_else(|| {
            SupersedeError::Domain(DomainError::SupersedeBeforeSubmit(
                superseded_declaration_id.0,
            ))
        })?;
        if expected_owner != new_command.declarant_principal {
            return Err(SupersedeError::Domain(DomainError::SupersedeNotOwner {
                declaration_id: superseded_declaration_id.0,
                expected: expected_owner,
                actual: new_command.declarant_principal.clone(),
            }));
        }

        // 3. Entity invariant — same entity.
        let expected_entity = old_aggregate.entity_id.ok_or_else(|| {
            SupersedeError::Domain(DomainError::SupersedeBeforeSubmit(
                superseded_declaration_id.0,
            ))
        })?;
        if expected_entity != new_command.entity_id {
            return Err(SupersedeError::Domain(DomainError::SupersedeEntityMismatch {
                old_entity_id: expected_entity.0,
                new_entity_id: new_command.entity_id.0,
            }));
        }

        // 4. Build the NEW aggregate and produce its Submitted event.
        let new_id = new_command.declaration_id;
        let correlation_id = new_command.correlation_id;
        let new_aggregate = DeclarationAggregate::fresh(new_id);
        let new_event = new_aggregate.handle_submit(new_command)?;

        // 5. Produce the OLD aggregate's Superseded event.
        let old_event = old_aggregate.handle_supersede(new_id, correlation_id)?;

        // 6. Atomically persist both event streams + both projections
        //    + both outbox rows.
        let DeclarationEvent::Submitted(new_payload) = &new_event else {
            // Defensive: handle_submit can only produce Submitted.
            return Err(SupersedeError::Domain(DomainError::EmptyDeclarantPrincipal));
        };
        let submitted_at = new_payload.submitted_at;
        let receipt_hash_hex = new_payload.receipt_hash_hex.clone();

        self.repository
            .save_supersede(
                &new_event,
                new_aggregate.version,
                superseded_declaration_id,
                &old_event,
                old_aggregate.version,
            )
            .instrument(info_span!("save_supersede"))
            .await?;

        Ok(SupersedeReceipt {
            new_declaration_id: new_id,
            superseded_declaration_id,
            receipt_hash_hex,
            submitted_at,
            state: "submitted".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ed25519_dalek::{Signer, SigningKey};
    use time::OffsetDateTime;
    use time::macros::date;
    use uuid::Uuid;

    use crate::application::DeclarationProjection;
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::InterestKind;
    use crate::domain::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationKind, EntityId, OwnershipBasisPoints,
        PersonId, RecordVerificationOutcome, VerificationLane,
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
            if stream.len() as u64 != expected_version {
                return Err(RepositoryError::Conflict {
                    expected: expected_version,
                    found: stream.len() as u64,
                });
            }
            stream.push(event.clone());
            Ok(())
        }

        async fn save_supersede(
            &self,
            new_event: &DeclarationEvent,
            new_expected_version: u64,
            old_id: DeclarationId,
            old_event: &DeclarationEvent,
            old_expected_version: u64,
        ) -> Result<(), RepositoryError> {
            // In-memory mock: emulate atomicity by writing both or
            // neither — re-check both versions before mutating.
            let mut guard = self.events.lock().unwrap();
            let new_id = new_event.declaration_id().0;
            let new_stream = guard.entry(new_id).or_default();
            if new_stream.len() as u64 != new_expected_version {
                return Err(RepositoryError::Conflict {
                    expected: new_expected_version,
                    found: new_stream.len() as u64,
                });
            }
            let old_stream = guard.get(&old_id.0).cloned().unwrap_or_default();
            if old_stream.len() as u64 != old_expected_version {
                return Err(RepositoryError::Conflict {
                    expected: old_expected_version,
                    found: old_stream.len() as u64,
                });
            }
            // Commit both.
            guard.entry(new_id).or_default().push(new_event.clone());
            guard.entry(old_id.0).or_default().push(old_event.clone());
            Ok(())
        }

        async fn load_projection(
            &self,
            _id: DeclarationId,
        ) -> Result<Option<DeclarationProjection>, RepositoryError> {
            Ok(None)
        }
    }

    fn submit_cmd(id: DeclarationId, entity: EntityId, principal: &str) -> SubmitDeclaration {
        let key = SigningKey::from_bytes(&[11u8; 32]);
        let sig = key.sign(b"x");
        SubmitDeclaration {
            declaration_id: id,
            entity_id: entity,
            declarant_principal: principal.into(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![BeneficialOwnerClaim {
                person_id: PersonId(Uuid::now_v7()),
                ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(10_000)
                    .unwrap(),
                interest_kind: InterestKind::Equity,
            }],
            attestation: CryptographicAttestation {
                signed_by: principal.into(),
                signature_algorithm: SignatureAlgorithm::Ed25519,
                signature_hex: hex::encode(sig.to_bytes()),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                nonce_hex: hex::encode([1u8; 16]),
            },
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    async fn seed_accepted_declaration(
        repo: &Arc<InMemoryRepo>,
        principal: &str,
    ) -> (DeclarationId, EntityId) {
        use crate::application::SubmitDeclarationUseCase;
        use crate::application::RecordVerificationOutcomeUseCase;
        let submit_uc = SubmitDeclarationUseCase::new(repo.clone());
        let verify_uc = RecordVerificationOutcomeUseCase::new(repo.clone());
        let id = DeclarationId::new();
        let entity = EntityId(Uuid::now_v7());
        submit_uc.execute(submit_cmd(id, entity, principal)).await.unwrap();
        verify_uc
            .execute(RecordVerificationOutcome {
                declaration_id: id,
                verification_case_id: Uuid::now_v7(),
                lane: VerificationLane::Green,
                fused_authenticity_belief: 0.92,
                fused_authenticity_plausibility: 0.97,
                fused_risk_belief: 0.05,
                completed_at: OffsetDateTime::now_utc(),
            })
            .await
            .unwrap();
        (id, entity)
    }

    #[tokio::test]
    async fn supersede_happy_path_produces_receipt() {
        let repo = Arc::new(InMemoryRepo::default());
        let (old_id, entity) = seed_accepted_declaration(&repo, "spiffe://recor.cm/alice").await;
        let usecase = SupersedeDeclarationUseCase::new(repo.clone());

        let new_id = DeclarationId::new();
        let new_cmd = submit_cmd(new_id, entity, "spiffe://recor.cm/alice");
        let receipt = usecase.execute(old_id, new_cmd).await.unwrap();

        assert_eq!(receipt.new_declaration_id, new_id);
        assert_eq!(receipt.superseded_declaration_id, old_id);
        assert_eq!(receipt.state, "submitted");

        // Both aggregates should have moved.
        let events = repo.events.lock().unwrap();
        let old_stream = events.get(&old_id.0).unwrap();
        let new_stream = events.get(&new_id.0).unwrap();
        assert_eq!(old_stream.len(), 3); // Submitted + Verified + Superseded
        assert_eq!(new_stream.len(), 1); // Submitted
        assert!(matches!(
            old_stream.last().unwrap(),
            DeclarationEvent::Superseded(_)
        ));
    }

    #[tokio::test]
    async fn supersede_rejected_when_principal_does_not_own_old() {
        let repo = Arc::new(InMemoryRepo::default());
        let (old_id, entity) = seed_accepted_declaration(&repo, "spiffe://recor.cm/alice").await;
        let usecase = SupersedeDeclarationUseCase::new(repo);

        let mut new_cmd =
            submit_cmd(DeclarationId::new(), entity, "spiffe://recor.cm/bob");
        new_cmd.attestation.signed_by = "spiffe://recor.cm/bob".into();
        let err = usecase.execute(old_id, new_cmd).await.unwrap_err();
        assert!(matches!(
            err,
            SupersedeError::Domain(DomainError::SupersedeNotOwner { .. })
        ));
    }

    #[tokio::test]
    async fn supersede_rejected_when_entity_differs() {
        let repo = Arc::new(InMemoryRepo::default());
        let (old_id, _entity) = seed_accepted_declaration(&repo, "spiffe://recor.cm/alice").await;
        let usecase = SupersedeDeclarationUseCase::new(repo);

        let different_entity = EntityId(Uuid::now_v7());
        let new_cmd = submit_cmd(
            DeclarationId::new(),
            different_entity,
            "spiffe://recor.cm/alice",
        );
        let err = usecase.execute(old_id, new_cmd).await.unwrap_err();
        assert!(matches!(
            err,
            SupersedeError::Domain(DomainError::SupersedeEntityMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn supersede_rejected_when_old_not_found() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = SupersedeDeclarationUseCase::new(repo);
        let entity = EntityId(Uuid::now_v7());
        let new_cmd = submit_cmd(DeclarationId::new(), entity, "spiffe://recor.cm/alice");
        let err = usecase
            .execute(DeclarationId::new(), new_cmd)
            .await
            .unwrap_err();
        assert!(matches!(err, SupersedeError::OldDeclarationNotFound(_)));
    }

    #[tokio::test]
    async fn supersede_twice_rejected() {
        let repo = Arc::new(InMemoryRepo::default());
        let (old_id, entity) = seed_accepted_declaration(&repo, "spiffe://recor.cm/alice").await;
        let usecase = SupersedeDeclarationUseCase::new(repo.clone());
        // First supersede succeeds.
        let first_new = DeclarationId::new();
        usecase
            .execute(old_id, submit_cmd(first_new, entity, "spiffe://recor.cm/alice"))
            .await
            .unwrap();
        // Second supersede of the same OLD must reject.
        let second_new = DeclarationId::new();
        let err = usecase
            .execute(old_id, submit_cmd(second_new, entity, "spiffe://recor.cm/alice"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SupersedeError::Domain(DomainError::AlreadySuperseded(_))
        ));
    }

    #[tokio::test]
    async fn supersede_rejected_when_old_not_accepted() {
        // OLD declaration was submitted but not yet verified (state =
        // Submitted, not Accepted/InVerification) — supersede should
        // refuse because the old declaration isn't authoritative yet.
        let repo = Arc::new(InMemoryRepo::default());
        use crate::application::SubmitDeclarationUseCase;
        let submit_uc = SubmitDeclarationUseCase::new(repo.clone());
        let old_id = DeclarationId::new();
        let entity = EntityId(Uuid::now_v7());
        submit_uc
            .execute(submit_cmd(old_id, entity, "spiffe://recor.cm/alice"))
            .await
            .unwrap();
        // No verify call — state stays Submitted.
        let usecase = SupersedeDeclarationUseCase::new(repo);
        let err = usecase
            .execute(
                old_id,
                submit_cmd(DeclarationId::new(), entity, "spiffe://recor.cm/alice"),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SupersedeError::Domain(DomainError::SupersedeFromInvalidState { .. })
        ));
    }
}
