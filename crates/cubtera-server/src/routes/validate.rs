//! `GET /v1/{org}/validate` - server-side `cubtera validate` (see
//! `crates/cubtera/src/commands/validate.rs`'s doc comment for what's
//! checked: JSON schema + dim-graph edges per dimension).

use crate::error::ApiError;
use crate::run_support::inventory_port;
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::Json;
use cubtera_app::{load_dim_graph, ResolveUseCase, ValidateUseCase};
use cubtera_kernel::Ident;
use cubtera_model::ModelError;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn validate(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let inventory = inventory_port(&state.config);

    let chain: Vec<Ident> = state
        .config
        .dim_relations
        .iter()
        .map(|s| Ident::parse(s))
        .collect::<Result<_, _>>()
        .map_err(cubtera_app::AppError::from)?;

    let graph = load_dim_graph(inventory.as_ref(), &org, &chain).await?;
    if let Err(errors) = graph.validate() {
        return Err(cubtera_app::AppError::Model(ModelError::Graph(errors)).into());
    }

    let use_case = ValidateUseCase::new(ResolveUseCase::new(inventory));
    let report = use_case.validate_fleet(&graph, &org, &chain).await?;

    Ok(Json(json!({
        "valid": report.is_ok(),
        "results": report.results.iter().map(|r| json!({
            "key": r.key.to_string(),
            "valid": r.is_ok(),
            "schema_errors": r.schema_errors,
            "graph_errors": r.graph_errors,
        })).collect::<Vec<_>>(),
    })))
}
