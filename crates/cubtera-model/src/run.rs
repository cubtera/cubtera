use crate::ids::{PlanId, RunId};
use crate::revision::Revision;
use cubtera_kernel::{Ident, InstanceId};
use serde::{Deserialize, Serialize};

/// What a [`Run`] is doing. `Other` covers runner-specific verbs (`init`,
/// arbitrary bash commands) without forcing every future verb into this
/// enum - but `should_publish`/deployment-log gating only ever fire for
/// `Apply`/`Destroy` (the same gate v2's `RunService::should_log_command`
/// applied to a bare command string; here it is a real variant, not a
/// string prefix check).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunOp {
    Plan,
    Apply,
    Destroy,
    Init,
    Other(String),
}

impl RunOp {
    /// Only `apply`/`destroy` publish `[outputs]` or append to the
    /// deployment log - see ยง"Runner pipeline" in AGENTS.md, ported
    /// unchanged as a policy decision (not a mechanism) into v3.
    pub fn publishes(&self) -> bool {
        matches!(self, RunOp::Apply | RunOp::Destroy)
    }
}

/// `queued -> resolving -> planning|applying -> succeeded|failed|cancelled`,
/// exactly the state machine in the spec (ยง2). `Store::update_run`'s
/// `RunPatch` only ever moves a `Run` forward through this chain - there is
/// no adapter-level enforcement of that here (that is a `cubtera-app` use-
/// case concern), but the type only offers states that exist in the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Queued,
    Resolving,
    Planning,
    Applying,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        )
    }
}

/// One execution attempt against an [`InstanceId`]. `logs_ref` is an
/// opaque pointer into wherever log bytes actually live (a content-
/// addressed artifact via `Store::put_artifact`, or a log-streaming
/// backend added in P7) - `cubtera-store` never inlines log bytes into the
/// `runs` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub instance: InstanceId,
    pub op: RunOp,
    pub status: RunStatus,
    pub actor: Ident,
    pub plan_ref: Option<PlanId>,
    /// Unix milliseconds - `cubtera-app`'s `Clock` port supplies this, the
    /// model layer just stores whatever it's given.
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub logs_ref: Option<String>,
    pub produced_outputs_revision: Option<Revision>,
}

impl Run {
    pub fn queued(
        id: RunId,
        instance: InstanceId,
        op: RunOp,
        actor: Ident,
        started_at: i64,
    ) -> Self {
        Self {
            id,
            instance,
            op,
            status: RunStatus::Queued,
            actor,
            plan_ref: None,
            started_at,
            finished_at: None,
            exit_code: None,
            logs_ref: None,
            produced_outputs_revision: None,
        }
    }
}

/// A partial update to a [`Run`] row - `Store::update_run` only touches the
/// fields set to `Some`, leaving the rest as-is. Keeps the port from
/// needing a full `Run` (with its immutable fields re-supplied) just to
/// flip `status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPatch {
    pub status: Option<RunStatus>,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub logs_ref: Option<String>,
    pub produced_outputs_revision: Option<Revision>,
}

impl RunPatch {
    pub fn apply_to(&self, run: &mut Run) {
        if let Some(status) = self.status {
            run.status = status;
        }
        if let Some(finished_at) = self.finished_at {
            run.finished_at = Some(finished_at);
        }
        if let Some(exit_code) = self.exit_code {
            run.exit_code = Some(exit_code);
        }
        if let Some(logs_ref) = self.logs_ref.clone() {
            run.logs_ref = Some(logs_ref);
        }
        if let Some(revision) = self.produced_outputs_revision {
            run.produced_outputs_revision = Some(revision);
        }
    }
}

/// Query filter for `Store::list_runs` - every field is optional/`None`
/// meaning "don't filter on this".
#[derive(Debug, Clone, Default)]
pub struct RunFilter {
    /// Exact-match on a single run - `cubtera explain run <run_id>`'s
    /// filter shape (P4-run). Combined with other fields via `AND`, though
    /// in practice an `id` filter is specific enough on its own.
    pub id: Option<RunId>,
    pub org: Option<Ident>,
    pub instance: Option<InstanceId>,
    pub status: Option<RunStatus>,
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_apply_and_destroy_publish() {
        assert!(RunOp::Apply.publishes());
        assert!(RunOp::Destroy.publishes());
        assert!(!RunOp::Plan.publishes());
        assert!(!RunOp::Init.publishes());
        assert!(!RunOp::Other("console".into()).publishes());
    }

    #[test]
    fn run_patch_only_touches_set_fields() {
        let mut run = Run::queued(
            RunId::new("r1"),
            crate::test_support::instance("cubtera", "network", &["dome:prod"]),
            RunOp::Apply,
            Ident::parse("ci").unwrap(),
            1000,
        );
        run.exit_code = Some(7);

        let patch = RunPatch {
            status: Some(RunStatus::Succeeded),
            finished_at: Some(2000),
            exit_code: None,
            logs_ref: None,
            produced_outputs_revision: None,
        };
        patch.apply_to(&mut run);

        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.finished_at, Some(2000));
        // exit_code was None in the patch, so the earlier value survives.
        assert_eq!(run.exit_code, Some(7));
    }

    #[test]
    fn terminal_states() {
        assert!(RunStatus::Succeeded.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(!RunStatus::Queued.is_terminal());
        assert!(!RunStatus::Planning.is_terminal());
    }
}
