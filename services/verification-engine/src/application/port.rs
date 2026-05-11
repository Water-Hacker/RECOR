//! Application-layer ports.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{VerificationCase, VerificationCaseId};

#[async_trait]
pub trait VerificationRepository: Send + Sync {
    /// Persist a verification case atomically with an outbox row.
    async fn save_case(&self, case: &VerificationCase) -> Result<(), RepositoryError>;

    /// Load a previously-persisted case by id.
    async fn load_case(
        &self,
        id: VerificationCaseId,
    ) -> Result<Option<VerificationCase>, RepositoryError>;

    /// Idempotent guard: has this declaration_id already been verified?
    /// Returns the existing case id, if any.
    async fn case_for_declaration(
        &self,
        declaration_id: Uuid,
    ) -> Result<Option<VerificationCaseId>, RepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),
    #[error("serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),
}

/// BUNEC (Bureau National de l'État Civil) identity adapter.
///
/// In production, this resolves declared `person_id` → canonical
/// identity record at the national identity registry. In dev / test,
/// the `MockBunecAdapter` resolves against an in-memory or
/// Postgres-seeded record set.
///
/// Real BUNEC integration is a follow-up ticket (R-VER-1).
#[async_trait]
pub trait BunecAdapter: Send + Sync {
    async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BunecLookup {
    /// Person record exists at BUNEC and matches expected attributes.
    /// In v1 the only attribute we validate is existence; future
    /// versions check date of birth, residential address, etc.
    Found {
        person_id: Uuid,
        canonical_full_name: String,
        nationality: String,
    },
    /// No record for that person_id at BUNEC. In production this is a
    /// strong negative signal; the declarant may have invented a
    /// person_id.
    NotFound { person_id: Uuid },
}

#[derive(Debug, thiserror::Error)]
pub enum BunecLookupError {
    #[error("BUNEC backend failure: {0}")]
    Backend(String),
}
