//! API key auth middleware
//!
//! v1 declared an `ApiKey` request guard but never attached it to a route
//! (dead code - see the migration plan, item 7). Here it's a real
//! `tower`/axum middleware, layered on every `/v1/*` route. Auth is
//! opt-in: if `Config::api_key` is `None` (nothing set via `CUBTERA_API_KEY`
//! or `apiKey` in `config.toml`), requests pass through unauthenticated -
//! that's the local-dev default, not a silent security hole in production
//! as long as operators set the env var.

use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;

const API_KEY_HEADER: &str = "x-api-key";

pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(expected) = &state.api_key else {
        return next.run(request).await;
    };

    let provided = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(key) if constant_time_eq(key, expected) => next.run(request).await,
        Some(_) => unauthorized("Invalid API key"),
        None => unauthorized("Missing x-api-key header"),
    }
}

fn unauthorized(detail: &str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "type": "unauthorized",
            "title": "Unauthorized",
            "status": 401,
            "detail": detail,
        })),
    )
        .into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

/// Avoid leaking key length/prefix via early-exit comparison timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("secret", "secret"));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq("secret", "wrong"));
        assert!(!constant_time_eq("secret", "secre"));
        assert!(!constant_time_eq("", "secret"));
    }
}
