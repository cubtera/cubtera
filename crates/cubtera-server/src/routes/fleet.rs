//! `GET /v1/{org}/fleet/status?unit=..&selector=..&exclude=..` -
//! server-side `cubtera fleet status`
//! (`crates/cubtera/src/commands/fleet.rs`'s `FleetCommands::Status`).

use crate::error::ApiError;
use crate::run_support::build_binding_use_case;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use cubtera_kernel::{DimRef, InstanceId};
use cubtera_model::{Binding, Selector};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct FleetStatusQuery {
    unit: String,
    selector: Option<String>,
    /// Comma-separated `type:name` refs, same coarser semantics as the
    /// CLI's `--exclude` (see `excluded_by_dim_ref`'s doc comment there).
    exclude: Option<String>,
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(params): Query<FleetStatusQuery>,
) -> Result<Json<Value>, ApiError> {
    let binding = Binding {
        id: format!("http:{}", params.unit),
        unit: cubtera_kernel::Ident::parse(&params.unit).map_err(cubtera_app::AppError::from)?,
        selector: match params.selector.as_deref() {
            Some(s) => Selector::parse(s).map_err(cubtera_app::AppError::Model)?,
            None => Selector::All,
        },
        exclude: Vec::new(),
        wave: 0,
    };
    let exclude: Vec<DimRef> = params
        .exclude
        .iter()
        .flat_map(|v| v.split(','))
        .filter(|s| !s.is_empty())
        .map(DimRef::parse)
        .collect::<Result<_, _>>()
        .map_err(cubtera_app::AppError::from)?;

    let binding_uc = build_binding_use_case(&state.config)?;
    let report = binding_uc.status(&org, &binding).await?;

    let items: Vec<_> = report
        .into_iter()
        .filter(|d| !excluded_by_dim_ref(&d.id, &exclude))
        .map(|d| {
            json!({
                "instance": d.id.canonical(),
                "state": format!("{:?}", d.state),
            })
        })
        .collect();
    Ok(Json(json!(items)))
}

fn excluded_by_dim_ref(id: &InstanceId, exclude: &[DimRef]) -> bool {
    exclude.iter().any(|ex| id.all_refs().any(|r| r == ex))
}
