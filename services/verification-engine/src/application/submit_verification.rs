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

    #[instrument(
        skip_all,
        fields(
            declaration_id = %declaration.declaration_id,
            correlation_id = %declaration.correlation_id,
        ),
    )]
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

        // TODO-061: writeback the declaration_projection BEFORE the
        // orchestrator runs so Stage 6 sees the current submission +
        // any prior submissions against the same entity / declarant.
        // The projection is a derived view of the declaration service's
        // events shipped over Kafka; this method is the consumer side
        // that closes the loop the orchestrator depends on. Errors are
        // surfaced (fail-closed): a writeback failure means Stage 6
        // would run against stale data, which is worse than refusing.
        let owners_json = serde_json::to_value(&declaration.beneficial_owners)
            .map_err(|e| SubmitError::Repository(RepositoryError::Serialisation(e)))?;
        self.repository
            .upsert_declaration_projection(
                declaration.declaration_id,
                declaration.entity_id,
                &declaration.declarant_principal,
                declaration.submitted_at,
                declaration.effective_from,
                owners_json,
                None, // entity_jurisdiction resolved by a later ticket
            )
            .await?;

        // TODO-049 — the orchestrator now returns both the case and the
        // rationale composed at adjudication time. The repository
        // persists them in the same transaction so a case never exists
        // without its explanatory record (D14 fail-closed against
        // half-written audit state).
        let outcome = self.orchestrator.run(declaration).await;
        self.repository
            .save_case(&outcome.case, Some(&outcome.rationale))
            .await?;
        Ok(outcome.case)
    }
}
