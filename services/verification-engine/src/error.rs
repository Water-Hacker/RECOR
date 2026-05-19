//! Service-level error mapping to HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;
use tracing::error;

use crate::application::{GetError, RepositoryError, SubmitError};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Repository(RepositoryError),
    #[error("verification case not found: {0}")]
    NotFound(String),
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("malformed request: {0}")]
    BadRequest(String),
    #[error("internal failure")]
    Internal,
    /// FIND-002 / FIND-004 (audit Sprint 0): REST endpoints are
    /// admin-only and the allowlist is empty.
    #[error("admin endpoints disabled — ADMIN_PRINCIPALS not configured")]
    AdminDisabled,
    /// FIND-002 / FIND-004: the authenticated principal is not on
    /// the admin allowlist.
    #[error("this principal is not authorised for admin endpoints")]
    NotAdmin,
}

impl From<SubmitError> for ServiceError {
    fn from(value: SubmitError) -> Self {
        match value {
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

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self {
            ServiceError::Repository(e) => {
                error!(error = ?e, "repository failure");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal failure".to_string(),
                )
            }
            ServiceError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            ServiceError::AuthenticationRequired => {
                (StatusCode::UNAUTHORIZED, "authentication_required", self.to_string())
            }
            ServiceError::BadRequest(_) => {
                (StatusCode::BAD_REQUEST, "bad_request", self.to_string())
            }
            ServiceError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "internal failure".to_string(),
            ),
            ServiceError::AdminDisabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                "admin_disabled",
                self.to_string(),
            ),
            ServiceError::NotAdmin => (
                StatusCode::FORBIDDEN,
                "not_admin",
                self.to_string(),
            ),
        };
        let body = Json(json!({ "error": { "kind": kind, "message": message } }));
        (status, body).into_response()
    }
}
