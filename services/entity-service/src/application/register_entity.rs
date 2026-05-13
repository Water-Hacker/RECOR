//! Register-entity use case.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::port::{EntityRepository, RepositoryError};
use crate::domain::{
    DomainError, EntityAggregate, EntityEvent, EntityId, EntityRegisteredV1, RegisterEntity,
};

/// Receipt returned to the API layer on successful registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterReceipt {
    pub entity_id: EntityId,
    pub registered_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

/// Use case object — a thin orchestrator over the repository port.
pub struct RegisterEntityUseCase {
    repository: Arc<dyn EntityRepository>,
}

impl RegisterEntityUseCase {
    pub fn new(repository: Arc<dyn EntityRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            entity_id = %command.entity_id,
            jurisdiction = %command.jurisdiction,
            registered_by = %command.registered_by_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: RegisterEntity,
    ) -> Result<RegisterReceipt, RegisterError> {
        let id = command.entity_id;
        // Hydrate aggregate from existing events (zero or more).
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        let aggregate = EntityAggregate::from_events(id, &events);

        // Validate + produce event (against UTC clock).
        let now = OffsetDateTime::now_utc();
        let event = aggregate.handle_register(command, now)?;

        // Persist event + projection + outbox row (atomic).
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let EntityEvent::Registered(payload) = &event else {
            // Defensive: handle_register only produces Registered.
            return Err(RegisterError::Domain(DomainError::AlreadyRegistered(id.0)));
        };
        Ok(RegisterReceipt {
            entity_id: payload.entity_id,
            registered_at: payload.registered_at,
        })
    }
}

/// Helper for the API layer to derive a receipt from a stored event
/// (used when an idempotency replay returns the same answer as the
/// original).
#[must_use]
pub fn receipt_from_event(event: &EntityRegisteredV1) -> RegisterReceipt {
    RegisterReceipt {
        entity_id: event.entity_id,
        registered_at: event.registered_at,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use time::macros::{date, datetime};
    use uuid::Uuid;

    use crate::application::{EntityProjection, SearchCriteria};
    use crate::domain::{CanonicalName, EntityType, Jurisdiction, RegistrationNumber};

    use super::*;

    #[derive(Default)]
    struct InMemoryRepo {
        events: Mutex<HashMap<Uuid, Vec<EntityEvent>>>,
    }

    #[async_trait]
    impl EntityRepository for InMemoryRepo {
        async fn load_events(
            &self,
            id: EntityId,
        ) -> Result<Vec<EntityEvent>, RepositoryError> {
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
            event: &EntityEvent,
            expected_version: u64,
        ) -> Result<(), RepositoryError> {
            let id = event.entity_id();
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
            _id: EntityId,
        ) -> Result<Option<EntityProjection>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_criteria(
            &self,
            _c: &SearchCriteria,
        ) -> Result<Vec<EntityProjection>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    fn make_cmd(id: EntityId) -> RegisterEntity {
        RegisterEntity {
            entity_id: id,
            canonical_name: CanonicalName::try_from_str("ACME").unwrap(),
            entity_type: EntityType::Sarl,
            jurisdiction: Jurisdiction::try_from_str("CM").unwrap(),
            registration_number_in_jurisdiction: RegistrationNumber::try_from_str("RC/1").unwrap(),
            founded_at: date!(2020 - 01 - 01),
            registered_by_principal: "spiffe://recor.cm/admin-1".into(),
            registered_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[tokio::test]
    async fn happy_path_registers_and_returns_receipt() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = RegisterEntityUseCase::new(repo.clone());
        let id = EntityId::new();
        let cmd = make_cmd(id);
        let receipt = usecase.execute(cmd.clone()).await.expect("register");
        assert_eq!(receipt.entity_id, id);
        let stored = repo.events.lock().unwrap();
        assert_eq!(stored.get(&id.0).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_register_rejects_with_domain_error() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = RegisterEntityUseCase::new(repo.clone());
        let id = EntityId::new();
        usecase.execute(make_cmd(id)).await.unwrap();
        let err = usecase.execute(make_cmd(id)).await.unwrap_err();
        assert!(matches!(err, RegisterError::Domain(DomainError::AlreadyRegistered(_))));
    }
}
