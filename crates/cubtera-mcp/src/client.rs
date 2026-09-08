//! HTTP client for `cubtera-server`.
//!
//! P7 turns `cubtera-mcp` into a pure client of the API instead of an
//! in-process consumer of `cubtera-core`/`cubtera-persistence` - every tool
//! in `server.rs` goes through here. This intentionally mirrors
//! `cubtera-server`'s route surface (`crates/cubtera-server/src/routes/`)
//! one-to-one rather than introducing its own shape.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// An error talking to `cubtera-server` - either the request itself failed
/// (network, timeout, malformed response) or the server returned a
/// non-2xx `application/problem+json` response.
#[derive(Debug, Clone)]
pub struct ClientError {
    pub status: Option<u16>,
    pub message: String,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "cubtera-server returned {status}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ClientError {}

/// The `application/problem+json` body shape `cubtera-server`'s `ApiError`
/// returns (see `crates/cubtera-server/src/error.rs`).
#[derive(Debug, Deserialize, Default)]
struct Problem {
    #[serde(default)]
    detail: String,
}

#[derive(Clone)]
pub struct CubteraApiClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl CubteraApiClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client builds with a fixed, valid config");
        Self {
            base_url,
            api_key,
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ClientError> {
        let mut req = self.http.get(self.url(path));
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }
        let resp = req.send().await.map_err(|e| ClientError {
            status: None,
            message: format!("request to cubtera-server failed: {e}"),
        })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| ClientError {
            status: Some(status.as_u16()),
            message: format!("failed to read cubtera-server response body: {e}"),
        })?;
        if !status.is_success() {
            let message = serde_json::from_slice::<Problem>(&bytes)
                .map(|p| p.detail)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(ClientError {
                status: Some(status.as_u16()),
                message,
            });
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| ClientError {
            status: Some(status.as_u16()),
            message: format!("failed to parse cubtera-server response: {e}"),
        })
    }

    pub async fn list_orgs(&self) -> Result<Value, ClientError> {
        self.get("/v1/orgs", &[]).await
    }

    pub async fn list_dim_types(&self, org: &str) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dim-types"), &[]).await
    }

    pub async fn list_dimension_names(
        &self,
        org: &str,
        dim_type: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}"), &[]).await
    }

    pub async fn get_dimension(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/{name}"), &[])
            .await
    }

    pub async fn get_dimension_defaults(
        &self,
        org: &str,
        dim_type: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/defaults"), &[])
            .await
    }

    pub async fn get_dimension_schema(
        &self,
        org: &str,
        dim_type: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/schema"), &[])
            .await
    }

    pub async fn get_dimension_parent(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/{name}/parent"), &[])
            .await
    }

    pub async fn get_dimension_children(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/{name}/children"), &[])
            .await
    }

    pub async fn validate_dimension(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/dims/{dim_type}/{name}/validate"), &[])
            .await
    }

    pub async fn list_units(&self, org: &str) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/units"), &[]).await
    }

    pub async fn get_unit_manifest(
        &self,
        org: &str,
        unit_name: &str,
    ) -> Result<Value, ClientError> {
        self.get(&format!("/v1/{org}/units/{unit_name}"), &[]).await
    }

    /// Reads a producer unit's published state via `cubtera-server`'s v3
    /// state-mesh endpoint (`GET /v1/{org}/state`, backed by
    /// `Store::get_output_set`) - not a consumer's `[inputs]` projection,
    /// which only happens inside `cubtera plan`/`apply`.
    pub async fn get_unit_state(
        &self,
        org: &str,
        unit_name: &str,
        dims: &[String],
        ext: &[String],
    ) -> Result<Value, ClientError> {
        let mut query: Vec<(&str, String)> = vec![("unit", unit_name.to_string())];
        if !dims.is_empty() {
            query.push(("dims", dims.join(",")));
        }
        if !ext.is_empty() {
            query.push(("ext", ext.join(",")));
        }
        self.get(&format!("/v1/{org}/state"), &query).await
    }

    pub async fn get_deployment_log(
        &self,
        org: &str,
        query: &HashMap<String, String>,
        limit: Option<usize>,
    ) -> Result<Value, ClientError> {
        let q = query
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut params: Vec<(&str, String)> = Vec::new();
        if !q.is_empty() {
            params.push(("q", q));
        }
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        self.get(&format!("/v1/{org}/dlog"), &params).await
    }
}
