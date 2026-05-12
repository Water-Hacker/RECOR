//! Search-entities use case. Returns entities matching the search
//! criteria (free-text + jurisdiction + entity_type filters).

use std::sync::Arc;

use thiserror::Error;

use crate::application::port::{EntityRepository, RepositoryError, SearchCriteria};
use crate::application::EntityProjection;

/// Maximum page size. Higher requested values are silently capped to
/// this; lower values pass through. v1 keeps this conservative to bound
/// the worst-case scan cost.
pub const MAX_SEARCH_LIMIT: u32 = 200;

/// Default page size if the caller did not specify one.
pub const DEFAULT_SEARCH_LIMIT: u32 = 50;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct SearchEntitiesUseCase {
    repository: Arc<dyn EntityRepository>,
}

impl SearchEntitiesUseCase {
    pub fn new(repository: Arc<dyn EntityRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self, criteria))]
    pub async fn execute(
        &self,
        mut criteria: SearchCriteria,
    ) -> Result<Vec<EntityProjection>, SearchError> {
        if criteria.limit == 0 {
            criteria.limit = DEFAULT_SEARCH_LIMIT;
        }
        if criteria.limit > MAX_SEARCH_LIMIT {
            criteria.limit = MAX_SEARCH_LIMIT;
        }
        // Normalise the search inputs at the use-case boundary; the
        // repository can then assume trimmed/canonical values.
        criteria.q = criteria.q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        criteria.jurisdiction = criteria
            .jurisdiction
            .map(|s| s.trim().to_ascii_uppercase())
            .filter(|s| !s.is_empty());
        criteria.entity_type = criteria
            .entity_type
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());

        let rows = self.repository.find_by_criteria(&criteria).await?;
        Ok(rows)
    }
}
