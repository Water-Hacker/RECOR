//! Search-persons use case.
//!
//! v1 search is intentionally crude: case-insensitive `ILIKE` against
//! `canonical_full_name`, optional exact-match filter on `nationality`,
//! capped at 50 results.
//!
//! Future tickets (tracked by a TODO marker in the Postgres adapter)
//! upgrade this to trigram + Levenshtein via the `pg_trgm` extension
//! so soundex-style mis-spellings ("Ngono" vs "N'gono") still match.
//! The use-case shape doesn't change when the upgrade ships; the
//! repository implementation changes the WHERE clause.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use crate::application::port::{PersonRepository, RepositoryError};
use crate::application::PersonProjection;

/// User-facing query shape.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SearchQuery {
    /// Full-text-like fragment to match against `canonical_full_name`.
    /// Must be non-empty; trimmed at the API boundary. The 1..=256
    /// length bound is enforced at the API handler, not in this
    /// schema (utoipa 5 only accepts min/max on String via `pattern`,
    /// not as separate attributes).
    pub q: String,
    /// Optional ISO 3166-1 alpha-2 country code filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, example = "CM")]
    pub nationality: Option<String>,
    /// Page size, capped at 50.
    #[serde(default = "default_limit")]
    #[schema(example = 25, minimum = 1, maximum = 50)]
    pub limit: i64,
}

fn default_limit() -> i64 {
    25
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("query string must be non-empty")]
    EmptyQuery,
    #[error("query string longer than 256 characters")]
    QueryTooLong,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct SearchPersonsUseCase {
    repository: Arc<dyn PersonRepository>,
}

impl SearchPersonsUseCase {
    pub fn new(repository: Arc<dyn PersonRepository>) -> Self {
        Self { repository }
    }

    /// FIND-005 RBAC: when `created_by_filter` is `Some(principal)` the
    /// use case restricts the search to rows that principal registered.
    /// Admin callers pass `None`. The handler decides which based on
    /// the admin allowlist; this use case is agnostic to the policy.
    #[tracing::instrument(
        skip(self),
        fields(
            query_len = query.q.len(),
            nationality = ?query.nationality,
            scoped_to_caller = created_by_filter.is_some(),
        )
    )]
    pub async fn execute(
        &self,
        query: SearchQuery,
        created_by_filter: Option<&str>,
    ) -> Result<Vec<PersonProjection>, SearchError> {
        let trimmed = query.q.trim();
        if trimmed.is_empty() {
            return Err(SearchError::EmptyQuery);
        }
        if trimmed.chars().count() > 256 {
            return Err(SearchError::QueryTooLong);
        }
        // Cap limit at 50 so a malicious or buggy client cannot scrape
        // the whole projection in one call.
        let limit = query.limit.clamp(1, 50);
        let nationality_filter = query
            .nationality
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let rows = self
            .repository
            .search(trimmed, nationality_filter, created_by_filter, limit)
            .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::application::port::{PersonRepository, RepositoryError};
    use crate::application::PersonProjection;
    use crate::domain::{PersonEvent, PersonId};

    use super::*;

    #[derive(Default)]
    struct StubRepo {
        last_query: std::sync::Mutex<
            Option<(String, Option<String>, Option<String>, i64)>,
        >,
    }

    #[async_trait]
    impl PersonRepository for StubRepo {
        async fn load_events(
            &self,
            _id: PersonId,
        ) -> Result<Vec<PersonEvent>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn save_event(
            &self,
            _event: &PersonEvent,
            _expected_version: u64,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn save_merge(
            &self,
            _event: &PersonEvent,
            _expected_version: u64,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn load_projection(
            &self,
            _id: PersonId,
        ) -> Result<Option<PersonProjection>, RepositoryError> {
            Ok(None)
        }
        async fn search(
            &self,
            query: &str,
            nationality_filter: Option<&str>,
            created_by_filter: Option<&str>,
            limit: i64,
        ) -> Result<Vec<PersonProjection>, RepositoryError> {
            *self.last_query.lock().unwrap() = Some((
                query.into(),
                nationality_filter.map(str::to_string),
                created_by_filter.map(str::to_string),
                limit,
            ));
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn empty_query_rejects() {
        let repo: Arc<dyn PersonRepository> = Arc::new(StubRepo::default());
        let usecase = SearchPersonsUseCase::new(repo);
        let err = usecase
            .execute(
                SearchQuery {
                    q: "   ".into(),
                    nationality: None,
                    limit: 10,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SearchError::EmptyQuery));
    }

    #[tokio::test]
    async fn limit_is_clamped_to_50() {
        let stub = Arc::new(StubRepo::default());
        let usecase = SearchPersonsUseCase::new(stub.clone());
        let _ = usecase
            .execute(
                SearchQuery {
                    q: "Ngono".into(),
                    nationality: Some("CM".into()),
                    limit: 9_999,
                },
                None,
            )
            .await
            .unwrap();
        let captured = stub.last_query.lock().unwrap().clone().unwrap();
        assert_eq!(captured.0, "Ngono");
        assert_eq!(captured.1.as_deref(), Some("CM"));
        assert_eq!(captured.3, 50, "limit must clamp to 50");
    }

    #[tokio::test]
    async fn empty_nationality_filter_is_dropped() {
        let stub = Arc::new(StubRepo::default());
        let usecase = SearchPersonsUseCase::new(stub.clone());
        let _ = usecase
            .execute(
                SearchQuery {
                    q: "Ngono".into(),
                    nationality: Some("   ".into()),
                    limit: 5,
                },
                None,
            )
            .await
            .unwrap();
        let captured = stub.last_query.lock().unwrap().clone().unwrap();
        assert!(captured.1.is_none());
    }

    /// FIND-005 RBAC scope. When the caller is non-admin the handler
    /// passes the caller's subject as `created_by_filter`; the use
    /// case must propagate it verbatim to the repository.
    #[tokio::test]
    async fn created_by_filter_propagates_to_repository() {
        let stub = Arc::new(StubRepo::default());
        let usecase = SearchPersonsUseCase::new(stub.clone());
        let _ = usecase
            .execute(
                SearchQuery {
                    q: "Ngono".into(),
                    nationality: None,
                    limit: 10,
                },
                Some("spiffe://recor.cm/declarant-42"),
            )
            .await
            .unwrap();
        let captured = stub.last_query.lock().unwrap().clone().unwrap();
        assert_eq!(
            captured.2.as_deref(),
            Some("spiffe://recor.cm/declarant-42")
        );
    }
}
