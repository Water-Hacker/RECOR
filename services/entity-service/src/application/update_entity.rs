//! Update-entity use case. In-place update of mutable fields
//! (canonical_name, entity_type). Identity tuple is not affected.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::port::{EntityRepository, RepositoryError};
use crate::domain::{
    DomainError, EntityAggregate, EntityEvent, EntityId, UpdateEntity,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateReceipt {
    pub entity_id: EntityId,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("entity {0} not found")]
    NotFound(EntityId),
}

pub struct UpdateEntityUseCase {
    repository: Arc<dyn EntityRepository>,
}

impl UpdateEntityUseCase {
    pub fn new(repository: Arc<dyn EntityRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(entity_id = %command.entity_id, correlation_id = %command.correlation_id)
    )]
    pub async fn execute(&self, command: UpdateEntity) -> Result<UpdateReceipt, UpdateError> {
        let id = command.entity_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(UpdateError::NotFound(id));
        }
        let aggregate = EntityAggregate::from_events(id, &events);
        let event = aggregate.handle_update(command)?;

        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let EntityEvent::Updated(p) = &event else {
            return Err(UpdateError::Domain(DomainError::UpdateBeforeRegistration(id.0)));
        };
        Ok(UpdateReceipt {
            entity_id: p.entity_id,
            updated_at: p.updated_at,
        })
    }
}
