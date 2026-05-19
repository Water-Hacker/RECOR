//! Register-person use case.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{Instrument, info_span};

use crate::application::port::{PersonRepository, RepositoryError};
use crate::domain::{
    DomainError, PersonAggregate, PersonEvent, PersonId, PersonRegisteredV1, RegisterPerson,
};

/// Receipt returned to the API layer on successful registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterReceipt {
    pub person_id: PersonId,
    pub registered_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct RegisterPersonUseCase {
    repository: Arc<dyn PersonRepository>,
}

impl RegisterPersonUseCase {
    pub fn new(repository: Arc<dyn PersonRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            person_id = %command.person_id,
            actor_principal = %command.actor_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: RegisterPerson,
    ) -> Result<RegisterReceipt, RegisterError> {
        let id = command.person_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        let aggregate = PersonAggregate::from_events(id, &events);
        let event = aggregate.handle_register(command)?;

        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let PersonEvent::Registered(payload) = &event else {
            return Err(RegisterError::Domain(DomainError::EmptyActorPrincipal));
        };
        Ok(RegisterReceipt {
            person_id: payload.person_id,
            registered_at: payload.registered_at,
        })
    }
}

/// Helper for the API layer to derive a receipt from a stored event
/// (used when an idempotency replay returns the same answer as the
/// original).
#[must_use]
pub fn receipt_from_event(event: &PersonRegisteredV1) -> RegisterReceipt {
    RegisterReceipt {
        person_id: event.person_id,
        registered_at: event.registered_at,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use time::macros::date;
    use uuid::Uuid;

    use crate::application::PersonProjection;
    use crate::domain::value_object::{
        CanonicalFullName, IdDocument, IdDocumentType, Nationality, PersonAttributes,
    };

    use super::*;

    /// In-memory repository double; deterministic for unit testing.
    #[derive(Default)]
    pub(crate) struct InMemoryRepo {
        pub events: Mutex<HashMap<Uuid, Vec<PersonEvent>>>,
    }

    #[async_trait]
    impl PersonRepository for InMemoryRepo {
        async fn load_events(
            &self,
            id: PersonId,
        ) -> Result<Vec<PersonEvent>, RepositoryError> {
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
            event: &PersonEvent,
            expected_version: u64,
        ) -> Result<(), RepositoryError> {
            let id = event.person_id();
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

        async fn save_merge(
            &self,
            event: &PersonEvent,
            expected_version: u64,
        ) -> Result<(), RepositoryError> {
            // Same shape as save_event for the in-memory double.
            self.save_event(event, expected_version).await
        }

        async fn load_projection(
            &self,
            _id: PersonId,
        ) -> Result<Option<PersonProjection>, RepositoryError> {
            Ok(None)
        }

        async fn search(
            &self,
            _query: &str,
            _nationality_filter: Option<&str>,
            _created_by_filter: Option<&str>,
            _limit: i64,
        ) -> Result<Vec<PersonProjection>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    pub(crate) fn make_cmd(person_id: PersonId) -> RegisterPerson {
        RegisterPerson {
            person_id,
            attributes: PersonAttributes {
                canonical_full_name: CanonicalFullName::try_new("Ngono Marie").unwrap(),
                nationality: Nationality::try_new("CM").unwrap(),
                date_of_birth: Some(date!(1980 - 04 - 21)),
                primary_id_document: IdDocument {
                    issuer: "CM:DGSN".into(),
                    doc_type: IdDocumentType::NationalId,
                    number: "100123456".into(),
                    expiry: None,
                },
                biometric_reference_hash: None,
            },
            actor_principal: "spiffe://recor.cm/test".into(),
            registered_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[tokio::test]
    async fn happy_path_registers_and_returns_receipt() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = RegisterPersonUseCase::new(repo.clone());
        let cmd = make_cmd(PersonId::new());
        let receipt = usecase.execute(cmd.clone()).await.expect("register");
        assert_eq!(receipt.person_id, cmd.person_id);
        let stored = repo.events.lock().unwrap();
        assert_eq!(stored.get(&cmd.person_id.0).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_register_rejects_with_domain_error() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = RegisterPersonUseCase::new(repo);
        let cmd = make_cmd(PersonId::new());
        usecase.execute(cmd.clone()).await.unwrap();
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(
            err,
            RegisterError::Domain(DomainError::AlreadyRegistered(_))
        ));
    }
}
