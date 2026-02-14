use crate::tools::error::{Result, ToolsError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Generates SHA256 hash for a JSON value with deterministic ordering
/// 
/// # Arguments
/// * `value` - JSON value to hash
/// 
/// # Returns
/// * `Result<String>` - Hex-encoded SHA256 hash or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::crypto::get_sha_by_value;
/// use serde_json::json;
/// 
/// let value = json!({"name": "test", "value": 42});
/// let hash = get_sha_by_value(&value)?;
/// ```
pub fn get_sha_by_value(value: &Value) -> Result<String> {
    let mut hasher = Sha256::new();
    let ordered = order_json(value);
    let canonical_json = serde_json::to_string(&ordered)
        .map_err(|e| ToolsError::crypto_error(format!("Failed to serialize JSON: {}", e)))?;
    
    hasher.update(canonical_json.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

/// Orders JSON recursively for deterministic hashing
/// 
/// # Arguments
/// * `value` - JSON value to order
/// 
/// # Returns
/// * `Value` - Ordered JSON value
fn order_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map
                .iter()
                .map(|(k, v)| (k.clone(), order_json(v)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(arr) => {
            let mut ordered: Vec<Value> = arr.iter().map(order_json).collect();
            if ordered.iter().all(|v| v.is_number() || v.is_string()) {
                ordered.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            }
            Value::Array(ordered)
        }
        _ => value.clone(),
    }
}

// TODO: Add more crypto operations as needed
// - hash_file
// - hash_string
// - generate_random_string
// - encrypt/decrypt operations
// etc. 