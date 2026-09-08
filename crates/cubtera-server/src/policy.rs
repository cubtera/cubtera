//! Per-request authorization: is `actor` allowed to run `op` against this
//! resolved unit, on top of `crate::auth`'s "is the caller allowed to talk
//! to the server at all".
//!
//! Uses `cubtera_model::Policy` (P6) compiled from the unit manifest's
//! v2-shaped `allowList`/`denyList` - the same rules
//! `UnitService::build_unit_with_extensions` already enforces via
//! `cubtera_domain::AccessPolicy` before this ever runs (so a denied
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
use cubtera_domain::Unit;
use cubtera_model::{context_for, Policy, SelectorContext};
use serde_json::Value;

/// Build the [`SelectorContext`] `Policy::evaluate` needs from a resolved
/// `Unit`: one flat `{"name": ..., ...meta fields}` entry per dimension
/// type in `unit.dimension_data` (which is keyed the same way
/// `Dimension::to_json()` is - `{"name", "meta": {...}, ...other
/// sections}` - so `meta`'s fields have to be lifted a level to match
/// `Selector`'s `<dim_type>.<field>` convention, the same flattening
/// `BindingUseCase::expand` does from the inventory side).
pub fn context_for_unit(unit: &Unit) -> SelectorContext {
    let mut ctx = SelectorContext::new();
    for (dim_type, data) in &unit.dimension_data {
        let mut fields = data.get("meta").cloned().unwrap_or_default();
        let name = data.get("name").cloned().unwrap_or(Value::Null);
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
    use cubtera_domain::{DimensionRef, Manifest};
    use serde_json::json;

    fn unit_with(allow: Option<Vec<&str>>, deny: Option<Vec<&str>>) -> Unit {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.allow_list = allow.map(|v| v.into_iter().map(String::from).collect());
        manifest.deny_list = deny.map(|v| v.into_iter().map(String::from).collect());
        Unit::new("network", "cubtera", manifest)
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension_data("env", json!({"name": "prod", "meta": {}}))
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
}
