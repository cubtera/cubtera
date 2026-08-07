//! JSON-schema validation for inventory records
//!
//! Pure function: given already-loaded `data`/`schema` JSON, validate one
//! against the other. Fetching `.schema:meta.json` off disk (or Mongo, in
//! wave 2) is an [`crate::RawDimension`]/`InventoryRepository` concern -
//! this module has no I/O of its own, same rationale as `render_value` in
//! `runner.rs` for handlebars.

use serde_json::Value;

/// Validate `data` against a JSON schema. On success, returns `Ok(())`; on
/// failure, returns every violation as a human-readable string (instance
/// path + reason) rather than stopping at the first one, since inventory
/// authors want the full picture in one pass.
pub fn validate_against_schema(data: &Value, schema: &Value) -> Result<(), Vec<String>> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("Invalid schema: {e}")]),
    };

    let errors: Vec<String> = validator
        .iter_errors(data)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_data_passes() {
        let schema = json!({
            "type": "object",
            "required": ["region"],
            "properties": { "region": { "type": "string" } }
        });
        let data = json!({"region": "us-east-1"});
        assert_eq!(validate_against_schema(&data, &schema), Ok(()));
    }

    #[test]
    fn missing_required_field_is_reported() {
        let schema = json!({
            "type": "object",
            "required": ["region"]
        });
        let data = json!({"name": "prod"});
        let errors = validate_against_schema(&data, &schema).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("region"));
    }

    #[test]
    fn wrong_type_is_reported() {
        let schema = json!({
            "type": "object",
            "properties": { "region": { "type": "string" } }
        });
        let data = json!({"region": 42});
        let errors = validate_against_schema(&data, &schema).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("region"));
    }

    #[test]
    fn malformed_schema_is_reported_without_panicking() {
        let schema = json!({"type": "not-a-real-type"});
        let data = json!({});
        let errors = validate_against_schema(&data, &schema).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Invalid schema"));
    }
}
