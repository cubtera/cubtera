use crate::tools::error::{Result, ToolsError};
use serde_json::Value;
use std::path::PathBuf;

/// Reads a JSON file and returns its content as a Value
/// 
/// # Arguments
/// * `path` - Path to the JSON file
/// 
/// # Returns
/// * `Result<Value>` - JSON content or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::json::read_json_file;
/// use std::path::PathBuf;
/// 
/// let json_data = read_json_file(&PathBuf::from("config.json"))?;
/// ```
pub fn read_json_file(path: &PathBuf) -> Result<Value> {
    if !path.exists() {
        return Err(ToolsError::file_not_found(path.to_string_lossy().to_string()));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to read file {:?}: {}", path, e)))?;

    let json_data: Value = serde_json::from_str(&content)
        .map_err(|e| ToolsError::validation_error(format!("Invalid JSON in file {:?}: {}", path, e)))?;

    Ok(json_data)
}

/// Merges two JSON values recursively
/// 
/// # Arguments
/// * `target` - Target JSON value to merge into
/// * `source` - Source JSON value to merge from
/// 
/// # Examples
/// ```
/// use cubtera::tools::json::merge_values;
/// use serde_json::json;
/// 
/// let mut target = json!({"a": 1});
/// let source = json!({"b": 2});
/// merge_values(&mut target, &source);
/// ```
pub fn merge_values(target: &mut Value, source: &Value) {
    if let (Value::Object(target_obj), Value::Object(source_obj)) = (target, source) {
        for (key, source_value) in source_obj {
            if let serde_json::map::Entry::Vacant(entry) = target_obj.entry(key) {
                entry.insert(source_value.clone());
            } else if let serde_json::map::Entry::Occupied(mut entry) = target_obj.entry(key) {
                if source_value.is_object() && entry.get().is_object() {
                    merge_values(entry.get_mut(), source_value);
                }
            }
        }
    }
}

/// Validates a JSON value against a JSON schema
/// 
/// # Arguments
/// * `json` - JSON value to validate
/// * `schema` - JSON schema to validate against
/// 
/// # Returns
/// * `Result<Value>` - Validated JSON or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::json::validate_json_by_schema;
/// use serde_json::json;
/// 
/// let json_data = json!({"name": "John", "age": 30});
/// let schema = json!({"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "number"}}});
/// let validated = validate_json_by_schema(&json_data, &schema)?;
/// ```
pub fn validate_json_by_schema(json: &Value, schema: &Value) -> Result<Value> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| ToolsError::validation_error(format!("Invalid schema: {}", e)))?;

    validator.validate(json)
        .map_err(|e| ToolsError::validation_error(format!("Validation failed: {}", e)))?;

    Ok(json.clone())
}

// TODO: Add more JSON operations as needed
// - read_and_validate_json
// - write_json_file
// - pretty_print_json
// - json_path operations
// etc. 