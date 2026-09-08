//! API key auth middleware.
//!
//! Identical pattern to `crates/cubtera-api/src/auth.rs`: opt-in (no
//! `apiKey`/`CUBTERA_API_KEY` set -> unauthenticated, local-dev default),
//! constant-time comparison, `problem+json` on rejection. Distinct from
//! `crate::policy`'s per-unit authorization check - this middleware only
//! answers "is the caller allowed to talk to this server at all", not
//! "is this caller allowed to run *this* unit against *these*
//! dimensions".

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

/// The actor a request is running as - `x-actor` if the caller supplies
/// one (a real deployment would derive this from whatever authenticates
/// the API key, e.g. a per-caller key -> identity mapping; nothing that
/// elaborate exists yet, so this is the honest interim shape), falling
/// back to `"anonymous"` so `crate::policy`'s `actor.name == "..."` rules
/// always have something to match against.
pub fn actor_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-actor")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string()
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
    }

    #[test]
    fn actor_defaults_to_anonymous() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(actor_from_headers(&headers), "anonymous");
    }
}
