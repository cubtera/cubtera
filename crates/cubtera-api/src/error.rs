//! `application/problem+json` (RFC 7807) error responses
//!
//! Plan item 7: handlers used to return a bare `StatusCode` on error,
//! discarding the message; item 9: `AppError` maps to an HTTP status here,
//! at the interface boundary, exactly like the CLI maps it to an exit code
//! in `cubtera::error` - neither domain nor core know about HTTP or process
//! exit codes.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cubtera_core::error::AppError;
use serde::Serialize;

/// Wraps [`AppError`] so it can be returned directly from an axum handler
/// (`Result<Json<T>, ApiError>`) and turned into a `problem+json` response.
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

#[derive(Serialize)]
struct Problem {
    /// A short, machine-readable error category (RFC 7807 calls this
    /// `type`, normally a URI; a stable slug is enough for our purposes).
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

fn status_and_type(err: &AppError) -> (StatusCode, &'static str) {
    match err {
        AppError::NotFound { .. } => (StatusCode::NOT_FOUND, "not-found"),
        AppError::Validation(_) | AppError::Domain(_) => (StatusCode::BAD_REQUEST, "validation"),
        AppError::AccessDenied(_) => (StatusCode::FORBIDDEN, "access-denied"),
        AppError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "config"),
        AppError::Repository(_) | AppError::Runner(_) | AppError::Io(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, problem_type) = status_and_type(&self.0);
        if status.is_server_error() {
            tracing::error!("{}", self.0);
        }

        let body = Problem {
            problem_type,
            title: status.canonical_reason().unwrap_or("Error"),
            status: status.as_u16(),
            detail: self.0.to_string(),
        };

        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
