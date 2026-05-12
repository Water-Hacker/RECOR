//! Merge-persons use case.
//!
//! Admin-only operation: collapses a duplicate person aggregate into a
//! surviving canonical record. The "from" aggregate transitions to a
//! merged-out state; the "into" aggregate is unchanged.
//!
//! Implementation order:
//!   1. Load both aggregates' event streams.
//!   2. Check target liveness (target must not itself be a merged-out
//!      shell — the aggregate's `handle_merge` flags this via the
//!      `target_already_merged` parameter).
//!   3. Produce the `Merged` event on the source aggregate.
//!   4. Persist the event atomically with the projection update and
//!      outbox row.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{Instrument, info_span};

use crate::application::port::{PersonRepository, RepositoryError};
use crate::domain::{
    DomainError, MergePersons, PersonAggregate, PersonEvent, PersonId,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeReceipt {
    pub from_person_id: PersonId,
    pub into_person_id: PersonId,
    pub merged_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum MergeError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("source person {0} not found")]
    SourceNotFound(PersonId),
    #[error("target person {0} not found")]
    TargetNotFound(PersonId),
}

pub struct MergePersonsUseCase {
    repository: Arc<dyn PersonRepository>,
}

impl MergePersonsUseCase {
    pub fn new(repository: Arc<dyn PersonRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            from_person_id = %command.from_person_id,
            into_person_id = %command.into_person_id,
            actor_principal = %command.actor_principal,
            correlation_id = %command.correlation_id,
        )
    )]
    pub async fn execute(
        &self,
        command: MergePersons,
    ) -> Result<MergeReceipt, MergeError> {
        let from = command.from_person_id;
        let into = command.into_person_id;

        let source_events = self
            .repository
            .load_events(from)
            .instrument(info_span!("load_source"))
            .await?;
        if source_events.is_empty() {
            return Err(MergeError::SourceNotFound(from));
        }
        let source_agg = PersonAggregate::from_events(from, &source_events);

        let target_events = self
            .repository
            .load_events(into)
            .instrument(info_span!("load_target"))
            .await?;
        if target_events.is_empty() {
            return Err(MergeError::TargetNotFound(into));
        }
        let target_agg = PersonAggregate::from_events(into, &target_events);
        let target_already_merged = target_agg.merged_into.is_some();

        let event = source_agg.handle_merge(command.clone(), target_already_merged)?;
        self.repository
            .save_merge(&event, source_agg.version)
            .instrument(info_span!("save_merge"))
            .await?;

        let PersonEvent::Merged(payload) = &event else {
            return Err(MergeError::Domain(DomainError::EmptyActorPrincipal));
        };
        Ok(MergeReceipt {
            from_person_id: payload.person_id,
            into_person_id: payload.into_person_id,
            merged_at: payload.merged_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uuid::Uuid;

    use crate::application::register_person::tests::{make_cmd, InMemoryRepo};
    use crate::application::RegisterPersonUseCase;

    use super::*;

    #[tokio::test]
    async fn merge_happy_path() {
        let repo = Arc::new(InMemoryRepo::default());
        let register = RegisterPersonUseCase::new(repo.clone());
        let merge = MergePersonsUseCase::new(repo.clone());

        let from = PersonId::new();
        let into = PersonId::new();
        register.execute(make_cmd(from)).await.unwrap();
        register.execute(make_cmd(into)).await.unwrap();

        let cmd = MergePersons {
            from_person_id: from,
            into_person_id: into,
            actor_principal: "spiffe://recor.cm/admin".into(),
            merged_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        };
        let receipt = merge.execute(cmd).await.expect("merge");
        assert_eq!(receipt.from_person_id, from);
        assert_eq!(receipt.into_person_id, into);
    }

    #[tokio::test]
    async fn merge_source_not_found() {
        let repo = Arc::new(InMemoryRepo::default());
        let usecase = MergePersonsUseCase::new(repo);
        let cmd = MergePersons {
            from_person_id: PersonId::new(),
            into_person_id: PersonId::new(),
            actor_principal: "x".into(),
            merged_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        };
        let err = usecase.execute(cmd).await.unwrap_err();
        assert!(matches!(err, MergeError::SourceNotFound(_)));
    }
}
