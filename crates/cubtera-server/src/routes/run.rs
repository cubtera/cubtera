//! `POST /v1/{org}/units/{unit}/plan`, `POST /v1/{org}/units/{unit}/apply`,
//! `GET /v1/{org}/runs/{run_id}`, `GET /v1/{org}/runs/{run_id}/log` -
//! server-side `cubtera plan`/`cubtera apply --plan`/`cubtera explain run`
//! (`crates/cubtera/src/commands/{plan,apply,explain}.rs`), the reason
//! this server exists at all instead of just being `cubtera-api` with
//! more routes: v2's REST API (`cubtera-api`) can only *read* the
//! inventory, never run anything (see this crate's `description` in
//! `Cargo.toml`).
//!
//! Authorization: `crate::policy::check` runs against the resolved `Unit`
//! before `apply` executes anything, using the `x-actor` header
//! (`crate::auth::actor_from_headers`) and the run's first command word as
//! `op`. `plan` is read-only (never touches real infrastructure) and is
//! deliberately not gated the same way - matching v2's `AccessPolicy`,
//! which likewise only ever blocked inside the run pipeline, never a
//! plan-only/dry-run step.

use crate::error::ApiError;
use crate::policy;
use crate::run_support::{build_input_requests, config_digest, prepare};
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use cubtera_app::{ApplyRequest, PlanRequest};
use cubtera_kernel::Ident;
use cubtera_model::{PlanId, RunId};
use cubtera_persistence::Repositories;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::auth::actor_from_headers;

#[derive(Deserialize)]
pub struct PlanBody {
    #[serde(default)]
    pub dims: Vec<String>,
    #[serde(default)]
    pub ext: Vec<String>,
    #[serde(default)]
    pub command: Vec<String>,
    pub actor: Option<String>,
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: i64,
}

fn default_ttl_seconds() -> i64 {
    3600
}

