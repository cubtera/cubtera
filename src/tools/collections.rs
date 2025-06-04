use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Checks if two vectors have any common elements
/// 
/// # Arguments
/// * `vec1` - First vector
/// * `vec2` - Second vector
/// 
/// # Returns
/// * `bool` - true if vectors intersect, false otherwise
/// 
/// # Examples
/// ```
/// use cubtera::tools::collections::if_intersect;
/// 
/// let vec1 = vec!["a".to_string(), "b".to_string()];
/// let vec2 = vec!["b".to_string(), "c".to_string()];
/// assert!(if_intersect(vec1, vec2));
/// ```
pub fn if_intersect(vec1: Vec<String>, vec2: Vec<String>) -> bool {
    let set1: HashSet<String> = vec1.into_iter().collect();
    let set2: HashSet<String> = vec2.into_iter().collect();
    let intersect: HashSet<String> = set1.intersection(&set2).cloned().collect();
    !intersect.is_empty()
}

/// Finds intersection of two JSON array values
/// 
/// # Arguments
/// * `value1` - First JSON value (should be array)
/// * `value2` - Second JSON value (should be array)
/// 
/// # Returns
/// * `Option<HashSet<String>>` - Intersection set or None if no intersection
/// 
/// # Examples
/// ```
/// use cubtera::tools::collections::value_intersection;
/// use serde_json::json;
/// 
/// let val1 = json!(["a", "b", "c"]);
/// let val2 = json!(["b", "c", "d"]);
/// let intersection = value_intersection(val1, val2);
/// ```
pub fn value_intersection(value1: Value, value2: Value) -> Option<HashSet<String>> {
    let vec1 = value_to_vec(&value1)?;
    let vec2 = value_to_vec(&value2)?;
    let set1: HashSet<_> = vec1.into_iter().collect();
    let set2: HashSet<_> = vec2.into_iter().collect();

    match set1.intersection(&set2).cloned().collect::<HashSet<_>>() {
        set if set.is_empty() => None,
        set => Some(set),
    }
}

/// Groups tuples by key
/// 
/// # Arguments
/// * `tuples` - Vector of key-value tuples
/// 
/// # Returns
/// * `HashMap<String, Vec<String>>` - Grouped values by key
/// 
/// # Examples
/// ```
/// use cubtera::tools::collections::group_tuples;
/// 
/// let tuples = vec![
///     ("key1".to_string(), "value1".to_string()),
///     ("key1".to_string(), "value2".to_string()),
///     ("key2".to_string(), "value3".to_string()),
/// ];
/// let grouped = group_tuples(tuples);
/// ```
pub fn group_tuples(tuples: Vec<(String, String)>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    tuples.into_iter().for_each(|(key, value)| {
        map.entry(key).or_default().push(value);
    });
    map
}

/// Converts a JSON value to a vector of strings
/// 
/// # Arguments
/// * `value` - JSON value to convert
/// 
/// # Returns
/// * `Option<Vec<String>>` - Vector of strings or None if conversion fails
fn value_to_vec(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
}

// TODO: Add more collection operations as needed
// - merge_hashmaps
// - deduplicate_vec
// - sort_by_key
// - filter_by_predicate
// etc. 