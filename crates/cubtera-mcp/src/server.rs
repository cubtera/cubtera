//! MCP tool surface
//!
//! Read-only queries over the same `cubtera-core` services the CLI and REST
//! API use (`DimensionService`, `UnitService`, `DeploymentLogRepository`) -
//! so results get the same defaults gap-fill/parent-chain/access-policy
//! behavior everywhere. Deliberately no `run`/write tools: giving an MCP
//! client (typically an LLM) the ability to apply infrastructure changes is
//! a product decision, not something this migration bundles in by default.

use crate::error::to_mcp_error;
use cubtera_core::ports::{DeploymentLogRepository, UnitStateRepository};
use cubtera_core::services::{DimensionService, SchemaValidation, UnitService};
use cubtera_domain::UnitStateKey;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrgParams {
    /// Organization name
    pub org: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DimTypeParams {
    /// Organization name
    pub org: String,
    /// Dimension type (e.g. "env", "dc")
    pub dim_type: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DimensionParams {
    /// Organization name
    pub org: String,
    /// Dimension type (e.g. "env", "dc")
    pub dim_type: String,
    /// Dimension name (e.g. "prod")
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnitParams {
    /// Organization name
    pub org: String,
    /// Unit name
    pub unit_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnitStateParams {
    /// Organization name
    pub org: String,
    /// Producer unit name
    pub unit_name: String,
    /// `type:name` dimensions the producer ran with - must match exactly
    /// what it published, not the caller's full ancestor chain
    #[serde(default)]
    pub dims: Vec<String>,
    /// `type:name` extensions the producer ran with, if any
    #[serde(default)]
    pub ext: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeploymentLogParams {
    /// Organization name
    pub org: String,
    /// Filters: `unit`/`unit_name` and `command` match the entry's fields
    /// exactly; any other key (e.g. `env`) is matched against the
    /// dimensions the run was against (e.g. `{"env": "prod"}` matches runs
    /// against `env:prod`).
    #[serde(default)]
    pub query: HashMap<String, String>,
    /// Maximum number of entries to return, most recent first (default 10)
    #[serde(default)]
    pub limit: Option<usize>,
}

fn json_result(value: Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&value).map_err(|e| {
        McpError::internal_error(format!("failed to serialize response: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

#[derive(Clone)]
pub struct CubteraMcp {
    dimensions: Arc<DimensionService>,
    units: Arc<UnitService>,
    deployment_log: Arc<dyn DeploymentLogRepository>,
    unit_state: Arc<dyn UnitStateRepository>,
    // Read by the `#[tool_handler]`-generated `ServerHandler` methods below;
    // rustc's dead-code pass doesn't see through that macro.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CubteraMcp {
    pub fn new(
        dimensions: Arc<DimensionService>,
        units: Arc<UnitService>,
        deployment_log: Arc<dyn DeploymentLogRepository>,
        unit_state: Arc<dyn UnitStateRepository>,
    ) -> Self {
        Self {
            dimensions,
            units,
            deployment_log,
            unit_state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List all organizations known to the inventory")]
    async fn list_orgs(&self) -> Result<CallToolResult, McpError> {
        let orgs = self.dimensions.get_orgs().await.map_err(to_mcp_error)?;
        json_result(serde_json::json!(orgs))
    }

    #[tool(description = "List all dimension types defined for an organization (e.g. env, dc)")]
    async fn list_dim_types(
        &self,
        Parameters(params): Parameters<OrgParams>,
    ) -> Result<CallToolResult, McpError> {
        let types = self
            .dimensions
            .get_types(&params.org)
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(types))
    }

    #[tool(description = "List all dimension names of a given type (e.g. all 'dc' names)")]
    async fn list_dimension_names(
        &self,
        Parameters(params): Parameters<DimTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        let names = self
            .dimensions
            .get_all_names(&params.org, &params.dim_type)
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(names))
    }

    #[tool(
        description = "Get a dimension fully assembled: defaults gap-filled, parent chain resolved, children listed"
    )]
    async fn get_dimension(
        &self,
        Parameters(params): Parameters<DimensionParams>,
    ) -> Result<CallToolResult, McpError> {
        let dim = self
            .dimensions
            .get_by_name(&params.org, &params.dim_type, &params.name)
            .await
            .map_err(to_mcp_error)?;
        json_result(dim.to_response_json())
    }

    #[tool(description = "Get the default dimension data for a type, if any is defined")]
    async fn get_dimension_defaults(
        &self,
        Parameters(params): Parameters<DimTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        let dim = self
            .dimensions
            .get_defaults(&params.org, &params.dim_type)
            .await
            .map_err(to_mcp_error)?;
        json_result(dim.map(|d| d.to_response_json()).unwrap_or(Value::Null))
    }

    #[tool(
        description = "Get the JSON-schema for a dimension type's meta section (.schema:meta.json), if any is defined"
    )]
    async fn get_dimension_schema(
        &self,
        Parameters(params): Parameters<DimTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        let schema = self
            .dimensions
            .get_schema(&params.org, &params.dim_type)
            .await
            .map_err(to_mcp_error)?;
        json_result(schema.unwrap_or(Value::Null))
    }

    #[tool(description = "Get the parent of a dimension, following meta.parent, if any")]
    async fn get_dimension_parent(
        &self,
        Parameters(params): Parameters<DimensionParams>,
    ) -> Result<CallToolResult, McpError> {
        let parent = self
            .dimensions
            .get_parent(&params.org, &params.dim_type, &params.name)
            .await
            .map_err(to_mcp_error)?;
        json_result(parent.map(|d| d.to_response_json()).unwrap_or(Value::Null))
    }

    #[tool(
        description = "Get the direct children of a dimension, per the configured dimension hierarchy"
    )]
    async fn get_dimension_children(
        &self,
        Parameters(params): Parameters<DimensionParams>,
    ) -> Result<CallToolResult, McpError> {
        let children = self
            .dimensions
            .get_children(&params.org, &params.dim_type, &params.name)
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(children
            .iter()
            .map(cubtera_domain::Dimension::to_response_json)
            .collect::<Vec<_>>()))
    }

    #[tool(
        description = "Validate a dimension: confirms it exists, and (if the type has a .schema:meta.json) that its meta section satisfies that schema"
    )]
    async fn validate_dimension(
        &self,
        Parameters(params): Parameters<DimensionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .dimensions
            .validate_schema(&params.org, &params.dim_type, &params.name)
            .await
            .map_err(to_mcp_error)?;
        let (valid, errors) = match result {
            SchemaValidation::NoSchema | SchemaValidation::Valid => (true, Vec::new()),
            SchemaValidation::Invalid(errors) => (false, errors),
        };
        json_result(serde_json::json!({ "valid": valid, "errors": errors }))
    }

    #[tool(description = "List all units defined for an organization")]
    async fn list_units(
        &self,
        Parameters(params): Parameters<OrgParams>,
    ) -> Result<CallToolResult, McpError> {
        let units = self
            .units
            .list_units(&params.org)
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(units))
    }

    #[tool(description = "Get a unit's manifest (dimensions, runner type, allow/deny lists, etc.)")]
    async fn get_unit_manifest(
        &self,
        Parameters(params): Parameters<UnitParams>,
    ) -> Result<CallToolResult, McpError> {
        let manifest = self
            .units
            .get_manifest(&params.org, &params.unit_name)
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(manifest))
    }

    #[tool(
        description = "Get a producer unit's published outputs ([outputs] publish = true) for an exact dims/ext key - not a consumer's [inputs] projection, which only happens inside `cubtera run`"
    )]
    async fn get_unit_state(
        &self,
        Parameters(params): Parameters<UnitStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let key = UnitStateKey::new(params.org, params.unit_name, params.dims, params.ext);
        let record = self.unit_state.get(&key).await.map_err(to_mcp_error)?;
        json_result(serde_json::json!(record))
    }

    #[tool(
        description = "Query the deployment log: past `run` invocations, their exit code, duration and dimensions"
    )]
    async fn get_deployment_log(
        &self,
        Parameters(params): Parameters<DeploymentLogParams>,
    ) -> Result<CallToolResult, McpError> {
        let entries = self
            .deployment_log
            .find(&params.org, &params.query, params.limit.or(Some(10)))
            .await
            .map_err(to_mcp_error)?;
        json_result(serde_json::json!(entries))
    }
}

#[tool_handler]
impl ServerHandler for CubteraMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Read-only access to Cubtera's inventory (organizations, dimension types, \
                 dimensions with defaults/parent-chain resolved), unit manifests, and the \
                 deployment log. No tool here executes infrastructure changes."
                    .to_string(),
            )
    }
}
