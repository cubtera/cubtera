//! Simple JSON-like value for domain layer
//!
//! This type avoids serde dependency in the domain layer.
//! It can be converted to/from serde_json::Value at the infrastructure boundary.

use std::collections::HashMap;

/// A simple JSON-like value type for the domain layer
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Numeric value (stored as f64)
    Number(f64),
    /// String value
    String(String),
    /// Array of values
    Array(Vec<Value>),
    /// Object (map of string to value)
    Object(HashMap<String, Value>),
}

impl Value {
    /// Check if this value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get this value as a string slice
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get this value as a boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get this value as a number
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get this value as an integer
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => Some(*n as i64),
            _ => None,
        }
    }

    /// Try to get this value as an object
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Try to get this value as a mutable object
    pub fn as_object_mut(&mut self) -> Option<&mut HashMap<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Try to get this value as an array
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Get a value by key (for objects)
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n as f64)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(m: HashMap<String, Value>) -> Self {
        Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_string() {
        let v = Value::from("hello");
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_value_bool() {
        let v = Value::from(true);
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn test_value_number() {
        let v = Value::from(42i64);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_value_object() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::from("value"));
        let v = Value::from(map);

        assert!(v.as_object().is_some());
        assert_eq!(v.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_value_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::from("test").is_null());
    }
}

