//! Per-request authorization: is `actor` allowed to run `op` against this
//! resolved unit, on top of `crate::auth`'s "is the caller allowed to talk
//! to the server at all".
//!
//! Uses `cubtera_model::Policy` (P6) compiled from the unit manifest's
//! v2-shaped `allowList`/`denyList` - the same rules
//! `AssembleUseCase::build_unit_with_extensions` already enforces via
//! `cubtera_model::AccessPolicy` before this ever runs (so a denied
//! request is already rejected earlier in `run_support::prepare` with a
//! `403`), but expressed here against `(actor, instance, op,
//! resolved_data)` instead of just `(instance,)` - `op.name == 'destroy'`
//! rules that don't exist in v2 at all become possible once a caller
//! (this server, or later the CLI) builds a `Policy` with them.
//!
//! This module's own check is therefore currently a redundant second
//! gate for the exact allow/deny semantics v2 already enforces - real
//! value comes from adding rules that reference `actor`/`op`, which the
//! manifest has no field for yet (a deliberate, separate decision: this
//! phase wires the *engine* through a real request path, not a new
//! manifest schema section for actor-scoped rules).

use crate::error::ApiError;
use cubtera_model::{context_for, Policy, SelectorContext, Unit};
use serde_json::Value;

/// Build the [`SelectorContext`] `Policy::evaluate` needs from a resolved
/// `Unit`: one flat `{"name": ..., ...meta fields}` entry per dimension
/// type in `unit.dimension_data`. `dimension_data[type]` is keyed by
/// section (`{"meta": {...}, ...other sections}` - `Dimension.sections`'s
/// shape, no top-level `"name"` key: neither v2's `Dimension::to_json()`
/// nor v3's `Dimension.sections` ever carried the dimension's own name as
/// a section), so `name` comes from `unit.dimensions` (the resolved
/// `DimRef`s `AssembleUseCase` attached) instead, and `meta`'s fields are
/// lifted a level to match `Selector`'s `<dim_type>.<field>` convention -
/// the same flattening `BindingUseCase::expand` does from the inventory
/// side.
pub fn context_for_unit(unit: &Unit) -> SelectorContext {
    let mut ctx = SelectorContext::new();
    for (dim_type, data) in &unit.dimension_data {
        let mut fields = data.get("meta").cloned().unwrap_or_default();
        let name = unit
            .get_dimension(dim_type)
            .map(|d| Value::String(d.name.as_str().to_string()))
            .unwrap_or(Value::Null);
        match fields.as_object_mut() {
            Some(obj) => {
                obj.insert("name".to_string(), name);
            }
            None => fields = serde_json::json!({ "name": name }),
        }
        ctx.insert(dim_type.clone(), fields);
    }
    ctx
}

/// Deny the request if `unit`'s compiled allow/deny [`Policy`] rejects
/// `(actor, unit's resolved dimensions, op)`. `op` is the run's first
/// command word (`"plan"`/`"apply"`/`"destroy"`/...), matching
/// `cubtera_model::RunOp`'s own vocabulary.
pub fn check(unit: &Unit, actor: &str, op: &str) -> Result<(), ApiError> {
    let allow = unit.manifest.allow_list.clone().unwrap_or_default();
    let deny = unit.manifest.deny_list.clone().unwrap_or_default();
    let policy = Policy::from_allow_deny_lists(&allow, &deny);

    let ctx = context_for(context_for_unit(unit), actor, op);
    let decision = policy.evaluate(&ctx);
    if !decision.is_allowed() {
        return Err(ApiError::forbidden(format!(
            "actor '{actor}' may not run '{op}' against unit '{}': {decision}",
            unit.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_kernel::DimRef;
    use cubtera_model::Manifest;
    use serde_json::json;

    fn unit_with(allow: Option<Vec<&str>>, deny: Option<Vec<&str>>) -> Unit {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.allow_list = allow.map(|v| v.into_iter().map(String::from).collect());
        manifest.deny_list = deny.map(|v| v.into_iter().map(String::from).collect());
        // Deliberately omits "name" from `dimension_data` - real
        // `AssembleUseCase`-built units never put it there either (see
        // `context_for_unit`'s doc comment), so a test fixture that added
        // it back would hide exactly the bug this module used to have.
        Unit::new("network", "cubtera", manifest)
            .with_dimension(DimRef::parse("env:prod").unwrap())
            .with_dimension_data("env", json!({"meta": {}}))
    }

    #[test]
    fn allowed_by_default() {
        let unit = unit_with(None, None);
        assert!(check(&unit, "alice", "apply").is_ok());
    }

    #[test]
    fn denied_when_env_is_in_deny_list() {
        let unit = unit_with(None, Some(vec!["env:prod"]));
        assert!(check(&unit, "alice", "apply").is_err());
    }

    #[test]
    fn denied_when_allow_list_does_not_match() {
        let unit = unit_with(Some(vec!["env:staging"]), None);
        assert!(check(&unit, "alice", "apply").is_err());
    }

    #[test]
    fn allowed_when_allow_list_matches_the_resolved_dimension_name() {
        // Regression test: `context_for_unit` used to source `<dim>.name`
        // from `dimension_data["<dim>"]["name"]`, which no real `Unit`
        // (`AssembleUseCase`-built or otherwise) ever populates - every
        // allow/deny-list rule was silently unmatchable, denying every
        // apply through this route regardless of the manifest's
        // `allowList`. Caught by a live smoke test against
        // `example/units/tf_unit01`, not by this suite - hence this test.
        let unit = unit_with(Some(vec!["env:prod"]), None);
        assert!(check(&unit, "alice", "apply").is_ok());
    }
}
