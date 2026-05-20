//! Amend-declaration use case.
//!
//! Atomic single-aggregate write: the amend produces a
//! `DeclarationAmendedV1` event that is appended to the declaration's
//! event log and projected back onto the `declarations` row by
//! re-writing the amendable columns (`beneficial_owners`,
//! `effective_from`, `declarant_role`) plus the new `amended_at`
//! timestamp. The aggregate's lifecycle state is unchanged — Amend is
//! only admitted from `Submitted` or `InVerification`.
//!
//! Authorisation: the declarant principal on the amend command must
//! match the declarant_principal stored on the aggregate (the original
//! submitter). The principal arrives via the authenticated session
//! (D17 zero trust); the request body never carries it.
//!
//! Attestation: every amendment carries a fresh Ed25519 signature
//! over the AMENDED canonical form. The API layer canonicalises the
//! amended payload and calls `attestation.verify_against(...)` before
//! the command reaches this use case (D15 cryptographic provenance).
//! The aggregate records the attestation on the emitted event so a
//! replay re-verifies the byte-parity property.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{Instrument, info_span};

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::domain::{
    AmendDeclaration, DeclarationAggregate, DeclarationEvent, DeclarationId, DomainError,
};

/// Receipt returned to the API on a successful amend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmendReceipt {
    pub declaration_id: DeclarationId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub amended_at: OffsetDateTime,
    pub aggregate_version: u64,
}

#[derive(Debug, Error)]
pub enum AmendError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("declaration {0} not found; cannot amend an aggregate with no events")]
    NotFound(DeclarationId),
}

pub struct AmendDeclarationUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl AmendDeclarationUseCase {
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
        command: AmendDeclaration,
    ) -> Result<AmendReceipt, AmendError> {
        let id = command.declaration_id;

        // Hydrate the aggregate from its event log.
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(AmendError::NotFound(id));
        }
        let aggregate = DeclarationAggregate::from_events(id, &events);

        // Validate + produce the Amended event.
        let event = aggregate.handle_amend(command)?;

        // Persist event + projection update + outbox row, atomically.
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        // Defensive: handle_amend can only produce Amended.
        let DeclarationEvent::Amended(payload) = &event else {
            return Err(AmendError::Domain(DomainError::EmptyDeclarantPrincipal));
        };
        Ok(AmendReceipt {
            declaration_id: payload.declaration_id,
            amended_at: payload.amended_at,
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
        AmendmentSet, BeneficialOwnerClaim, DeclarantRole, DeclarationKind, EntityId,
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
            unimplemented!("amend tests don't exercise the supersede path")
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

    const PRINCIPAL: &str = "spiffe://recor.cm/amend-test-declarant";

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[19u8; 32])
    }

    fn attestation_for(principal: &str) -> CryptographicAttestation {
        let key = signing_key();
        let signature = key.sign(b"x");
        CryptographicAttestation {
            signed_by: principal.to_string(),
            signature_algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: hex::encode(signature.to_bytes()),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            nonce_hex: hex::encode([3u8; 16]),
        }
    }

    fn owner(bp: u32) -> BeneficialOwnerClaim {
        BeneficialOwnerClaim {
            person_id: PersonId(Uuid::now_v7()),
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(bp).unwrap(),
            interest_kind: InterestKind::Equity,
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
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
            beneficial_owners: vec![owner(10_000)],
            attestation: attestation_for(PRINCIPAL),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            adequacy_claims: None,
            last_event_observed_at: None,
        }
    }

    fn amend_cmd(id: DeclarationId, principal: &str, amendments: AmendmentSet) -> AmendDeclaration {
        AmendDeclaration {
            declaration_id: id,
            declarant_principal: principal.to_string(),
            amendments,
            attestation: attestation_for(principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn default_amendments() -> AmendmentSet {
        AmendmentSet {
            beneficial_owners: vec![owner(6_000), owner(4_000)],
            effective_from: date!(2026 - 02 - 01),
            declarant_role: DeclarantRole::AuthorisedAgent,
            adequacy_claims: None,
        }
    }

    async fn seed_submitted(repo: &Arc<InMemoryRepo>) -> (DeclarationId, EntityId) {
        let submit_uc = SubmitDeclarationUseCase::new(repo.clone());
        let id = DeclarationId::new();
        let entity = EntityId(Uuid::now_v7());
        submit_uc.execute(submit_cmd(id, entity)).await.unwrap();
        (id, entity)
    }

    #[tokio::test]
    async fn amend_happy_path_returns_receipt_and_appends_event() {
        let repo = Arc::new(InMemoryRepo::default());
        let (id, _) = seed_submitted(&repo).await;
        let usecase = AmendDeclarationUseCase::new(repo.clone());
        let receipt = usecase
            .execute(amend_cmd(id, PRINCIPAL, default_amendments()))
            .await
            .unwrap();
        assert_eq!(receipt.declaration_id, id);
        // Submitted (v1) + Amended (v2) = aggregate_version 2.
        assert_eq!(receipt.aggregate_version, 2);
        let events = repo.events.lock().unwrap();
        let stream = events.get(&id.0).unwrap();
        assert_eq!(stream.len(), 2);
        assert!(matches!(stream.last().unwrap(), DeclarationEvent::Amended(_)));
    }

    #[tokio::test]
    async fn amend_refused_when_declaration_not_found() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = AmendDeclarationUseCase::new(repo);
        let err = usecase
            .execute(amend_cmd(DeclarationId::new(), PRINCIPAL, default_amendments()))
            .await
            .unwrap_err();
        assert!(matches!(err, AmendError::NotFound(_)));
    }

    #[tokio::test]
    async fn amend_propagates_domain_error_on_invalid_state() {
        // Drive the aggregate to Accepted then attempt to amend.
        let repo = Arc::new(InMemoryRepo::default());
        let (id, _entity) = seed_submitted(&repo).await;
        use crate::application::RecordVerificationOutcomeUseCase;
        let verify_uc = RecordVerificationOutcomeUseCase::new(repo.clone());
        verify_uc
            .execute(crate::domain::RecordVerificationOutcome {
                declaration_id: id,
                verification_case_id: Uuid::now_v7(),
                lane: crate::domain::VerificationLane::Green,
                fused_authenticity_belief: 0.95,
                fused_authenticity_plausibility: 0.98,
                fused_risk_belief: 0.02,
                completed_at: OffsetDateTime::now_utc(),
            })
            .await
            .unwrap();
        let usecase = AmendDeclarationUseCase::new(repo);
        let err = usecase
            .execute(amend_cmd(id, PRINCIPAL, default_amendments()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AmendError::Domain(DomainError::AmendFromInvalidState { .. })
        ));
    }

    #[tokio::test]
    async fn amend_by_non_owner_refused() {
        let repo = Arc::new(InMemoryRepo::default());
        let (id, _) = seed_submitted(&repo).await;
        let usecase = AmendDeclarationUseCase::new(repo);
        let err = usecase
            .execute(amend_cmd(id, "spiffe://recor.cm/imposter", default_amendments()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AmendError::Domain(DomainError::AmendNotOwner { .. })
        ));
    }
}
