//! Service-level error type. Maps domain / application / infrastructure
//! errors to HTTP responses at the API boundary.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;
use tracing::error;

use crate::application::{
    DissolveError, GetError, RegisterError, RepositoryError, SearchError, UpdateError,
};
use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Domain(DomainError),
    #[error(transparent)]
    Repository(RepositoryError),
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("authorization denied: {0}")]
    AuthorizationDenied(&'static str),
    #[error("idempotency conflict: same key, different request body")]
    IdempotencyConflict,
    #[error("malformed request: {0}")]
    BadRequest(String),
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

impl From<UpdateError> for ServiceError {
    fn from(value: UpdateError) -> Self {
        match value {
            UpdateError::Domain(e) => Self::Domain(e),
            UpdateError::Repository(e) => Self::Repository(e),
            UpdateError::NotFound(id) => Self::NotFound(id.to_string()),
        }
    }
}

impl From<DissolveError> for ServiceError {
    fn from(value: DissolveError) -> Self {
        match value {
            DissolveError::Domain(e) => Self::Domain(e),
            DissolveError::Repository(e) => Self::Repository(e),
            DissolveError::NotFound(id) => Self::NotFound(id.to_string()),
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
            SearchError::Repository(e) => Self::Repository(e),
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self {
            ServiceError::Domain(e) => {
                let kind = match e {
                    DomainError::AlreadyRegistered(_) => "conflict",
                    DomainError::AlreadyDissolved { .. } => "conflict",
                    DomainError::UpdateOnDissolvedEntity(_) => "conflict",
                    DomainError::EntityNotFound(_) => "not_found",
                    DomainError::UpdateBeforeRegistration(_) => "not_found",
                    DomainError::DissolveBeforeRegistration(_) => "not_found",
                    _ => "bad_request",
                };
                let status = match e {
                    DomainError::AlreadyRegistered(_) => StatusCode::CONFLICT,
                    DomainError::AlreadyDissolved { .. } => StatusCode::CONFLICT,
                    DomainError::UpdateOnDissolvedEntity(_) => StatusCode::CONFLICT,
                    DomainError::EntityNotFound(_) => StatusCode::NOT_FOUND,
                    DomainError::UpdateBeforeRegistration(_) => StatusCode::NOT_FOUND,
                    DomainError::DissolveBeforeRegistration(_) => StatusCode::NOT_FOUND,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, kind, e.to_string())
            }
            ServiceError::Repository(RepositoryError::Conflict { .. }) => (
                StatusCode::CONFLICT,
                "optimistic_concurrency_conflict",
                self.to_string(),
            ),
            ServiceError::Repository(RepositoryError::DuplicateIdentityTuple { .. }) => (
                StatusCode::CONFLICT,
                "duplicate_identity_tuple",
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
            ServiceError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
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
