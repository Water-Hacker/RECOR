//! Submit-declaration use case.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{Instrument, info_span};

use crate::application::port::{
    DeclarationRepository, PersonRegistryError, PersonRegistryPort, RepositoryError,
};
use crate::domain::{
    DeclarationAggregate, DeclarationEvent, DeclarationId, DeclarationSubmittedV1, DomainError,
    SubmitDeclaration,
};

/// Receipt returned to the API layer on successful submission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmitReceipt {
    pub declaration_id: DeclarationId,
    pub receipt_hash_hex: String,
    pub submitted_at: OffsetDateTime,
    pub state: String,
}

#[derive(Debug, Error)]
pub enum SubmitError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("person registry unavailable: {0}")]
    PersonRegistry(#[from] PersonRegistryError),
}

/// Use case object — a thin orchestrator over the repository port.
pub struct SubmitDeclarationUseCase {
    repository: Arc<dyn DeclarationRepository>,
    /// R-DECL-4: optional gate that validates each
    /// `beneficial_owner.person_id` against the Person registry. `None`
    /// (the test/dev default while `PERSON_SERVICE_URL` is empty) skips
    /// the check. Production wiring uses
    /// `PersonRegistryHttpAdapter`.
    person_registry: Option<Arc<dyn PersonRegistryPort>>,
}

impl SubmitDeclarationUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self {
            repository,
            person_registry: None,
        }
    }

    /// Builder-style: attach a Person registry port. Once attached,
    /// `execute` validates every `beneficial_owner.person_id` against
    /// the registry before persisting the event. A negative answer
    /// surfaces as `DomainError::BeneficialOwnerNotInPersonRegistry`.
    #[must_use]
    pub fn with_person_registry(mut self, registry: Arc<dyn PersonRegistryPort>) -> Self {
        self.person_registry = Some(registry);
        self
    }

    #[tracing::instrument(
        skip_all,
        fields(
            declaration_id = %command.declaration_id,
            entity_id = %command.entity_id,
            declarant_principal = %command.declarant_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: SubmitDeclaration,
    ) -> Result<SubmitReceipt, SubmitError> {
        let id = command.declaration_id;
        // Hydrate aggregate from existing events (zero or more).
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        let aggregate = DeclarationAggregate::from_events(id, &events);

        // R-DECL-4 cross-service validation: each beneficial_owner.person_id
        // must resolve in the Person registry. Gated behind an Option<_> so
        // dev/test stays end-to-end exercisable without a second service.
        // Run BEFORE the aggregate's domain validation so an unknown person
        // surfaces as the more-precise BeneficialOwnerNotInPersonRegistry
        // error rather than a generic ownership invariant failure.
        if let Some(registry) = self.person_registry.as_ref() {
            for owner in &command.beneficial_owners {
                let exists = registry
                    .exists(owner.person_id.0)
                    .instrument(info_span!(
                        "person_registry.exists",
                        person_id = %owner.person_id
                    ))
                    .await?;
                if !exists {
                    return Err(SubmitError::Domain(
                        DomainError::BeneficialOwnerNotInPersonRegistry(owner.person_id.0),
                    ));
                }
            }
        }

        // Validate + produce event.
        let event = aggregate.handle_submit(command)?;

        // Persist event + projection + outbox row (atomic).
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        // The aggregate's handle_submit only produces `Submitted`. This
        // `let-else` is defensive — if a future variant is added we'd
        // surface it as an internal error rather than panic.
        let DeclarationEvent::Submitted(payload) = &event else {
            return Err(SubmitError::Domain(DomainError::EmptyDeclarantPrincipal));
        };
        let receipt = SubmitReceipt {
            declaration_id: payload.declaration_id,
            receipt_hash_hex: payload.receipt_hash_hex.clone(),
            submitted_at: payload.submitted_at,
            state: "submitted".to_string(),
        };
        Ok(receipt)
    }
}

