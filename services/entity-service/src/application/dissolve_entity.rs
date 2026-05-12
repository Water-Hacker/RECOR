//! Dissolve-entity use case. Records the entity's dissolution date and
//! transitions the aggregate to terminal `Dissolved` state.
//!
//! D17 zero-trust: administrative endpoint. The API layer enforces the
//! admin-principal allowlist before this use case is called; the use
//! case itself trusts that the caller has authority.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::port::{EntityRepository, RepositoryError};
use crate::domain::{
    DissolveEntity, DomainError, EntityAggregate, EntityEvent, EntityId,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DissolveReceipt {
    pub entity_id: EntityId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolved_at: time::Date,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum DissolveError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("entity {0} not found")]
    NotFound(EntityId),
}

pub struct DissolveEntityUseCase {
    repository: Arc<dyn EntityRepository>,
}

impl DissolveEntityUseCase {
    pub fn new(repository: Arc<dyn EntityRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(entity_id = %command.entity_id, correlation_id = %command.correlation_id)
    )]
    pub async fn execute(&self, command: DissolveEntity) -> Result<DissolveReceipt, DissolveError> {
        let id = command.entity_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(DissolveError::NotFound(id));
        }
        let aggregate = EntityAggregate::from_events(id, &events);
        let event = aggregate.handle_dissolve(command)?;
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;

        let EntityEvent::Dissolved(p) = &event else {
            return Err(DissolveError::Domain(DomainError::DissolveBeforeRegistration(id.0)));
        };
        Ok(DissolveReceipt {
            entity_id: p.entity_id,
            dissolved_at: p.dissolved_at,
            recorded_at: p.recorded_at,
        })
    }
}
