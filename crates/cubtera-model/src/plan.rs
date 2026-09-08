use crate::ids::PlanId;
use crate::revision::Revision;
use cubtera_kernel::{Digest, InstanceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The real bill-of-materials for one `Plan`/`Run`: every input that could
/// change the outcome of re-running the same command, hashed or pinned so
/// `apply --plan <id>` can refuse to proceed if any of them drifted between
/// `plan` and `apply` (ยง"Plan as artifact"). v2 had nothing like this - a
/// `terraform apply` could silently pick up a module that changed on disk,
/// a different inventory revision, or a different runner version than the
/// plan was computed against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionManifest {
    /// `UnitPackage::content_hash` at plan time.
    pub package_digest: Digest,
    /// Pinned module name -> its `PinnedModule::content_hash`.
    pub module_digests: BTreeMap<String, Digest>,
    /// Inventory revision the dimensions were read at (e.g. a git commit
    /// SHA from `cubtera-source::GitSource::revision`, or a content-hash
    /// snapshot id from `FsSource`).
    pub inventory_revision: String,
    /// `DimRef::key()` -> that dimension's `Dimension::content_hash` at
    /// plan time, for every dimension actually consumed.
    pub inventory_digests: BTreeMap<String, Digest>,
    /// Hash of the effective (merged, gap-filled) `config.toml` used.
    pub config_digest: Digest,
    /// Input alias -> the producer `OutputSet::revision` it was resolved
    /// against, so a plan can be checked against "did any input move".
    pub consumed_inputs: BTreeMap<String, Revision>,
    /// Runner binary + version actually resolved (e.g. `"tofu 1.9.0"`).
    pub runner_version: String,
    /// Hash of the provider lockfile, if the runner has one.
    pub provider_lock_digest: Option<Digest>,
}

impl ResolutionManifest {
    /// A manifest with no pins recorded yet - callers fill in fields as
    /// each resolution step completes.
    pub fn empty(
        package_digest: Digest,
        inventory_revision: String,
        config_digest: Digest,
    ) -> Self {
        Self {
            package_digest,
            module_digests: BTreeMap::new(),
            inventory_revision,
            inventory_digests: BTreeMap::new(),
            config_digest,
            consumed_inputs: BTreeMap::new(),
            runner_version: String::new(),
            provider_lock_digest: None,
        }
    }
}

/// A reviewed, storable artifact of `plan`: the exact set of pins `apply
/// --plan <id>` must still match before it's allowed to proceed (the
/// approval gate for CI/PR the spec calls out - v2 had no `Plan` object at
/// all, so `apply` could never be gated on a previously reviewed plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub instance: InstanceId,
    pub resolution: ResolutionManifest,
    /// Digest of the plan artifact bytes (e.g. `terraform show -json`
    /// output), retrievable via `Store::get_artifact`.
    pub artifact_digest: Digest,
    pub diff_summary: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Plan {
    /// `true` once `now >= expires_at` - `apply --plan` must re-plan rather
    /// than trust a stale artifact.
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    /// The check `apply --plan <id>` performs before proceeding: the
    /// resolution pins recorded when this plan was created must still
    /// match what a fresh resolution would produce right now.
    pub fn pins_match(&self, current: &ResolutionManifest) -> bool {
        &self.resolution == current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlanId;

    fn manifest() -> ResolutionManifest {
        ResolutionManifest::empty(Digest::of(b"pkg"), "deadbeef".into(), Digest::of(b"cfg"))
    }

    fn plan() -> Plan {
        Plan {
            id: PlanId::new("p1"),
            instance: crate::test_support::instance("cubtera", "network", &["dome:prod"]),
            resolution: manifest(),
            artifact_digest: Digest::of(b"artifact"),
            diff_summary: "1 to add".into(),
            created_at: 1000,
            expires_at: 2000,
        }
    }

    #[test]
    fn expiry_is_inclusive_of_the_boundary() {
        let p = plan();
        assert!(!p.is_expired(1999));
        assert!(p.is_expired(2000));
        assert!(p.is_expired(2001));
    }

    #[test]
    fn pins_match_detects_drift() {
        let p = plan();
        assert!(p.pins_match(&manifest()));

        let mut drifted = manifest();
        drifted.runner_version = "tofu 1.9.1".into();
        assert!(!p.pins_match(&drifted));
    }
}
