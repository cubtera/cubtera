use rocket::response::content;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use serde_json::{json, Value};
use cubtera::prelude::data::Storage;
use cubtera::core::im::*;

// MCP Function manifest structure
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionManifest {
    name: String,
    description: String,
    parameters: Value,
    returns: Value,
}


// Store for function manifests
pub struct McpRegistry {
    functions: HashMap<String, FunctionManifest>
}

impl McpRegistry {
    fn new() -> Self {
        let mut registry = McpRegistry { functions: HashMap::new() };
        
        // Register API functions here
        registry.register_function(
            "get_orgs",
            "Retrieves all existing organizations for the given inventory",
            json!({}),
            json!({
                "type": "object",
                "properties": {
                    "org": {"type": "string"},
                    "data": {"type": "list"},
                }
            })
        );

        registry.register_function(
            "get_dim_types",
            "Retrieves all existing dimensions for the given organisation",
            json!({
                "org": {"type": "string", "description": "Unique organization name"}
            }),
            json!({
                "type": "object",
                "properties": {
                    "org": {"type": "string"},
                    "data": {"type": "list"},
                }
            })
        );

        registry.register_function(
            "get_dim_names_by_type",
            "Retrieves all dimension names for the given dimension type",
            json!({
                "org": {"type": "string", "description": "Unique organization name"},
                "type": {"type": "string", "description": "Dimension type"}
            }),
            json!({
                "type": "object",
                "properties": {
                    "org": {"type": "string"},
                    "type": {"type": "string"},
                    "data": {"type": "list"},
                }
            })
        );

        registry
    }

    fn register_function(&mut self, name: &str, description: &str,
                         parameters: serde_json::Value, returns: serde_json::Value) {
        self.functions.insert(name.to_string(), FunctionManifest {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            returns,
        });
    }

    fn get_manifest(&self, function_name: &str) -> Option<&FunctionManifest> {
        self.functions.get(function_name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }
}

// List all MCP functions
#[rocket::get("/functions")]
pub async fn list_mcp_functions() -> Value {
    let registry = McpRegistry::new();
    let functions = registry.list_functions();
    functions.into()
}

// Return schema for specific function
#[rocket::get("/functions/<name>")]
pub async fn get_function_manifest(name: String) -> Value {
    let registry = McpRegistry::new();
    let manifest = registry.get_manifest(&name);
    manifest.map(|m| m.returns.clone()).unwrap_or_default()
}


// Execute function with parameters
#[rocket::post("/invoke/<name>", data = "<params>", format = "json")]
pub async fn invoke_function(name: String, params: String) -> Result<Value, String> {
    let registry = McpRegistry::new();
    let manifest = registry.get_manifest(&name);
    
    let json_params = serde_json::from_str::<Value>(&params).unwrap_or(json!({}));
    // dbg!(json_params);
    
    if let Some(manifest) = manifest {
        // let params = serde_json::from_str::<Value>(&params).unwrap();

        match name.as_str() {
            "get_orgs" => {
               Ok(get_all_orgs(&Storage::DB))
            },
            "get_dim_types" => {
                let org = json_params["org"].as_str().unwrap_or("cubtera");
                Ok(get_all_dim_types(org, &Storage::DB))
            },
            "get_dim_names_by_type" => {
                let org = json_params["org"].as_str().unwrap_or("cubtera");
                let dim_type = json_params["type"].as_str().unwrap_or("unknown");
                Ok(get_dim_names_by_type(dim_type, org, &Storage::DB))
            },  
            _ => Err("Function not found".to_string())
        }
    } else {
        Err("Function not found".to_string())
    }
    // manifest.map(|m| m.returns.clone()).unwrap_or_default()
}

// Return MCP version information
#[rocket::get("/version")]
pub async fn get_mcp_version() -> Value {
    // Return MCP version information
    serde_json::json!({
        "version": "1.0",
        "protocol": "MCP/1.0"
    })
}


#[rocket::get("/docs")]
pub async fn get_documentation() -> content::RawJson<String> {
    // Return API documentation
    content::RawJson(serde_json::json!({
        "version": "1.0",
        "protocol": "MCP/1.0"
    }).to_string())
}