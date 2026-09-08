//! Gap-fill merge with field-level provenance.
//!
//! v2's `cubtera_domain::gap_fill_merge` (ported here unchanged in spirit -
//! same merge semantics, same "existing wins, missing keys are copied,
//! nested objects recurse" rule) merges a dimension's own data with its
//! type's `.default` record, but throws away *which* fields came from
//! which layer once the merge is done. `docs/specs/2026-09-03-cubtera-v3-architecture.md`
//! ยง5.2 calls this out explicitly: there was no way to answer "why does
//! `dc:prod-use1` have `region = us-east-1`" without reading the `.default`
//! file by hand and diffing it against the dimension's own JSON.
//!
//! [`gap_fill_merge_with_provenance`] fixes that: every field that gets
//! filled in from `defaults` (rather than already present in `data`) is
//! recorded in `provenance`, keyed by a `/`-joined field path. A whole
//! subtree copied wholesale from defaults (the dimension didn't have that
//! section/key at all) is recorded once, at the subtree's own path, not
//! once per leaf inside it - [`field_provenance_for`] resolves a deeper
//! query path (e.g. `"meta/account_id"`) against the nearest recorded
//! ancestor path (e.g. `"meta"`) so that still answers correctly.

use cubtera_kernel::Ident;
use serde_json::Value;
use std::collections::BTreeMap;

/// Where a field's value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceSource {
    /// Present in the dimension's own record already - `defaults` never
    /// contributed this field (even if `defaults` also declares a value
    /// for it, the dimension's own value always wins and is what's
    /// stored).
    Own,
    /// Not present in the dimension's own record; copied from `dim_type`'s
    /// `.default` record.
    Default(Ident),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProvenance {
    pub source: ProvenanceSource,
}

/// Merge `defaults` into `data` in place, exactly like v2's
/// `gap_fill_merge` (data's own value always wins; missing keys are
/// copied; nested objects recurse; non-object/non-matching shapes at a key
/// just keep `data`'s value as-is), while recording provenance for every
/// key this call touches under `path` (empty string for the root call).
pub fn gap_fill_merge_with_provenance(
    data: &mut Value,
    defaults: &Value,
    default_source: &Ident,
    path: &str,
    provenance: &mut BTreeMap<String, FieldProvenance>,
) {
    let (Value::Object(data_obj), Value::Object(defaults_obj)) = (data, defaults) else {
        return;
    };
    for (key, default_value) in defaults_obj {
        let field_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}/{key}")
        };
        match data_obj.get_mut(key) {
            None => {
                data_obj.insert(key.clone(), default_value.clone());
                provenance.insert(
                    field_path,
                    FieldProvenance {
                        source: ProvenanceSource::Default(default_source.clone()),
                    },
                );
            }
            Some(existing) if existing.is_object() && default_value.is_object() => {
                gap_fill_merge_with_provenance(
                    existing,
                    default_value,
                    default_source,
                    &field_path,
                    provenance,
                );
            }
            Some(_) => {
                provenance.entry(field_path).or_insert(FieldProvenance {
                    source: ProvenanceSource::Own,
                });
            }
        }
    }
}

/// Resolve provenance for `query_path` (e.g. `"meta/account_id"`) against a
/// provenance map built by [`gap_fill_merge_with_provenance`]: an exact
/// match wins, otherwise the nearest recorded ancestor path (a whole
/// subtree that was copied from defaults in one go covers every leaf
/// beneath it). Returns `None` if nothing under `query_path` was ever
/// recorded - meaning it's `Own` and simply wasn't touched by any merge.
pub fn field_provenance_for<'a>(
    provenance: &'a BTreeMap<String, FieldProvenance>,
    query_path: &str,
) -> Option<&'a FieldProvenance> {
    if let Some(p) = provenance.get(query_path) {
        return Some(p);
    }
    let mut prefix = query_path;
    while let Some(idx) = prefix.rfind('/') {
        prefix = &prefix[..idx];
        if let Some(p) = provenance.get(prefix) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dc() -> Ident {
        Ident::parse("dc").unwrap()
    }

    #[test]
    fn own_value_wins_and_is_not_recorded_as_defaulted() {
        let mut data = json!({"meta": {"region": "us-west-2"}});
        let defaults = json!({"meta": {"region": "us-east-1"}});
        let mut prov = BTreeMap::new();
        gap_fill_merge_with_provenance(&mut data, &defaults, &dc(), "", &mut prov);

        assert_eq!(data["meta"]["region"], "us-west-2");
        assert_eq!(
            field_provenance_for(&prov, "meta/region"),
            Some(&FieldProvenance {
                source: ProvenanceSource::Own
            })
        );
    }

    #[test]
    fn missing_leaf_is_filled_and_recorded_as_default() {
        let mut data = json!({"meta": {}});
        let defaults = json!({"meta": {"region": "us-east-1"}});
        let mut prov = BTreeMap::new();
        gap_fill_merge_with_provenance(&mut data, &defaults, &dc(), "", &mut prov);

        assert_eq!(data["meta"]["region"], "us-east-1");
        assert_eq!(
            field_provenance_for(&prov, "meta/region"),
            Some(&FieldProvenance {
                source: ProvenanceSource::Default(dc())
            })
        );
    }

    #[test]
    fn missing_whole_section_is_recorded_once_at_the_section_root() {
        let mut data = json!({});
        let defaults = json!({"manifest": {"owner": "platform", "tier": 1}});
        let mut prov = BTreeMap::new();
        gap_fill_merge_with_provenance(&mut data, &defaults, &dc(), "", &mut prov);

        assert_eq!(data, json!({"manifest": {"owner": "platform", "tier": 1}}));
        // Recorded once at "manifest", not once per leaf underneath it.
        assert_eq!(prov.len(), 1);
        assert!(prov.contains_key("manifest"));

        // A deep query path still resolves via the recorded ancestor.
        assert_eq!(
            field_provenance_for(&prov, "manifest/owner").map(|p| &p.source),
            Some(&ProvenanceSource::Default(dc()))
        );
    }

    #[test]
    fn untouched_field_has_no_provenance_entry() {
        let mut data = json!({"meta": {"region": "us-west-2"}});
        let defaults = json!({});
        let mut prov = BTreeMap::new();
        gap_fill_merge_with_provenance(&mut data, &defaults, &dc(), "", &mut prov);
        assert_eq!(field_provenance_for(&prov, "meta/region"), None);
    }

    #[test]
    fn matches_v2_gap_fill_merge_semantics_for_non_object_conflict() {
        // If data's value isn't an object but defaults' is (or vice
        // versa), data's value wins outright - no recursion, no error.
        let mut data = json!({"tags": "prod"});
        let defaults = json!({"tags": {"env": "prod"}});
        let mut prov = BTreeMap::new();
        gap_fill_merge_with_provenance(&mut data, &defaults, &dc(), "", &mut prov);
        assert_eq!(data["tags"], json!("prod"));
    }
}
