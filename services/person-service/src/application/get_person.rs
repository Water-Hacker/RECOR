//! Get-person use case. Reads the projection (current-state) for a
//! given person id.
//!
//! When the person has been merged into a surviving canonical record,
//! the projection still resolves but carries `merged_into = Some(...)`
//! so the caller can follow the pointer (the API handler does this
//! transparently in v1 if `?follow=true`; default behaviour returns
//! the row as-is).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::application::port::{PersonRepository, RepositoryError};
use crate::domain::{PersonAttributes, PersonId};

/// Projection shape returned to API consumers.
///
/// Carries the full PII payload — the consumer is the API layer, which
/// applies field-level redaction at the wire boundary if and when the
/// caller is not authorised to see the Sensitive-PII fields. v1's
/// authorisation rule is operator-only (admin allowlist); a future
/// ticket layers per-field ABAC on top.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersonProjection {
    pub person_id: PersonId,
    pub attributes: PersonAttributes,
    pub aggregate_version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub updated_at: OffsetDateTime,
    /// If this person has been merged into a canonical record, the
    /// target id. Reads following the pointer should land on the
    /// target's projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<PersonId>,
}

#[derive(Debug, Error)]
pub enum GetError {
    #[error("person {0} not found")]
    NotFound(PersonId),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct GetPersonUseCase {
    repository: Arc<dyn PersonRepository>,
}

impl GetPersonUseCase {
    pub fn new(repository: Arc<dyn PersonRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), fields(person_id = %id))]
    pub async fn execute(&self, id: PersonId) -> Result<PersonProjection, GetError> {
        match self.repository.load_projection(id).await? {
            Some(p) => Ok(p),
            None => Err(GetError::NotFound(id)),
        }
    }
}
