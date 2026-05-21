//! Dissolve-arrangement use case (TODO-002-domain). Records the
//! arrangement's dissolution date, transitions the aggregate to the
//! terminal Dissolved state, and computes the R.25 INR §3.f
//! 5-year-after-cessation retention deadline.
//!
//! D17 zero-trust: administrative endpoint. The API layer enforces the
//! admin-principal allowlist before this use case is called; the use
//! case itself trusts that the caller has authority.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info_span, Instrument};

use crate::application::arrangement_port::{ArrangementRepository, ArrangementRepositoryError};
use crate::domain::{
    ArrangementAggregate, ArrangementDomainError, ArrangementEvent, ArrangementId,
    DissolveArrangement,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DissolveArrangementReceipt {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub dissolution_date: time::Date,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub retention_until: time::Date,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum DissolveArrangementError {
    #[error(transparent)]
    Domain(#[from] ArrangementDomainError),
    #[error(transparent)]
    Repository(#[from] ArrangementRepositoryError),
    #[error("arrangement {0} not found")]
    NotFound(ArrangementId),
}

pub struct DissolveArrangementUseCase {
    repository: Arc<dyn ArrangementRepository>,
}

impl DissolveArrangementUseCase {
    pub fn new(repository: Arc<dyn ArrangementRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        skip_all,
        fields(arrangement_id = %command.arrangement_id, correlation_id = %command.correlation_id)
    )]
    pub async fn execute(
        &self,
        command: DissolveArrangement,
    ) -> Result<DissolveArrangementReceipt, DissolveArrangementError> {
        let id = command.arrangement_id;
        let events = self
            .repository
            .load_events(id)
            .instrument(info_span!("load_events"))
            .await?;
        if events.is_empty() {
            return Err(DissolveArrangementError::NotFound(id));
        }
        let aggregate = ArrangementAggregate::from_events(id, &events);
        let event = aggregate.handle_dissolve(command)?;
        self.repository
            .save_event(&event, aggregate.version)
            .instrument(info_span!("save_event"))
            .await?;
        let ArrangementEvent::Dissolved(p) = &event else {
            return Err(DissolveArrangementError::Domain(
                ArrangementDomainError::DissolveBeforeRegistration(id.0),
            ));
        };
        Ok(DissolveArrangementReceipt {
            arrangement_id: p.arrangement_id,
            dissolution_date: p.dissolution_date,
            retention_until: p.retention_until,
            recorded_at: p.recorded_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};
    use uuid::Uuid;

    use super::super::register_arrangement::tests::{make_cmd, InMemoryArrangementRepo};
    use super::super::register_arrangement::RegisterArrangementUseCase;

    use super::*;

    #[tokio::test]
    async fn dissolve_on_missing_returns_not_found() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let uc = DissolveArrangementUseCase::new(repo.clone());
        let id = ArrangementId::new();
        let cmd = DissolveArrangement {
            arrangement_id: id,
            dissolution_date: date!(2026 - 04 - 01),
            dissolved_by_principal: "spiffe://recor.cm/admin-1".into(),
            recorded_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        };
        let err = uc.execute(cmd).await.unwrap_err();
        assert!(matches!(err, DissolveArrangementError::NotFound(_)));
    }

    #[tokio::test]
    async fn happy_path_returns_retention_until() {
        let repo = Arc::new(InMemoryArrangementRepo::default());
        let id = ArrangementId::new();
        let reg = RegisterArrangementUseCase::new(repo.clone());
        reg.execute(make_cmd(id)).await.unwrap();

        let uc = DissolveArrangementUseCase::new(repo.clone());
        let dissolution_date = date!(2026 - 04 - 01);
        let cmd = DissolveArrangement {
            arrangement_id: id,
            dissolution_date,
            dissolved_by_principal: "spiffe://recor.cm/admin-1".into(),
            recorded_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        };
        let receipt = uc.execute(cmd).await.expect("dissolve");
        assert_eq!(receipt.dissolution_date, dissolution_date);
        let delta = (receipt.retention_until - receipt.dissolution_date).whole_days();
        assert!((1825..=1830).contains(&delta), "retention 5 years; got {delta} days");
    }
}
