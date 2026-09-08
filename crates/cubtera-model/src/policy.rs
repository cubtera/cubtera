//! Unified access policy engine (P6, section 5.5): compiles v2's three
//! independent gates (`allowList`/`denyList`/`affinityTags`,
//! `cubtera_domain::access::AccessPolicy`) into one evaluator over
//! `(actor, instance, op, resolved_data)`, reusing [`crate::Selector`]
//! instead of inventing a second expression language - a
//! `PolicyRule::when` is exactly the same grammar a `Binding::selector`
//! already uses, evaluated against the same [`SelectorContext`] shape
//! plus two reserved pseudo dimension-types the caller injects:
//! `actor.name` and `op.name` (see [`context_for`]).
//!
//! Semantics, chosen to be a strict superset of v2's:
//! - No rules at all -> [`PolicyDecision::Allowed`] (v2's "no
//!   restrictions" default).
//! - Any matching `Deny` rule -> denied immediately, regardless of any
//!   `Allow` rule (v2's denyList always won too - it was checked
//!   independently of allowList, never overridden by it).
//! - If at least one `Allow` rule exists, at least one of them must match
//!   (v2's allowList: presence makes it a whitelist).
//!
//! `affinityTags` is deliberately *not* folded into this - it's a
//! different shape of constraint ("every resolved dimension must overlap
//! the unit's tags", a universal quantification over the whole
//! [`SelectorContext`], not a single-record match) and stays a dedicated
//! check in `cubtera_domain::access` until v2 is retired (P7). Compiling
//! `allowList`/`denyList` into [`Policy`] here is the real, usable half of
//! "one engine instead of three mechanisms"; a full affinity-as-Selector
//! encoding is a deliberate non-goal for this phase, not an oversight.

use crate::binding::{Literal, Path, Selector, SelectorContext};
use serde::Serialize;
use serde_json::json;
use std::fmt;

/// What a matching [`PolicyRule`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Effect {
    Allow,
    Deny,
}

/// One rule: if `when` matches the evaluation context, contribute
/// `effect` to the decision (see [`Policy::evaluate`] for how multiple
/// rules combine).
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyRule {
    pub effect: Effect,
    pub when: Selector,
}

impl PolicyRule {
    pub fn allow(when: Selector) -> Self {
        Self {
            effect: Effect::Allow,
            when,
        }
    }

    pub fn deny(when: Selector) -> Self {
        Self {
            effect: Effect::Deny,
            when,
        }
    }
}

/// An ordered set of [`PolicyRule`]s - the compiled form of a unit's
/// `allowList`/`denyList` (via [`Policy::from_allow_deny_lists`]), or any
/// other rule source (P7's server-side authz can build one directly from
/// role assignments using the same `actor.name`/`op.name` context shape).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Policy {
    pub rules: Vec<PolicyRule>,
}

/// Outcome of evaluating a [`Policy`] - deliberately the same shape as
/// v2's `cubtera_domain::access::AccessDecision`, so a caller migrating
/// from `AccessPolicy::evaluate` to `Policy::evaluate` doesn't also have
/// to change how it reacts to the result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PolicyDecision {
    Allowed,
    Denied { reason: String },
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

impl fmt::Display for PolicyDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allowed => write!(f, "allowed"),
            Self::Denied { reason } => write!(f, "denied: {reason}"),
        }
    }
}

impl Policy {
    /// Compile v2-shaped `allowList`/`denyList` (`"type:name"` entries,
    /// matched against the *full* ancestor `dims_tree`, exactly like
    /// `cubtera_domain::access::AccessPolicy::evaluate`) into a [`Policy`].
    /// Each entry becomes one rule whose `when` is `<dim_type>.name ==
    /// "<name>"` - a `dims_tree` entry like `"dome:prod"` only makes sense
    /// against a [`SelectorContext`] that has a `"dome"` key with
    /// `{"name": "prod", ...}`, which is exactly what
    /// `cubtera_app::bindings::BindingUseCase::expand` already builds.
    ///
    /// Malformed entries (missing the `:`) are skipped rather than
    /// erroring - the same permissiveness `DimRef::parse` chose not to
    /// have at a real boundary, but acceptable here since a policy that's
    /// merely "less restrictive than intended" is far safer to fail open
    /// on for a *compile* step than one that's silently more restrictive
    /// or panics on a config typo. Prefer fixing the source manifest.
    pub fn from_allow_deny_lists(allow_list: &[String], deny_list: &[String]) -> Self {
        let mut rules = Vec::with_capacity(allow_list.len() + deny_list.len());
        for entry in allow_list {
            if let Some(selector) = selector_for_type_name(entry) {
                rules.push(PolicyRule::allow(selector));
            }
        }
        for entry in deny_list {
            if let Some(selector) = selector_for_type_name(entry) {
                rules.push(PolicyRule::deny(selector));
            }
        }
        Self { rules }
    }

