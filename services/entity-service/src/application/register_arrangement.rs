//! Register-arrangement use case (TODO-002-domain).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::arrangement_port::{ArrangementRepository, ArrangementRepositoryError};
use crate::domain::{
    ArrangementAggregate, ArrangementDomainError, ArrangementEvent, ArrangementId,
    RegisterArrangement,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterArrangementReceipt {
    pub arrangement_id: ArrangementId,
    pub registered_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum RegisterArrangementError {
    #[error(transparent)]
    Domain(#[from] ArrangementDomainError),
    #[error(transparent)]
    Repository(#[from] ArrangementRepositoryError),
}

pub struct RegisterArrangementUseCase {
    repository: Arc<dyn ArrangementRepository>,
}

impl RegisterArrangementUseCase {
    pub fn new(repository: Arc<dyn ArrangementRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            arrangement_id = %command.arrangement_id,
            arrangement_kind = ?command.arrangement_kind,
            jurisdiction = %command.governing_law_jurisdiction,
            registered_by = %command.registered_by_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: RegisterArrangement,
    ) -> Result<RegisterArrangementReceipt, RegisterArrangementError> {
        let id = command.arrangement_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        let aggregate = ArrangementAggregate::from_events(id, &events);

        let now = OffsetDateTime::now_utc();
        let event = aggregate.handle_register(command, now)?;

        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let ArrangementEvent::Registered(p) = &event else {
            return Err(RegisterArrangementError::Domain(
                ArrangementDomainError::AlreadyRegistered(id.0),
            ));
        };
        Ok(RegisterArrangementReceipt {
            arrangement_id: p.arrangement_id,
            registered_at: p.registered_at,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use time::macros::{date, datetime};
    use uuid::Uuid;

    use crate::application::arrangement_port::{
        ArrangementProjection, ArrangementRepository, ArrangementRepositoryError,
    };
    use crate::domain::{
        ArrangementKind, ArrangementUpdatableFields, GoverningLawJurisdiction, SettlorRef,
        TrusteeRef,
    };

    use super::*;

    #[derive(Default)]
    pub(crate) struct InMemoryArrangementRepo {
        pub(crate) events: Mutex<HashMap<Uuid, Vec<ArrangementEvent>>>,
    }

    #[async_trait]
    impl ArrangementRepository for InMemoryArrangementRepo {
        async fn load_events(
            &self,
            id: ArrangementId,
        ) -> Result<Vec<ArrangementEvent>, ArrangementRepositoryError> {
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
            event: &ArrangementEvent,
            expected_version: u64,
        ) -> Result<(), ArrangementRepositoryError> {
            let id = event.arrangement_id();
            let mut guard = self.events.lock().unwrap();
            let stream = guard.entry(id.0).or_default();
            let current = stream.len() as u64;
            if current != expected_version {
                return Err(ArrangementRepositoryError::Conflict {
                    expected: expected_version,
                    found: current,
                });
            }
            stream.push(event.clone());
            Ok(())
        }

        async fn load_projection(
            &self,
            _id: ArrangementId,
        ) -> Result<Option<ArrangementProjection>, ArrangementRepositoryError> {
            Ok(None)
        }
    }

    pub(crate) fn make_cmd(id: ArrangementId) -> RegisterArrangement {
        RegisterArrangement {
            arrangement_id: id,
            arrangement_kind: ArrangementKind::ExpressTrust,
            governing_law_jurisdiction: GoverningLawJurisdiction::try_from_str("CM").unwrap(),
            constitution_date: date!(2024 - 06 - 01),
            fields: ArrangementUpdatableFields {
                settlor_refs: vec![SettlorRef {
                    person_id: Uuid::now_v7(),
                    role_metadata: None,
                }],
                trustee_refs: vec![TrusteeRef {
                    person_id: Some(Uuid::now_v7()),
                    entity_id: None,
                    fiduciary_registration_id: None,
                    role_metadata: None,
                }],
                protector_refs: vec![],
                named_beneficiary_refs: vec![],
                class_beneficiary_specs: vec![],
                control_exercise_refs: vec![],
            },
            registered_by_principal: "spiffe://recor.cm/admin-1".into(),
            registered_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[tokio::test]
    async fn happy_path_registers_and_returns_receipt() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let uc = RegisterArrangementUseCase::new(repo.clone());
        let id = ArrangementId::new();
        let receipt = uc.execute(make_cmd(id)).await.expect("register");
        assert_eq!(receipt.arrangement_id, id);
        assert_eq!(repo.events.lock().unwrap().get(&id.0).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_register_rejects_with_domain_error() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let uc = RegisterArrangementUseCase::new(repo.clone());
        let id = ArrangementId::new();
        uc.execute(make_cmd(id)).await.unwrap();
        let err = uc.execute(make_cmd(id)).await.unwrap_err();
        assert!(matches!(
            err,
            RegisterArrangementError::Domain(ArrangementDomainError::AlreadyRegistered(_))
        ));
    }

    #[tokio::test]
    async fn missing_settlor_rejected() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let uc = RegisterArrangementUseCase::new(repo.clone());
        let id = ArrangementId::new();
        let mut cmd = make_cmd(id);
        cmd.fields.settlor_refs.clear();
        let err = uc.execute(cmd).await.unwrap_err();
        assert!(matches!(
            err,
            RegisterArrangementError::Domain(ArrangementDomainError::NoSettlor)
        ));
    }
}
