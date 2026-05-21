//! Get-arrangement use case (TODO-002-domain). Reads the projection
//! for a given arrangement id.

use std::sync::Arc;

use thiserror::Error;

use crate::application::arrangement_port::{
    ArrangementProjection, ArrangementRepository, ArrangementRepositoryError,
};
use crate::domain::ArrangementId;

#[derive(Debug, Error)]
pub enum GetArrangementError {
    #[error("arrangement {0} not found")]
    NotFound(ArrangementId),
    #[error(transparent)]
    Repository(#[from] ArrangementRepositoryError),
}

pub struct GetArrangementUseCase {
    repository: Arc<dyn ArrangementRepository>,
}

impl GetArrangementUseCase {
    pub fn new(repository: Arc<dyn ArrangementRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), fields(arrangement_id = %id))]
    pub async fn execute(
        &self,
        id: ArrangementId,
    ) -> Result<ArrangementProjection, GetArrangementError> {
        match self.repository.load_projection(id).await? {
            Some(p) => Ok(p),
            None => Err(GetArrangementError::NotFound(id)),
        }
    }
}
