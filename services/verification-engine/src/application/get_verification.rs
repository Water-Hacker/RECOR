//! Get-verification use case.

use std::sync::Arc;

use thiserror::Error;

use crate::application::port::{RepositoryError, VerificationRepository};
use crate::domain::{DecisionRationale, VerificationCase, VerificationCaseId};

pub struct GetVerificationUseCase {
    repository: Arc<dyn VerificationRepository>,
}

#[derive(Debug, Error)]
pub enum GetError {
    #[error("verification case {0} not found")]
    NotFound(VerificationCaseId),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl GetVerificationUseCase {
    pub fn new(repository: Arc<dyn VerificationRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        case_id: VerificationCaseId,
    ) -> Result<VerificationCase, GetError> {
        self.repository
            .load_case(case_id)
            .await?
            .ok_or(GetError::NotFound(case_id))
    }

    /// TODO-049 — fetch the per-decision rationale persisted alongside
    /// the case. Returns `NotFound` when either the case itself does
    /// not exist OR the case predates the rationale migration.
    pub async fn execute_rationale(
        &self,
        case_id: VerificationCaseId,
    ) -> Result<DecisionRationale, GetError> {
        self.repository
            .load_rationale(case_id)
            .await?
            .ok_or(GetError::NotFound(case_id))
    }
}
