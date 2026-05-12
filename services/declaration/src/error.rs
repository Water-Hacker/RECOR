//! Service-level error type. Maps domain / application / infrastructure
//! errors to HTTP responses at the API boundary; internal contexts use
//! the inner error directly.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;
use tracing::error;

use crate::application::{
    GetError, RecordVerificationError, RepositoryError, SubmitError, SupersedeError,
};
use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Domain(DomainError),
    #[error(transparent)]
    Repository(RepositoryError),
    #[error("declaration not found: {0}")]
    NotFound(String),
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("authorization denied: {0}")]
    AuthorizationDenied(&'static str),
    #[error("idempotency conflict: same key, different request body")]
    IdempotencyConflict,
    #[error("attestation verification failed: {0}")]
    AttestationVerificationFailed(String),
    #[error("malformed request: {0}")]
    BadRequest(String),
    #[error("internal failure")]
    Internal,
}

impl From<SubmitError> for ServiceError {
    fn from(value: SubmitError) -> Self {
        match value {
            SubmitError::Domain(e) => Self::Domain(e),
            SubmitError::Repository(e) => Self::Repository(e),
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

impl From<RecordVerificationError> for ServiceError {
    fn from(value: RecordVerificationError) -> Self {
        match value {
            RecordVerificationError::Domain(e) => Self::Domain(e),
            RecordVerificationError::Repository(e) => Self::Repository(e),
        }
    }
}

impl From<SupersedeError> for ServiceError {
    fn from(value: SupersedeError) -> Self {
        match value {
            SupersedeError::Domain(e) => Self::Domain(e),
            SupersedeError::Repository(e) => Self::Repository(e),
            SupersedeError::OldDeclarationNotFound(id) => Self::NotFound(id.to_string()),
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self {
            ServiceError::Domain(e) => {
                let kind = match e {
                    DomainError::AlreadySubmitted(_) => "conflict",
                    DomainError::VerificationCaseMismatch { .. } => "conflict",
                    DomainError::AlreadySuperseded(_) => "conflict",
                    DomainError::AttestationPrincipalMismatch { .. } => "forbidden",
                    DomainError::SupersedeNotOwner { .. } => "forbidden",
                    DomainError::VerificationOutcomeBeforeSubmit(_) => "not_found",
                    DomainError::SupersedeBeforeSubmit(_) => "not_found",
                    _ => "bad_request",
                };
                let status = match e {
                    DomainError::AlreadySubmitted(_) => StatusCode::CONFLICT,
                    DomainError::VerificationCaseMismatch { .. } => StatusCode::CONFLICT,
                    DomainError::AlreadySuperseded(_) => StatusCode::CONFLICT,
                    DomainError::AttestationPrincipalMismatch { .. } => StatusCode::FORBIDDEN,
                    DomainError::SupersedeNotOwner { .. } => StatusCode::FORBIDDEN,
                    DomainError::VerificationOutcomeBeforeSubmit(_) => StatusCode::NOT_FOUND,
                    DomainError::SupersedeBeforeSubmit(_) => StatusCode::NOT_FOUND,
                    _ => StatusCode::BAD_REQUEST,
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
            ServiceError::AttestationVerificationFailed(_) => {
                (StatusCode::UNAUTHORIZED, "bad_attestation", self.to_string())
            }
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
