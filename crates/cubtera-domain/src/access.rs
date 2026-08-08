//! Unit access policy
//!
//! v1 enforced `allowList`/`denyList`/`affinityTags` via `std::process::exit(0)`
//! buried inside `Unit::build()` - a domain-shaped decision made with
//! interface-layer control flow, and impossible to test without exiting the
//! test process. Here it's a pure function returning [`AccessDecision`]; the
//! caller (`UnitService::build_unit`) turns a `Denied` decision into an
//! `AppError`, and the CLI/API decide what that means (message + exit code,
//! or an HTTP status).
//!
//! Note on `affinityTags`: v1 only ran this check when the *first* resolved
//! dimension happened to carry a `meta.affinity_tags` key, which made the
//! gate accidental rather than declared. Here the check is opt-in: it only
//! applies when the manifest itself sets `affinityTags`, and then every
//! provided dimension must overlap it.

use crate::manifest::Manifest;
use std::collections::HashSet;

/// Outcome of evaluating a unit's access policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessDecision {
    /// The unit may proceed with the provided dimensions
    Allowed,
    /// The unit must not proceed; `reason` is safe to surface to the user
    Denied { reason: String },
}

impl AccessDecision {
    /// Convenience check
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// A single dimension's contribution to an access policy decision: its
/// "type:name" key plus its own `meta.affinity_tags` (empty if unset).
#[derive(Debug, Clone)]
pub struct DimensionAccessContext {
    /// "type:name" key of the dimension
    pub key: String,
    /// This dimension's own `meta.affinity_tags`, if any
    pub affinity_tags: Vec<String>,
}

/// Pure evaluator for a unit's `allowList`/`denyList`/`affinityTags` rules.
pub struct AccessPolicy;

impl AccessPolicy {
    /// Evaluate `manifest`'s policy against a unit's resolved dimensions.
    ///
    /// `dims_tree` is the full set of "type:name" keys across all resolved
    /// dimensions *and their ancestors* (i.e. every [`crate::Dimension::key_path`]
    /// entry, flattened) - matching v1, `allowList`/`denyList` can reference
    /// a parent dimension to gate an entire subtree.
    pub fn evaluate(
        manifest: &Manifest,
        dims_tree: &HashSet<String>,
        dims: &[DimensionAccessContext],
    ) -> AccessDecision {
        if let Some(allow_list) = &manifest.allow_list {
            if !allow_list.iter().any(|allowed| dims_tree.contains(allowed)) {
                return AccessDecision::Denied {
                    reason: format!(
                        "none of the provided dimensions {dims_tree:?} are in allowList {allow_list:?}"
                    ),
                };
            }
        }

        if let Some(deny_list) = &manifest.deny_list {
            if let Some(denied) = deny_list.iter().find(|d| dims_tree.contains(d.as_str())) {
                return AccessDecision::Denied {
                    reason: format!("dimension '{denied}' is in denyList {deny_list:?}"),
                };
            }
        }

        if let Some(unit_tags) = &manifest.affinity_tags {
            for dim in dims {
                if !dim.affinity_tags.iter().any(|tag| unit_tags.contains(tag)) {
                    return AccessDecision::Denied {
                        reason: format!(
                            "dimension '{}' has affinity tags {:?}, none of which are in the unit's affinityTags {:?}",
                            dim.key, dim.affinity_tags, unit_tags
                        ),
                    };
                }
            }
        }

        AccessDecision::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(
        allow: Option<Vec<&str>>,
        deny: Option<Vec<&str>>,
        affinity: Option<Vec<&str>>,
    ) -> Manifest {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.allow_list = allow.map(|v| v.into_iter().map(String::from).collect());
        manifest.deny_list = deny.map(|v| v.into_iter().map(String::from).collect());
        manifest.affinity_tags = affinity.map(|v| v.into_iter().map(String::from).collect());
        manifest
    }

    fn dims_tree(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    fn ctx(key: &str, tags: &[&str]) -> DimensionAccessContext {
        DimensionAccessContext {
            key: key.to_string(),
            affinity_tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn allowed_with_no_restrictions() {
        let manifest = manifest_with(None, None, None);
        let decision = AccessPolicy::evaluate(&manifest, &dims_tree(&["env:prod"]), &[]);
        assert_eq!(decision, AccessDecision::Allowed);
    }

    #[test]
    fn denied_when_not_in_allow_list() {
        let manifest = manifest_with(Some(vec!["env:staging"]), None, None);
        let decision = AccessPolicy::evaluate(&manifest, &dims_tree(&["env:prod"]), &[]);
        assert!(!decision.is_allowed());
    }

    #[test]
    fn allowed_when_ancestor_in_allow_list() {
        // allowList references a parent dome, not the leaf dc dimension
        let manifest = manifest_with(Some(vec!["dome:prod"]), None, None);
        let decision = AccessPolicy::evaluate(
            &manifest,
            &dims_tree(&["dome:prod", "env:prod", "dc:us-east-1"]),
            &[],
        );
        assert_eq!(decision, AccessDecision::Allowed);
    }

    #[test]
    fn denied_when_in_deny_list() {
        let manifest = manifest_with(None, Some(vec!["env:dev"]), None);
        let decision = AccessPolicy::evaluate(&manifest, &dims_tree(&["env:dev"]), &[]);
        assert!(!decision.is_allowed());
    }

    #[test]
    fn allow_list_wins_over_absence_of_deny() {
        let manifest = manifest_with(None, Some(vec!["env:dev"]), None);
        let decision = AccessPolicy::evaluate(&manifest, &dims_tree(&["env:prod"]), &[]);
        assert_eq!(decision, AccessDecision::Allowed);
    }

    #[test]
    fn denied_when_dimension_missing_required_affinity_tag() {
        let manifest = manifest_with(None, None, Some(vec!["critical"]));
        let decision = AccessPolicy::evaluate(
            &manifest,
            &dims_tree(&["env:prod"]),
            &[ctx("env:prod", &["core"])],
        );
        assert!(!decision.is_allowed());
    }

    #[test]
    fn allowed_when_affinity_tags_overlap() {
        let manifest = manifest_with(None, None, Some(vec!["critical", "core"]));
        let decision = AccessPolicy::evaluate(
            &manifest,
            &dims_tree(&["env:prod"]),
            &[ctx("env:prod", &["core"])],
        );
        assert_eq!(decision, AccessDecision::Allowed);
    }

    #[test]
    fn affinity_check_skipped_when_manifest_declares_no_tags() {
        // Dimension carries tags, but the manifest never opted into the check.
        let manifest = manifest_with(None, None, None);
        let decision = AccessPolicy::evaluate(
            &manifest,
            &dims_tree(&["env:prod"]),
            &[ctx("env:prod", &["core"])],
        );
        assert_eq!(decision, AccessDecision::Allowed);
    }
}