pub async fn plan(
    State(state): State<Arc<AppState>>,
    Path((org, unit)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<PlanBody>,
) -> Result<Json<Value>, ApiError> {
    let prepared = prepare(&state.config, &org, &unit, &body.dims, &body.ext)
        .await
        .map_err(ApiError::from)?;
    let runner_type = prepared.unit.manifest.runner_type().as_str().to_string();
    let command = if body.command.is_empty() {
        vec!["plan".to_string()]
    } else {
        body.command
    };
    let actor = body.actor.unwrap_or_else(|| actor_from_headers(&headers));

    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(ApiError::from)?;
    let inputs = build_input_requests(&org, &repos, &prepared.unit)
        .await
        .map_err(ApiError::from)?;

    let plan_result = prepared
        .use_case
        .plan(PlanRequest {
            instance: prepared.instance.clone(),
            runner_type,
            command,
            actor: Ident::parse(&actor).map_err(cubtera_app::AppError::from)?,
            config_digest: config_digest(&state.config).map_err(ApiError::from)?,
            ttl_seconds: body.ttl_seconds,
            inputs,
        })
        .await?;

    Ok(Json(serde_json::to_value(plan_result).map_err(|e| {
        ApiError::bad_request(format!("failed to serialize plan: {e}"))
    })?))
}

#[derive(Deserialize)]
pub struct ApplyBody {
    #[serde(default)]
    pub dims: Vec<String>,
    #[serde(default)]
    pub ext: Vec<String>,
    pub plan_id: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub auto_approve: bool,
    pub actor: Option<String>,
    #[serde(default = "default_lease_ttl_seconds")]
    pub lease_ttl_seconds: u64,
    #[serde(default = "default_outputs_schema_version")]
    pub outputs_schema_version: String,
}

fn default_lease_ttl_seconds() -> u64 {
    300
}

fn default_outputs_schema_version() -> String {
    "1.0.0".to_string()
}

pub async fn apply(
    State(state): State<Arc<AppState>>,
    Path((org, unit)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<ApplyBody>,
) -> Result<Json<Value>, ApiError> {
    let prepared = prepare(&state.config, &org, &unit, &body.dims, &body.ext)
        .await
        .map_err(ApiError::from)?;
    let runner_type = prepared.unit.manifest.runner_type().as_str().to_string();
    let command = if body.command.is_empty() {
        vec!["apply".to_string()]
    } else {
        body.command
    };
    let actor = body.actor.unwrap_or_else(|| actor_from_headers(&headers));
    let op = command.first().map(String::as_str).unwrap_or("apply");

    // Authz gate - see this module's doc comment for why `plan` doesn't
    // get the same check.
    policy::check(&prepared.unit, &actor, op)?;

    let publish_outputs = prepared
        .unit
        .manifest
        .outputs
        .as_ref()
        .map(|o| o.publish)
        .unwrap_or(false);
    let outputs_schema_version = semver::Version::parse(&body.outputs_schema_version)
        .map_err(|e| ApiError::bad_request(format!("invalid outputs_schema_version: {e}")))?;

    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(ApiError::from)?;
    let inputs = build_input_requests(&org, &repos, &prepared.unit)
        .await
        .map_err(ApiError::from)?;

    let run = prepared
        .use_case
        .apply(
            &PlanId::new(body.plan_id),
            ApplyRequest {
                runner_type,
                command,
                auto_approve: body.auto_approve,
                actor: Ident::parse(&actor).map_err(cubtera_app::AppError::from)?,
                config_digest: config_digest(&state.config).map_err(ApiError::from)?,
                publish_outputs,
                outputs_schema_version,
                lease_ttl: std::time::Duration::from_secs(body.lease_ttl_seconds),
                inputs,
            },
        )
        .await?;

    Ok(Json(serde_json::to_value(run).map_err(|e| {
        ApiError::bad_request(format!("failed to serialize run: {e}"))
    })?))
}

pub async fn explain(
    State(state): State<Arc<AppState>>,
    Path((_org, run_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(ApiError::from)?;
    let use_case = crate::run_support::build_use_case(
        &state.config,
        &repos,
        state.config.temp_folder_path.clone(),
    )
    .map_err(ApiError::from)?;

    let found = use_case.explain(&RunId::new(run_id)).await?;
    Ok(Json(serde_json::to_value(found).map_err(|e| {
        ApiError::bad_request(format!("failed to serialize run: {e}"))
    })?))
}

/// Not true live-tailing of an in-progress run (see
/// `cubtera_exec::process::CapturingProcessRunner`'s doc comment) -
/// serves the captured combined stdout+stderr of an already-finished run,
/// via `Run::logs_ref` (a content digest into `Store`'s artifact table).
pub async fn log(
    State(state): State<Arc<AppState>>,
    Path((_org, run_id)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;

    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(ApiError::from)?;
    let use_case = crate::run_support::build_use_case(
        &state.config,
        &repos,
        state.config.temp_folder_path.clone(),
    )
    .map_err(ApiError::from)?;

    let found = use_case.explain(&RunId::new(run_id.clone())).await?;
    let Some(logs_ref) = &found.logs_ref else {
        return Err(ApiError::not_found(format!(
            "run '{run_id}' has no captured log (either it hasn't finished, or its Executor \
             never captures stdio - see cubtera_exec::process::CapturingProcessRunner)"
        )));
    };
    let digest = cubtera_kernel::Digest::from_hex(logs_ref)
        .ok_or_else(|| ApiError::bad_request(format!("malformed logs_ref {logs_ref:?}")))?;

    let store: Arc<dyn cubtera_store::Store> = Arc::new(
        cubtera_store::SqliteStore::open(&state.config.store_path)
            .map_err(|e| ApiError::bad_request(format!("failed to open store: {e}")))?,
    );
    let bytes = store
        .get_artifact(&digest)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("no artifact for digest {logs_ref}")))?;

    Ok(([("content-type", "text/plain; charset=utf-8")], bytes).into_response())
}
