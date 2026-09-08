//! Dimension inventory endpoints - v3-native, `cubtera_app::ResolveUseCase`
//! over `FsInventoryPort` (see `run_support::inventory_port`) instead of
//! v2's `DimensionService`/`Repositories`. This, plus `units.rs`/`dlog.rs`,
//! is what lets `cubtera-mcp` (P7) drop its direct `cubtera-core`/
//! `cubtera-persistence` dependency and become a pure HTTP client of
//! `cubtera-server`.

use crate::error::ApiError;
use crate::run_support::inventory_port;
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::Json;
use cubtera_app::{AppError, ResolveUseCase};
use cubtera_kernel::Ident;
use serde_json::{json, Value};
use std::sync::Arc;

fn resolve(state: &AppState) -> ResolveUseCase {
    ResolveUseCase::new(inventory_port(&state.config))
}

fn ident(raw: &str) -> Result<Ident, ApiError> {
    Ident::parse(raw)
        .map_err(AppError::from)
        .map_err(ApiError::from)
}

pub async fn list_orgs(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let orgs = resolve(&state).list_orgs().await?;
    Ok(Json(json!(orgs)))
}

pub async fn list_dim_types(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let types = resolve(&state).list_types(&org).await?;
    Ok(Json(json!(types)))
}

pub async fn list_dimension_names(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let names = resolve(&state).list_names(&org, &ident(&dim_type)?).await?;
    Ok(Json(json!(names)))
}

pub async fn get_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dim_type = ident(&dim_type)?;
    let name = ident(&name)?;
    let uc = resolve(&state);
    let dim = uc.resolve(&org, &dim_type, &name).await?;
    let kids = uc
        .kids_of(&org, &state.config.dim_relations, &dim_type, &name)
        .await?;
    Ok(Json(dim.to_response_json(&kids)))
}

pub async fn get_defaults(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dim = resolve(&state)
        .get_defaults(&org, &ident(&dim_type)?)
        .await?;
    Ok(Json(
        dim.as_ref()
            .map(|d| d.to_response_json(&[]))
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_schema(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let schema = resolve(&state).get_schema(&org, &ident(&dim_type)?).await?;
    Ok(Json(schema.unwrap_or(Value::Null)))
}

pub async fn get_parent(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let parent = resolve(&state)
        .get_parent(&org, &ident(&dim_type)?, &ident(&name)?)
        .await?;
    Ok(Json(
        parent
            .as_ref()
            .map(|d| d.to_response_json(&[]))
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_children(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dim_type = ident(&dim_type)?;
    let name = ident(&name)?;
    let children = resolve(&state)
        .get_children(&org, &state.config.dim_relations, &dim_type, &name)
        .await?;
    Ok(Json(json!(children
        .iter()
        .map(|d| d.to_response_json(&[]))
        .collect::<Vec<_>>())))
}

pub async fn validate_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let errors = resolve(&state)
        .validate_schema(&org, &ident(&dim_type)?, &ident(&name)?)
        .await?;
    let valid = errors.is_empty();
    Ok(Json(json!({ "valid": valid, "errors": errors })))
}