/// Helper for the API layer to derive a receipt from a stored event
/// (used when an idempotency replay returns the same answer as the
/// original).
#[must_use]
pub fn receipt_from_event(event: &DeclarationSubmittedV1) -> SubmitReceipt {
    SubmitReceipt {
        declaration_id: event.declaration_id,
        receipt_hash_hex: event.receipt_hash_hex.clone(),
        submitted_at: event.submitted_at,
        state: "submitted".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ed25519_dalek::{Signer, SigningKey};
    use time::macros::date;
    use uuid::Uuid;

    use crate::application::DeclarationProjection;
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationKind, DeclarationState, EntityId,
        OwnershipBasisPoints, PersonId,
    };
    use crate::domain::value_object::InterestKind;

    use super::*;

    /// In-memory repository double; deterministic for unit testing.
    #[derive(Default)]
    struct InMemoryRepo {
        // declaration_id -> events
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
            // not exercised by these tests
            Ok(None)
        }

        async fn find_by_principal(
            &self,
            _principal: &str,
        ) -> Result<Vec<DeclarationProjection>, RepositoryError> {
            // not exercised by these tests
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
            unimplemented!("submit_declaration tests don't exercise the supersede path")
        }
    }

    fn make_cmd(declaration_id: DeclarationId) -> SubmitDeclaration {
        let key = SigningKey::from_bytes(&[3u8; 32]);
        let payload = b"x";
        let signature = key.sign(payload);
        let person = PersonId(Uuid::now_v7());
        SubmitDeclaration {
            declaration_id,
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: "spiffe://recor.cm/test".into(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![BeneficialOwnerClaim {
                person_id: person,
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
                signed_by: "spiffe://recor.cm/test".into(),
                signature_algorithm: SignatureAlgorithm::Ed25519,
                signature_hex: hex::encode(signature.to_bytes()),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                nonce_hex: hex::encode([0u8; 16]),
            },
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
            adequacy_claims: None,
}

    #[tokio::test]
    async fn happy_path_submits_and_returns_receipt() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = SubmitDeclarationUseCase::new(repo.clone());
        let cmd = make_cmd(DeclarationId::new());
        let receipt = usecase.execute(cmd.clone()).await.expect("submit");
        assert_eq!(receipt.state, "submitted");
        assert_eq!(receipt.declaration_id, cmd.declaration_id);
        assert!(!receipt.receipt_hash_hex.is_empty());
        assert_eq!(receipt.receipt_hash_hex.len(), 64);
        let stored = repo.events.lock().unwrap();
        assert_eq!(stored.get(&cmd.declaration_id.0).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_submit_rejects_with_domain_error() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = SubmitDeclarationUseCase::new(repo.clone());
        let cmd = make_cmd(DeclarationId::new());
        usecase.execute(cmd.clone()).await.unwrap();
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(err, SubmitError::Domain(DomainError::AlreadySubmitted(_))));
    }

    #[tokio::test]
    async fn invalid_command_propagates_domain_error() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = SubmitDeclarationUseCase::new(repo);
        let mut cmd = make_cmd(DeclarationId::new());
        cmd.beneficial_owners.clear();
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(err, SubmitError::Domain(DomainError::NoBeneficialOwners)));
    }

    // ─── R-DECL-4: Person registry validation ─────────────────────────────

    /// A stub that returns the same answer for every person id.
    struct StaticRegistry {
        answer: bool,
    }

    #[async_trait]
    impl crate::application::port::PersonRegistryPort for StaticRegistry {
        async fn exists(
            &self,
            _person_id: uuid::Uuid,
        ) -> Result<bool, crate::application::port::PersonRegistryError> {
            Ok(self.answer)
        }
    }

    #[tokio::test]
    async fn person_registry_denial_surfaces_beneficial_owner_not_in_registry() {
        let repo = Arc::new(InMemoryRepo::default());
        let registry: Arc<dyn crate::application::port::PersonRegistryPort> =
            Arc::new(StaticRegistry { answer: false });
        let usecase = SubmitDeclarationUseCase::new(repo).with_person_registry(registry);
        let cmd = make_cmd(DeclarationId::new());
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(
            err,
            SubmitError::Domain(DomainError::BeneficialOwnerNotInPersonRegistry(_))
        ));
    }

    #[tokio::test]
    async fn person_registry_acceptance_admits_submission() {
        let repo = Arc::new(InMemoryRepo::default());
        let registry: Arc<dyn crate::application::port::PersonRegistryPort> =
            Arc::new(StaticRegistry { answer: true });
        let usecase = SubmitDeclarationUseCase::new(repo).with_person_registry(registry);
        let cmd = make_cmd(DeclarationId::new());
        let receipt = usecase.execute(cmd).await.expect("registry-accepted submit");
        assert_eq!(receipt.state, "submitted");
    }

    // Suppress the "DeclarationState used by transitive imports" warning.
    #[allow(dead_code)]
    fn _force_imports(_s: DeclarationState) {}
}
