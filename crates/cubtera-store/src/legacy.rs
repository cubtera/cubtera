use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A v2-shaped deployment-log row, kept only so `cubtera-persistence`'s
/// `DeploymentLogRepository` adapter can share this store's SQLite file
/// during the P2-P7 migration window, without `cubtera-store` depending on
/// `cubtera-domain`/`cubtera-core` (that would invert the dependency rule -
/// adapters for v2 ports belong in a crate that depends on v2's core, not
/// the other way around).
///
/// The v2 port's `dimensions` field is a full resolved ancestor chain (e.g.
/// `["dome:prod", "env:prod", "dc:use1"]`), not an `InstanceId`'s required
/// dims - it doesn't map onto [`cubtera_model::Run`]/[`cubtera_kernel::InstanceId`]
/// without losing the ancestor entries `-q dome:prod` queries rely on. Hence
/// its own small table instead of a forced, lossy fit. Retired in P7
/// alongside the rest of v2's ports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyDeploymentLogRow {
    pub org: String,
    pub unit_name: String,
    pub dimensions: Vec<String>,
    pub command: String,
    pub exit_code: i32,
    pub timestamp: i64,
    pub duration_ms: u64,
    pub git_shas: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl LegacyDeploymentLogRow {
    /// Whether this row satisfies every `key:value` pair in `query` -
    /// ported verbatim from v2's `cubtera_core::ports::deployment_log::entry_matches`
    /// so `cubtera log get`/`GET /v1/{org}/dlog` keep exactly the same
    /// query semantics without a `cubtera-core` dependency: `unit`/
    /// `unit_name` and `command` match the corresponding field exactly;
    /// everything else is treated as a dimension type and checked against
    /// `dimensions` (`type:name` strings).
    pub fn matches(&self, query: &HashMap<String, String>) -> bool {
        query.iter().all(|(key, value)| match key.as_str() {
            "unit" | "unit_name" => self.unit_name == *value,
            "command" => self.command == *value,
            "exit_code" => self.exit_code.to_string() == *value,
            _ => self.dimensions.contains(&format!("{key}:{value}")),
        })
    }
}

/// A v2-shaped unit-state row - see [`LegacyDeploymentLogRow`] for why this
/// doesn't reuse `OutputSet`/`InstanceId` either.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyUnitStateRow {
    pub org: String,
    pub unit: String,
    pub dims: Vec<String>,
    pub ext: Vec<String>,
    pub outputs: serde_json::Value,
    pub updated_at: i64,
}

impl LegacyUnitStateRow {
    /// A stable lookup key: sorted `dims`/`ext` joined the same way
    /// `cubtera_domain::UnitStateKey::canonical` does, so two logically
    /// identical keys built in a different order still land on the same
    /// row.
    pub fn state_key(org: &str, unit: &str, dims: &[String], ext: &[String]) -> String {
        let mut dims = dims.to_vec();
        dims.sort();
        let mut ext = ext.to_vec();
        ext.sort();
        let mut key = format!("{org}/{unit}@{}", dims.join(","));
        if !ext.is_empty() {
            key.push('#');
            key.push_str(&ext.join(","));
        }
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_row() -> LegacyDeploymentLogRow {
        LegacyDeploymentLogRow {
            org: "cubtera".to_string(),
            unit_name: "network".to_string(),
            dimensions: vec!["dome:prod".to_string(), "env:prod".to_string()],
            command: "apply".to_string(),
            exit_code: 0,
            timestamp: 1_700_000_000,
            duration_ms: 1234,
            git_shas: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn matches_by_unit_name() {
        let mut query = HashMap::new();
        query.insert("unit".to_string(), "network".to_string());
        assert!(log_row().matches(&query));

        query.insert("unit".to_string(), "other".to_string());
        assert!(!log_row().matches(&query));
    }

    #[test]
    fn matches_by_dimension() {
        let mut query = HashMap::new();
        query.insert("env".to_string(), "prod".to_string());
        assert!(log_row().matches(&query));

        query.insert("env".to_string(), "staging".to_string());
        assert!(!log_row().matches(&query));
    }

    #[test]
    fn matches_requires_every_query_key() {
        let mut query = HashMap::new();
        query.insert("unit".to_string(), "network".to_string());
        query.insert("command".to_string(), "destroy".to_string());
        assert!(!log_row().matches(&query));
    }

    #[test]
    fn state_key_is_order_independent() {
        let a = LegacyUnitStateRow::state_key(
            "cubtera",
            "network",
            &["dome:prod".into(), "env:prod".into()],
            &[],
        );
        let b = LegacyUnitStateRow::state_key(
            "cubtera",
            "network",
            &["env:prod".into(), "dome:prod".into()],
            &[],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn state_key_includes_extensions() {
        let key = LegacyUnitStateRow::state_key(
            "cubtera",
            "network",
            &["dome:prod".into()],
            &["index:0".into()],
        );
        assert_eq!(key, "cubtera/network@dome:prod#index:0");
    }
}
