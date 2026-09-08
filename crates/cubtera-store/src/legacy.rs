use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
