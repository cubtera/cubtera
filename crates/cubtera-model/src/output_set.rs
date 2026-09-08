use crate::ids::RunId;
use crate::revision::Revision;
use cubtera_kernel::Digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An opaque pointer to a secret (e.g. `"vault://path#field"`), never a
/// resolved value. `OutputSet` stores this for any key named in the
/// producer's `[outputs] sensitive = [...]` - the resolved value only ever
/// exists inside a running process's environment, supplied by
/// `cubtera-identity` at execution time (ยง8, closing v2's H2: sensitive
/// Terraform outputs persisted in plaintext in the state-mesh JSON file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef(pub String);

/// One value in an [`OutputSet`] - either a plain, storable JSON value, or
/// a [`SecretRef`] that must be resolved through `cubtera-identity` before
/// use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputValue {
    Plain(serde_json::Value),
    Secret(SecretRef),
}

/// State-mesh v2's producer-side record (ยง5.5): versioned, schema-checked,
/// and never storing a sensitive value in the clear. `revision` is assigned
/// by `Store::put_output_set` (monotonic per `InstanceId`) - the thing a
/// consumer's `mark_consumed`/`state ls --stale` compares against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSet {
    pub schema_version: semver::Version,
    pub values: BTreeMap<String, OutputValue>,
    pub produced_by: RunId,
    /// The producer's `UnitPackage::content_hash` at publish time - lets a
    /// consumer (or a human) tell "this output set was produced by exactly
    /// this unit package" without trusting the `Run` row to still exist.
    pub source_hash: Digest,
    pub revision: Revision,
}

impl OutputSet {
    /// Whether a consumer's `expects` requirement (ยง5.5) is satisfied by
    /// this output set's `schema_version`.
    pub fn satisfies(&self, expects: &semver::VersionReq) -> bool {
        expects.matches(&self.schema_version)
    }
}

/// Returned by `Store::list_stale_consumers`: a consumer whose recorded
/// `consumed_revision` for a producer no longer matches that producer's
/// current `OutputSet::revision` - the backing data for `cubtera state ls
/// --stale`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleConsumer {
    pub consumer: cubtera_kernel::InstanceId,
    pub producer: cubtera_kernel::InstanceId,
    pub consumed_revision: Revision,
    pub current_revision: Revision,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satisfies_checks_semver_req() {
        let set = OutputSet {
            schema_version: semver::Version::parse("1.2.0").unwrap(),
            values: BTreeMap::new(),
            produced_by: RunId::new("r1"),
            source_hash: Digest::of(b"pkg"),
            revision: Revision::from_raw(3),
        };
        assert!(set.satisfies(&semver::VersionReq::parse("^1.0").unwrap()));
        assert!(!set.satisfies(&semver::VersionReq::parse("^2.0").unwrap()));
    }

    #[test]
    fn secret_values_never_carry_a_resolved_value() {
        let mut values = BTreeMap::new();
        values.insert(
            "kms_key_arn".to_string(),
            OutputValue::Secret(SecretRef("vault://prod/kms#arn".into())),
        );
        values.insert(
            "vpc_id".to_string(),
            OutputValue::Plain(serde_json::json!("vpc-123")),
        );

        let json = serde_json::to_string(&values).unwrap();
        assert!(json.contains("vault://prod/kms#arn"));
        assert!(!json.contains("resolved"));
    }
}
