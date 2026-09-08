//! `application/problem+json` (RFC 7807) error responses.
//!
//! Same shape as `crates/cubtera-api/src/error.rs`, mapping
//! `cubtera_app::AppError` instead of v2's `cubtera_core::error::AppError`.
//! The whole point of P7's server is that it can drive `cubtera-app`'s
//! use cases directly, so its error boundary maps *that* crate's error
//! type. `run_support`'s v2 seam (`UnitService::build_unit_with_extensions`,
//! materializing a unit's files) still raises `cubtera_core::error::AppError`
//! for a handful of failure modes (access denied, manifest not found), so
//! this also accepts that type via a second `From` impl.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

pub struct ApiError {
    status: StatusCode,
    problem_type: &'static str,
    detail: String,
}

impl From<cubtera_app::AppError> for ApiError {
    fn from(err: cubtera_app::AppError) -> Self {
        use cubtera_app::AppError as E;
        let (status, problem_type) = match &err {
            E::NotFound { .. } => (StatusCode::NOT_FOUND, "not-found"),
            E::Validation(_) => (StatusCode::BAD_REQUEST, "validation"),
            E::Model(_) => (StatusCode::BAD_REQUEST, "model"),
            E::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "backend"),
        };
        Self {
            status,
            problem_type,
            detail: err.to_string(),
        }
    }
}

impl From<cubtera_core::error::AppError> for ApiError {
    fn from(err: cubtera_core::error::AppError) -> Self {
        use cubtera_core::error::AppError as E;
        let (status, problem_type) = match &err {
            E::NotFound { .. } => (StatusCode::NOT_FOUND, "not-found"),
            E::Validation(_) | E::Domain(_) => (StatusCode::BAD_REQUEST, "validation"),
            E::AccessDenied(_) => (StatusCode::FORBIDDEN, "access-denied"),
            E::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "config"),
            E::Repository(_) | E::Runner(_) | E::Io(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        Self {
            status,
            problem_type,
            detail: err.to_string(),
        }
    }
}

/// `Repositories::from_config`'s error type - a bare `String`, not an
/// `AppError` (see `cubtera_persistence::Repositories::from_config`).
impl From<String> for ApiError {
    fn from(err: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            problem_type: "config",
            detail: err,
        }
    }
}

impl From<Box<dyn std::error::Error>> for ApiError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            problem_type: "internal",
            detail: err.to_string(),
        }
    }
}

impl ApiError {
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            problem_type: "access-denied",
            detail: detail.into(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            problem_type: "not-found",
            detail: detail.into(),
        }
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            problem_type: "validation",
            detail: detail.into(),
        }
    }
}

#[derive(Serialize)]
struct Problem {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!("{}", self.detail);
        }
        let body = Problem {
            problem_type: self.problem_type,
            title: self.status.canonical_reason().unwrap_or("Error"),
            status: self.status.as_u16(),
            detail: self.detail,
        };
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
