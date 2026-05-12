//! Service-level error type. Maps domain / application / infrastructure
//! errors to HTTP responses at the API boundary.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;
use tracing::error;

use crate::application::{
    GetError, MergeError, RegisterError, RepositoryError, SearchError,
};
use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Domain(DomainError),
    #[error(transparent)]
    Repository(RepositoryError),
    #[error("person not found: {0}")]
    NotFound(String),
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("authorization denied: {0}")]
    AuthorizationDenied(&'static str),
    #[error("idempotency conflict: same key, different request body")]
    IdempotencyConflict,
    #[error("malformed request: {0}")]
    BadRequest(String),
    #[error("admin endpoint disabled (no admin principals configured)")]
    AdminDisabled,
    #[error("internal failure")]
    Internal,
}

impl From<RegisterError> for ServiceError {
    fn from(value: RegisterError) -> Self {
        match value {
            RegisterError::Domain(e) => Self::Domain(e),
            RegisterError::Repository(e) => Self::Repository(e),
        }
    }
}

impl From<GetError> for ServiceError {
    fn from(value: GetError) -> Self {
        match value {
            GetError::NotFound(id) => Self::NotFound(id.to_string()),
            GetError::Repository(e) => Self::Repository(e),
        }
    }
}

impl From<SearchError> for ServiceError {
    fn from(value: SearchError) -> Self {
        match value {
            SearchError::EmptyQuery => Self::BadRequest("query must not be empty".into()),
            SearchError::QueryTooLong => {
                Self::BadRequest("query length exceeds maximum of 256 characters".into())
            }
            SearchError::Repository(e) => Self::Repository(e),
        }
    }
}

impl From<MergeError> for ServiceError {
    fn from(value: MergeError) -> Self {
        match value {
            MergeError::Domain(e) => Self::Domain(e),
            MergeError::Repository(e) => Self::Repository(e),
            MergeError::SourceNotFound(id) => Self::NotFound(id.to_string()),
            MergeError::TargetNotFound(id) => Self::NotFound(id.to_string()),
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self {
            ServiceError::Domain(e) => {
                let (kind, status) = match e {
                    DomainError::AlreadyRegistered(_) => ("conflict", StatusCode::CONFLICT),
                    DomainError::AlreadyMerged { .. } => ("conflict", StatusCode::CONFLICT),
                    DomainError::MergeIntoSelf(_) => ("bad_request", StatusCode::BAD_REQUEST),
                    DomainError::MergeTargetIsMerged(_) => {
                        ("conflict", StatusCode::CONFLICT)
                    }
                    DomainError::PersonNotFound(_) => ("not_found", StatusCode::NOT_FOUND),
                    DomainError::UpdateBeforeRegister(_)
                    | DomainError::MergeBeforeRegister(_) => {
                        ("not_found", StatusCode::NOT_FOUND)
                    }
                    DomainError::EmptyActorPrincipal => {
                        ("bad_request", StatusCode::BAD_REQUEST)
                    }
                    DomainError::ValueObject(_) => ("bad_request", StatusCode::BAD_REQUEST),
                };
                (status, kind, e.to_string())
            }
            ServiceError::Repository(RepositoryError::Conflict { .. }) => (
                StatusCode::CONFLICT,
                "optimistic_concurrency_conflict",
                self.to_string(),
            ),
            ServiceError::Repository(e) => {
                error!(error = ?e, "repository failure");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal failure".to_string(),
                )
            }
            ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, "not_found", self.to_string())
            }
            ServiceError::AuthenticationRequired => (
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                self.to_string(),
            ),
            ServiceError::AuthorizationDenied(_) => {
                (StatusCode::FORBIDDEN, "forbidden", self.to_string())
            }
            ServiceError::IdempotencyConflict => (
                StatusCode::CONFLICT,
                "idempotency_conflict",
                self.to_string(),
            ),
            ServiceError::BadRequest(_) => {
                (StatusCode::BAD_REQUEST, "bad_request", self.to_string())
            }
            ServiceError::AdminDisabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                "admin_disabled",
                self.to_string(),
            ),
            ServiceError::Internal => {
                error!(error = ?self, "internal failure");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal failure".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": {
                "kind": kind,
                "message": message,
            }
        }));
        (status, body).into_response()
    }
}