    /// Evaluate every rule against `ctx`. See the module doc comment for
    /// the combination semantics.
    pub fn evaluate(&self, ctx: &SelectorContext) -> PolicyDecision {
        let mut any_allow_rule = false;
        let mut matched_allow = false;

        for rule in &self.rules {
            let is_match = rule.when.evaluate(ctx);
            match rule.effect {
                Effect::Deny if is_match => {
                    return PolicyDecision::Denied {
                        reason: format!("denied by rule 'deny when {}'", rule.when_display()),
                    };
                }
                Effect::Deny => {}
                Effect::Allow => {
                    any_allow_rule = true;
                    matched_allow |= is_match;
                }
            }
        }

        if any_allow_rule && !matched_allow {
            return PolicyDecision::Denied {
                reason: "no Allow rule matched".to_string(),
            };
        }
        PolicyDecision::Allowed
    }
}

impl PolicyRule {
    fn when_display(&self) -> String {
        format!("{:?}", self.when)
    }
}

/// `<dim_type>.name == "<name>"` for a `"type:name"` entry, or `None` if
/// `entry` isn't in that shape.
fn selector_for_type_name(entry: &str) -> Option<Selector> {
    let (dim_type, name) = entry.split_once(':')?;
    Some(Selector::Eq(
        Path {
            dim_type: dim_type.to_string(),
            field: "name".to_string(),
        },
        Literal::Str(name.to_string()),
    ))
}

/// Inject the two reserved pseudo dimension-types a [`Policy`] can match
/// on beyond a `Binding`'s usual per-dimension fields: `actor.name` and
/// `op.name`. Callers build the rest of `ctx` the same way
/// `BindingUseCase::expand` does (one entry per resolved dimension type);
/// this just adds the two fields that make `(actor, instance, op,
/// resolved_data)` expressible in the *same* context map instead of a
/// separate parameter list.
pub fn context_for(mut ctx: SelectorContext, actor: &str, op: &str) -> SelectorContext {
    ctx.insert("actor".to_string(), json!({"name": actor}));
    ctx.insert("op".to_string(), json!({"name": op}));
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(dome: &str) -> SelectorContext {
        let mut ctx = SelectorContext::new();
        ctx.insert("dome".to_string(), json!({"name": dome}));
        ctx.insert("env".to_string(), json!({"name": "prod"}));
        ctx
    }

    #[test]
    fn no_rules_is_allowed() {
        let policy = Policy::default();
        assert_eq!(policy.evaluate(&ctx_with("prod")), PolicyDecision::Allowed);
    }

    #[test]
    fn allow_list_denies_when_nothing_matches() {
        let policy = Policy::from_allow_deny_lists(&["env:staging".to_string()], &[]);
        let decision = policy.evaluate(&ctx_with("prod"));
        assert!(!decision.is_allowed());
    }

    #[test]
    fn allow_list_allows_on_an_ancestor_match() {
        let policy = Policy::from_allow_deny_lists(&["dome:prod".to_string()], &[]);
        assert_eq!(policy.evaluate(&ctx_with("prod")), PolicyDecision::Allowed);
    }

    #[test]
    fn deny_list_always_wins_even_without_an_allow_list() {
        let policy = Policy::from_allow_deny_lists(&[], &["env:prod".to_string()]);
        let decision = policy.evaluate(&ctx_with("prod"));
        assert!(!decision.is_allowed());
    }

    #[test]
    fn deny_wins_over_a_matching_allow() {
        let policy =
            Policy::from_allow_deny_lists(&["dome:prod".to_string()], &["env:prod".to_string()]);
        let decision = policy.evaluate(&ctx_with("prod"));
        assert!(!decision.is_allowed());
    }

    #[test]
    fn actor_and_op_are_matchable_pseudo_dimensions() {
        let policy = Policy {
            rules: vec![PolicyRule::deny(
                Selector::parse("op.name == 'destroy'").unwrap(),
            )],
        };
        let ctx = context_for(ctx_with("prod"), "alice", "destroy");
        assert!(!policy.evaluate(&ctx).is_allowed());

        let ctx = context_for(ctx_with("prod"), "alice", "plan");
        assert_eq!(policy.evaluate(&ctx), PolicyDecision::Allowed);
    }

    #[test]
    fn malformed_list_entries_are_skipped_not_errors() {
        let policy = Policy::from_allow_deny_lists(&["no-colon-here".to_string()], &[]);
        // An allow list with zero *usable* rules behaves like "no allow
        // rules at all" - still permissive, since the malformed entry
        // never became a rule.
        assert_eq!(policy.rules.len(), 0);
        assert_eq!(policy.evaluate(&ctx_with("prod")), PolicyDecision::Allowed);
    }
}
