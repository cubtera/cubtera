//! Dimension inventory endpoints - `cubtera-api`'s `dimensions.rs`/`orgs.rs`
//! ported onto `cubtera-server`'s per-request `Repositories::from_config`
//! pattern (see `validate.rs`/`fleet.rs`) instead of a long-lived `App`.
//! Backed by v2's `DimensionService` directly: v3's `InventoryPort`/
//! `ResolveUseCase` (P3) don't yet have an equivalent for `list_orgs`/
//! `list_dim_types` (dim types are declared via `config.toml`'s
//! `dimRelations` in v3, not discovered from the filesystem), so this is
//! the same seam `run_support.rs` already crosses for manifest reads.
//!
//! This, plus `units.rs`/`dlog.rs`, is what lets `cubtera-mcp` (P7) drop
//! its direct `cubtera-core`/`cubtera-persistence` dependency and become a
//! pure HTTP client of `cubtera-server`.

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::Json;
use cubtera_core::services::{DimensionService, SchemaValidation};
use cubtera_domain::Dimension;
use cubtera_persistence::Repositories;
use serde_json::{json, Value};
use std::sync::Arc;

async fn dimension_service(state: &AppState) -> Result<DimensionService, ApiError> {
    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let hierarchy = Repositories::hierarchy(&state.config);
    Ok(DimensionService::new(repos.inventory, hierarchy))
}

pub async fn list_orgs(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let orgs = dimensions.get_orgs().await?;
    Ok(Json(json!(orgs)))
}

pub async fn list_dim_types(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let types = dimensions.get_types(&org).await?;
    Ok(Json(json!(types)))
}

pub async fn list_dimension_names(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let names = dimensions.get_all_names(&org, &dim_type).await?;
    Ok(Json(json!(names)))
}

pub async fn get_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let dim = dimensions.get_by_name(&org, &dim_type, &name).await?;
    Ok(Json(dim.to_response_json()))
}

pub async fn get_defaults(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let dim = dimensions.get_defaults(&org, &dim_type).await?;
    Ok(Json(
        dim.as_ref()
            .map(Dimension::to_response_json)
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_schema(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let schema = dimensions.get_schema(&org, &dim_type).await?;
    Ok(Json(schema.unwrap_or(Value::Null)))
}

pub async fn get_parent(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let parent = dimensions.get_parent(&org, &dim_type, &name).await?;
    Ok(Json(
        parent
            .as_ref()
            .map(Dimension::to_response_json)
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_children(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let children = dimensions.get_children(&org, &dim_type, &name).await?;
    Ok(Json(json!(children
        .iter()
        .map(Dimension::to_response_json)
        .collect::<Vec<_>>())))
}

pub async fn validate_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dimensions = dimension_service(&state).await?;
    let result = dimensions.validate_schema(&org, &dim_type, &name).await?;
    let (valid, errors) = match result {
        SchemaValidation::NoSchema | SchemaValidation::Valid => (true, Vec::new()),
        SchemaValidation::Invalid(errors) => (false, errors),
    };
    Ok(Json(json!({ "valid": valid, "errors": errors })))
}
