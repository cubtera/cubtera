//! `POST /v1/{org}/units/{unit}/plan`, `POST /v1/{org}/units/{unit}/apply`,
//! `GET /v1/{org}/runs/{run_id}`, `GET /v1/{org}/runs/{run_id}/log`,
//! `GET /v1/{org}/runs/{run_id}/log/stream` - server-side `cubtera plan`/
//! `cubtera apply --plan`/`cubtera explain run`
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
//!
//! `apply` queues the run and returns immediately (P7 log streaming):
//! see `apply`'s and `log_stream`'s own doc comments, and
//! `crate::log_hub`.

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

    let inputs = build_input_requests(&state.config, &org, &prepared.unit)
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

/// Queues the run and returns immediately with the `Run` row in
/// `Queued` status - the actual execution happens in a `tokio::spawn`ed
/// background task (`RunUseCase::run_and_finish`). This is deliberate,
/// not a shortcut: it's what makes `GET .../runs/{run_id}/log/stream`
/// (below) a genuine live tail instead of a poll-until-the-artifact-shows
/// -up hack - see `crate::log_hub`'s doc comment. Poll `GET
/// .../runs/{run_id}` for the final status/exit code.
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

    let inputs = build_input_requests(&state.config, &org, &prepared.unit)
        .await
        .map_err(ApiError::from)?;

    let req = ApplyRequest {
        runner_type,
        command,
        auto_approve: body.auto_approve,
        actor: Ident::parse(&actor).map_err(cubtera_app::AppError::from)?,
        config_digest: config_digest(&state.config).map_err(ApiError::from)?,
        publish_outputs,
        outputs_schema_version,
        lease_ttl: std::time::Duration::from_secs(body.lease_ttl_seconds),
        inputs,
    };

    let queued = prepared
        .use_case
        .queue_apply(&PlanId::new(body.plan_id), &req)
        .await?;
    let run = queued.run.clone();

    let sink = state.log_hub.register(run.id.as_str());
    let use_case = prepared.use_case;
    let hub = state.log_hub.clone();
    let run_id = run.id.clone();
    tokio::spawn(async move {
        // Errors are already persisted onto the `Run` row itself by
        // `run_and_finish` (see its doc comment) - nothing more useful to
        // do with them here than let the row speak for itself via
        // `GET .../runs/{run_id}`.
        let _ = use_case.run_and_finish(queued, &req, Some(sink)).await;
        hub.unregister(run_id.as_str());
    });

    Ok(Json(serde_json::to_value(run).map_err(|e| {
        ApiError::bad_request(format!("failed to serialize run: {e}"))
    })?))
}

/// `GET /v1/{org}/runs/{run_id}/log/stream` (SSE) - live-tails a run's
/// combined stdout+stderr while it's in flight (subscribing to
/// `AppState::log_hub`), then closes after one final `event: done` frame.
/// If the run has already finished (or `log_hub` has no entry for it for
/// any other reason - e.g. this server process restarted mid-run), falls
/// back to replaying the finished run's stored artifact
/// (`Run::logs_ref`, same bytes `GET .../runs/{run_id}/log` serves) as a
/// single `data:` frame followed immediately by `event: done` - a client
/// that always opens this endpoint (rather than choosing between it and
/// the plain `/log` route based on whether the run *looks* finished)
/// gets the right behavior either way.
pub async fn log_stream(
    State(state): State<Arc<AppState>>,
    Path((_org, run_id)): Path<(String, String)>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::convert::Infallible;
    use std::pin::Pin;
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::{Stream, StreamExt};

    // Both branches below build a differently-shaped combinator chain, so
    // erase to a common boxed trait object up front - the only way to
    // give both branches (and thus the whole `impl IntoResponse` return
    // type) one concrete type to agree on.
    type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

    let done = || Ok(Event::default().event("done").data(""));
    let run_id = RunId::new(run_id);

    if let Some(rx) = state.log_hub.subscribe(run_id.as_str()) {
        // Live tail: forward every chunk as it arrives, until the sender
        // side (`RunUseCase::run_and_finish`, via `LogHub::unregister`)
        // drops the channel.
        let live = BroadcastStream::new(rx).filter_map(|item| match item {
            Ok(chunk) => Some(Ok(Event::default()
                .event("chunk")
                .data(String::from_utf8_lossy(&chunk)))),
            // A slow client fell behind the broadcast buffer - skip
            // ahead rather than erroring the whole stream.
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
        });
        let stream: EventStream = Box::pin(live.chain(tokio_stream::once(done())));
        return Ok(Sse::new(stream).keep_alive(KeepAlive::default()));
    }

    // Not currently in flight - replay whatever's already stored (same
    // bytes `GET .../runs/{run_id}/log` serves), if anything.
    let use_case =
        crate::run_support::build_use_case(&state.config, state.config.temp_folder_path.clone())
            .map_err(ApiError::from)?;
    let found = use_case.explain(&run_id).await?;

    let body = match &found.logs_ref {
        Some(logs_ref) => {
            let digest = cubtera_kernel::Digest::from_hex(logs_ref)
                .ok_or_else(|| ApiError::bad_request(format!("malformed logs_ref {logs_ref:?}")))?;
            let store: Arc<dyn cubtera_store::Store> = Arc::new(
                cubtera_store::SqliteStore::open(&state.config.store_path)
                    .map_err(|e| ApiError::bad_request(format!("failed to open store: {e}")))?,
            );
            store
                .get_artifact(&digest)
                .await
                .map_err(|e| ApiError::bad_request(e.to_string()))?
                .ok_or_else(|| ApiError::not_found(format!("no artifact for digest {logs_ref}")))?
        }
        None => Vec::new(),
    };
    let chunk = Ok(Event::default()
        .event("chunk")
        .data(String::from_utf8_lossy(&body)));
    let stream: EventStream = Box::pin(tokio_stream::once(chunk).chain(tokio_stream::once(done())));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn explain(
    State(state): State<Arc<AppState>>,
    Path((_org, run_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let use_case =
        crate::run_support::build_use_case(&state.config, state.config.temp_folder_path.clone())
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

    let use_case =
        crate::run_support::build_use_case(&state.config, state.config.temp_folder_path.clone())
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
