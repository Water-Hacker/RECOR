//! Update-arrangement use case (TODO-002-domain). In-place edit of the
//! R.25 identifier-role columns.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::arrangement_port::{ArrangementRepository, ArrangementRepositoryError};
use crate::domain::{
    ArrangementAggregate, ArrangementDomainError, ArrangementEvent, ArrangementId,
    UpdateArrangement,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateArrangementReceipt {
    pub arrangement_id: ArrangementId,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum UpdateArrangementError {
    #[error(transparent)]
    Domain(#[from] ArrangementDomainError),
    #[error(transparent)]
    Repository(#[from] ArrangementRepositoryError),
    #[error("arrangement {0} not found")]
    NotFound(ArrangementId),
}

pub struct UpdateArrangementUseCase {
    repository: Arc<dyn ArrangementRepository>,
}

impl UpdateArrangementUseCase {
    pub fn new(repository: Arc<dyn ArrangementRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(arrangement_id = %command.arrangement_id, correlation_id = %command.correlation_id)
    )]
    pub async fn execute(
        &self,
        command: UpdateArrangement,
    ) -> Result<UpdateArrangementReceipt, UpdateArrangementError> {
        let id = command.arrangement_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(UpdateArrangementError::NotFound(id));
        }
        let aggregate = ArrangementAggregate::from_events(id, &events);
        let event = aggregate.handle_update(command)?;
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;
        let ArrangementEvent::Updated(p) = &event else {
            return Err(UpdateArrangementError::Domain(
                ArrangementDomainError::UpdateBeforeRegistration(id.0),
            ));
        };
        Ok(UpdateArrangementReceipt {
            arrangement_id: p.arrangement_id,
            updated_at: p.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::super::register_arrangement::tests::{make_cmd, InMemoryArrangementRepo};
    use super::super::register_arrangement::RegisterArrangementUseCase;
    use crate::domain::{ArrangementUpdatableFields, ControlExerciseRef, SettlorRef, TrusteeRef};
    use time::macros::datetime;

    use super::*;

    fn after_with_extra_control() -> ArrangementUpdatableFields {
        ArrangementUpdatableFields {
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
            control_exercise_refs: vec![ControlExerciseRef {
                person_id: Uuid::now_v7(),
                control_basis: "Settlor-puppet trustee identified by tax filings".into(),
            }],
        }
    }

    #[tokio::test]
    async fn update_on_missing_arrangement_returns_not_found() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let uc = UpdateArrangementUseCase::new(repo.clone());
        let id = ArrangementId::new();
        let cmd = UpdateArrangement {
            arrangement_id: id,
            after: after_with_extra_control(),
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        };
        let err = uc.execute(cmd).await.unwrap_err();
        assert!(matches!(err, UpdateArrangementError::NotFound(_)));
    }

    #[tokio::test]
    async fn happy_path_updates_and_returns_receipt() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let id = ArrangementId::new();
        let reg = RegisterArrangementUseCase::new(repo.clone());
        reg.execute(make_cmd(id)).await.unwrap();

        let uc = UpdateArrangementUseCase::new(repo.clone());
        let cmd = UpdateArrangement {
            arrangement_id: id,
            after: after_with_extra_control(),
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: datetime!(2026-05-02 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        };
        let receipt = uc.execute(cmd).await.expect("update");
        assert_eq!(receipt.arrangement_id, id);
        assert_eq!(repo.events.lock().unwrap().get(&id.0).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn update_must_preserve_settlor_invariant() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let id = ArrangementId::new();
        let reg = RegisterArrangementUseCase::new(repo.clone());
        reg.execute(make_cmd(id)).await.unwrap();

        let uc = UpdateArrangementUseCase::new(repo.clone());
        let mut after = after_with_extra_control();
        after.settlor_refs.clear();
        let cmd = UpdateArrangement {
            arrangement_id: id,
            after,
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: datetime!(2026-05-02 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        };
        let err = uc.execute(cmd).await.unwrap_err();
        assert!(matches!(
            err,
            UpdateArrangementError::Domain(ArrangementDomainError::NoSettlor)
        ));
    }
}
