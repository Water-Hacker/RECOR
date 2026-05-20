//! Correct-declaration use case.
//!
//! Pre-verification metadata correction. Sibling of `AmendDeclaration`
//! with a stricter state-machine rule: only the `Submitted` state
//! admits corrections. Once the verification engine has touched the
//! aggregate (any state other than Submitted) callers must use Amend
//! or Supersede instead.
//!
//! Like Amend, every correction carries a fresh Ed25519 attestation
//! over the corrected metadata bytes — Doctrine 15 (cryptographic
//! provenance on every consequential event) holds even when the
//! canonical declaration body is unchanged.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{Instrument, info_span};

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    CorrectDeclaration, DeclarationAggregate, DeclarationEvent, DeclarationId, DomainError,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectReceipt {
    pub declaration_id: DeclarationId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub corrected_at: OffsetDateTime,
    pub aggregate_version: u64,
}

#[derive(Debug, Error)]
pub enum CorrectError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("declaration {0} not found; cannot correct an aggregate with no events")]
    NotFound(DeclarationId),
}

pub struct CorrectDeclarationUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl CorrectDeclarationUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            declaration_id = %command.declaration_id,
            declarant_principal = %command.declarant_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: CorrectDeclaration,
    ) -> Result<CorrectReceipt, CorrectError> {
        let id = command.declaration_id;

        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(CorrectError::NotFound(id));
        }
        let aggregate = DeclarationAggregate::from_events(id, &events);
        let event = aggregate.handle_correct(command)?;
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let DeclarationEvent::Corrected(payload) = &event else {
            return Err(CorrectError::Domain(DomainError::EmptyDeclarantPrincipal));
        };
        Ok(CorrectReceipt {
            declaration_id: payload.declaration_id,
            corrected_at: payload.corrected_at,
            aggregate_version: aggregate.version.saturating_add(1),
        })
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

    use crate::application::{DeclarationProjection, SubmitDeclarationUseCase};
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::InterestKind;
    use crate::domain::{
        BeneficialOwnerClaim, CorrectionSet, DeclarantRole, DeclarationKind, EntityId,
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
            _new_event: &DeclarationEvent,
            _new_expected_version: u64,
            _old_id: DeclarationId,
            _old_event: &DeclarationEvent,
            _old_expected_version: u64,
        ) -> Result<(), RepositoryError> {
            unimplemented!("correct tests don't exercise the supersede path")
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
    }

    const PRINCIPAL: &str = "spiffe://recor.cm/correct-test-declarant";

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[29u8; 32])
    }

    fn attestation_for(principal: &str) -> CryptographicAttestation {
        let key = signing_key();
        let signature = key.sign(b"x");
        CryptographicAttestation {
            signed_by: principal.to_string(),
            signature_algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: hex::encode(signature.to_bytes()),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            nonce_hex: hex::encode([5u8; 16]),
        }
    }

    fn submit_cmd(id: DeclarationId, entity: EntityId) -> SubmitDeclaration {
        SubmitDeclaration {
            declaration_id: id,
            entity_id: entity,
            declarant_principal: PRINCIPAL.into(),
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
            attestation: attestation_for(PRINCIPAL),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            adequacy_claims: None,
            last_event_observed_at: None,
        }
    }

    fn correct_cmd(
        id: DeclarationId,
        principal: &str,
        corrections: CorrectionSet,
    ) -> CorrectDeclaration {
        CorrectDeclaration {
            declaration_id: id,
            declarant_principal: principal.to_string(),
            corrections,
            attestation: attestation_for(principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    async fn seed_submitted(repo: &Arc<InMemoryRepo>) -> DeclarationId {
        let submit_uc = SubmitDeclarationUseCase::new(repo.clone());
        let id = DeclarationId::new();
        submit_uc
            .execute(submit_cmd(id, EntityId(Uuid::now_v7())))
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn correct_happy_path_returns_receipt_and_appends_event() {
        let repo = Arc::new(InMemoryRepo::default());
        let id = seed_submitted(&repo).await;
        let usecase = CorrectDeclarationUseCase::new(repo.clone());
        let receipt = usecase
            .execute(correct_cmd(
                id,
                PRINCIPAL,
                CorrectionSet { metadata_notes: Some("typo in cover note".into()) },
            ))
            .await
            .unwrap();
        assert_eq!(receipt.declaration_id, id);
        assert_eq!(receipt.aggregate_version, 2);
        let events = repo.events.lock().unwrap();
        let stream = events.get(&id.0).unwrap();
        assert_eq!(stream.len(), 2);
        assert!(matches!(stream.last().unwrap(), DeclarationEvent::Corrected(_)));
    }

    #[tokio::test]
    async fn correct_refused_when_declaration_not_found() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = CorrectDeclarationUseCase::new(repo);
        let err = usecase
            .execute(correct_cmd(
                DeclarationId::new(),
                PRINCIPAL,
                CorrectionSet { metadata_notes: Some("x".into()) },
            ))
            .await
            .unwrap_err();
        assert!(matches!(err, CorrectError::NotFound(_)));
    }

    #[tokio::test]
    async fn correct_refused_in_invalid_state() {
        let repo = Arc::new(InMemoryRepo::default());
        let id = seed_submitted(&repo).await;
        use crate::application::RecordVerificationOutcomeUseCase;
        let verify_uc = RecordVerificationOutcomeUseCase::new(repo.clone());
        verify_uc
            .execute(crate::domain::RecordVerificationOutcome {
                declaration_id: id,
                verification_case_id: Uuid::now_v7(),
                lane: crate::domain::VerificationLane::Yellow,
                fused_authenticity_belief: 0.7,
                fused_authenticity_plausibility: 0.9,
                fused_risk_belief: 0.3,
                completed_at: OffsetDateTime::now_utc(),
            })
            .await
            .unwrap();
        let usecase = CorrectDeclarationUseCase::new(repo);
        let err = usecase
            .execute(correct_cmd(
                id,
                PRINCIPAL,
                CorrectionSet { metadata_notes: Some("x".into()) },
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            CorrectError::Domain(DomainError::CorrectFromInvalidState { .. })
        ));
    }
}
