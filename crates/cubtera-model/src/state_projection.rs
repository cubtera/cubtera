//! `project_state_key`: v3 port of `cubtera_domain::unit_state::project_state_key`
//! (carried over as-is per the architecture doc's "what we keep" list) -
//! project a consumer's own resolved dimension chain onto a producer's
//! required dimension types, for `[inputs.<alias>]` resolution when the
//! manifest doesn't name the producer's dimensions explicitly.
//!
//! Deliberately not a DAG and never a guess: a producer dimension type
//! the consumer never resolved, or an ambiguous match (two dimensions of
//! the same type in the consumer's chain - can't happen today since
//! `InstanceId` rejects duplicate types, but this function doesn't assume
//! that of its caller), is a hard [`ModelError`], never a silent fallback.

use crate::error::ModelError;
use std::collections::HashSet;

/// `consumer_key_path`/`producer_dims` are both `"type:name"` strings (the
/// same shape `InstanceId::all_refs`/`DimRef::key` produce) - kept as
/// plain strings rather than `DimRef` so this function has zero
/// `cubtera-kernel` dependency beyond what the caller already has parsed.
pub fn project_state_key(
    consumer_key_path: &[String],
    producer_dims: &[String],
) -> Result<Vec<String>, ModelError> {
    let mut projected = Vec::with_capacity(producer_dims.len());
    for dim_type in producer_dims {
        let prefix = format!("{dim_type}:");
        let matches: Vec<&String> = consumer_key_path
            .iter()
            .filter(|k| k.starts_with(&prefix))
            .collect();

        let unique: HashSet<&String> = matches.iter().copied().collect();
        match unique.len() {
            0 => {
                return Err(ModelError::InputResolution(format!(
                    "cannot resolve unit state: producer requires dimension type \
                     '{dim_type}', which is not present in the consumer's resolved \
                     dimension chain {consumer_key_path:?}"
                )));
            }
            1 => projected.push((*matches[0]).clone()),
            _ => {
                return Err(ModelError::InputResolution(format!(
                    "cannot resolve unit state: ambiguous dimension type '{dim_type}' in \
                     consumer's resolved chain: {matches:?}"
                )));
            }
        }
    }
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn projects_ancestor_type_from_deeper_consumer_chain() {
        let consumer = strings(&["dome:prod", "env:prod", "dc:prod-use1"]);
        let producer_dims = strings(&["env"]);
        assert_eq!(
            project_state_key(&consumer, &producer_dims).unwrap(),
            vec!["env:prod".to_string()]
        );
    }

    #[test]
    fn preserves_producer_dim_order() {
        let consumer = strings(&["dome:prod", "env:prod", "dc:prod-use1"]);
        let producer_dims = strings(&["dc", "dome"]);
        assert_eq!(
            project_state_key(&consumer, &producer_dims).unwrap(),
            vec!["dc:prod-use1".to_string(), "dome:prod".to_string()]
        );
    }

    #[test]
    fn errors_when_producer_dim_type_missing_from_consumer_chain() {
        let consumer = strings(&["dome:prod", "env:prod"]);
        let producer_dims = strings(&["dc"]);
        let err = project_state_key(&consumer, &producer_dims).unwrap_err();
        assert!(matches!(err, ModelError::InputResolution(_)));
    }

    #[test]
    fn errors_on_ambiguous_dimension_type() {
        let consumer = strings(&["dome:prod", "dome:other"]);
        let producer_dims = strings(&["dome"]);
        let err = project_state_key(&consumer, &producer_dims).unwrap_err();
        assert!(matches!(err, ModelError::InputResolution(_)));
    }
}
