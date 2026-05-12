//! Submit-verification use case.

use std::sync::Arc;

use thiserror::Error;
use tracing::{info, instrument};

use crate::application::orchestrator::PipelineOrchestrator;
use crate::application::port::{RepositoryError, VerificationRepository};
use crate::domain::{DeclarationSnapshot, VerificationCase};

pub struct SubmitVerificationUseCase {
    orchestrator: Arc<PipelineOrchestrator>,
    repository: Arc<dyn VerificationRepository>,
}

#[derive(Debug, Error)]
pub enum SubmitError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl SubmitVerificationUseCase {
    pub fn new(
        orchestrator: Arc<PipelineOrchestrator>,
        repository: Arc<dyn VerificationRepository>,
    ) -> Self {
        Self { orchestrator, repository }
    }

    #[instrument(skip_all, fields(declaration_id = %declaration.declaration_id))]
    pub async fn execute(
        &self,
        declaration: DeclarationSnapshot,
    ) -> Result<VerificationCase, SubmitError> {
        // Idempotent: if this declaration has already been verified,
        // return the existing case rather than re-running.
        if let Some(existing_id) = self
            .repository
            .case_for_declaration(declaration.declaration_id)
            .await?
        {
            info!(case_id = %existing_id, "replaying existing verification case");
            if let Some(case) = self.repository.load_case(existing_id).await? {
                return Ok(case);
            }
        }

        let case = self.orchestrator.run(declaration).await;
        self.repository.save_case(&case).await?;
        Ok(case)
    }
}
