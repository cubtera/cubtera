use crate::error::StoreResult;
use async_trait::async_trait;
use cubtera_kernel::{Digest, Ident, InstanceId};
use cubtera_model::{
    Instance, Lease, OutputSet, Plan, PlanId, Revision, Run, RunFilter, RunId, RunPatch,
    StaleConsumer,
};
use std::time::Duration;

/// The one storage port for everything durable in v3: instance specs, plan
/// artifacts, run history, published outputs, mutual-exclusion leases, and
/// content-addressed blobs. This replaces three independently-inconsistent
/// v2 mechanisms (a JSONL deployment log, a JSON-per-key unit-state file,
/// and no `Plan`/`Run` persistence at all) with one transactional,
/// revisioned store - see docs/specs/2026-09-03-cubtera-v3-architecture.md
/// ยง9.
///
/// Every write that can race (`upsert_instance`, `put_output_set`,
/// `acquire_lease`/`renew_lease`/`release_lease`) is optimistic-concurrency
/// or fencing-token based, never "last write wins" - the exact class of bug
/// v2 shipped (best-effort, unordered, non-transactional state publish).
#[async_trait]
pub trait Store: Send + Sync {
    /// Insert or update an instance's spec row. `expected` must be `None`
    /// to create a brand-new row (fails if one already exists), or
    /// `Some(revision)` matching the row's current `spec_revision` to
    /// update it (fails with [`crate::StoreError::RevisionConflict`]
    /// otherwise). Returns the new revision on success.
    async fn upsert_instance(
        &self,
        inst: &Instance,
        expected: Option<Revision>,
    ) -> StoreResult<Revision>;
    async fn get_instance(&self, id: &InstanceId) -> StoreResult<Option<Instance>>;
    async fn list_instances(&self, org: &Ident) -> StoreResult<Vec<Instance>>;

    async fn put_plan(&self, plan: &Plan) -> StoreResult<()>;
    async fn get_plan(&self, id: &PlanId) -> StoreResult<Option<Plan>>;

    async fn append_run(&self, run: &Run) -> StoreResult<()>;
    async fn update_run(&self, id: &RunId, patch: RunPatch) -> StoreResult<()>;
    async fn list_runs(&self, filter: RunFilter) -> StoreResult<Vec<Run>>;

    /// Publish a new output set for `key`, assigning the next monotonic
    /// [`Revision`] for that instance and returning it. Never overwrites a
    /// prior revision - state-mesh history is append-only, unlike v2's
    /// unversioned JSON-per-key file that `destroy` silently overwrote.
    async fn put_output_set(&self, key: &InstanceId, set: &OutputSet) -> StoreResult<Revision>;
    /// The latest (highest-revision) output set for `key`, if any has ever
    /// been published.
    async fn get_output_set(&self, key: &InstanceId) -> StoreResult<Option<OutputSet>>;
    /// Record that `consumer` has consumed `producer`'s output set as of
    /// `revision` - the bookkeeping `list_stale_consumers`/`state ls
    /// --stale` reads back.
    async fn mark_consumed(
        &self,
        consumer: &InstanceId,
        producer: &InstanceId,
        revision: Revision,
    ) -> StoreResult<()>;
    async fn list_stale_consumers(&self, org: &Ident) -> StoreResult<Vec<StaleConsumer>>;

    /// Acquire a mutual-exclusion lease over `key`. Fails with
    /// [`crate::StoreError::LeaseHeld`] if another owner's lease on the
    /// same instance hasn't expired yet - closing v2's gap where only
    /// `init` was locked (via a TCP port), and nothing else was.
    async fn acquire_lease(
        &self,
        key: &InstanceId,
        owner: &str,
        ttl: Duration,
    ) -> StoreResult<Lease>;
    /// Extend a held lease by its own `ttl_ms`, from now. Fails with
    /// [`crate::StoreError::LeaseLost`] if `lease.token` no longer matches
    /// the current holder (expired and re-acquired by someone else, or
    /// already released).
    async fn renew_lease(&self, lease: &Lease) -> StoreResult<Lease>;
    /// Release a held lease. Fails with
    /// [`crate::StoreError::LeaseLost`] under the same conditions as
    /// `renew_lease` - a caller can never release a lease it no longer
    /// actually holds.
    async fn release_lease(&self, lease: Lease) -> StoreResult<()>;

    /// Store `bytes` under their blake3 digest (deduplicating automatically
    /// - re-`put`ting identical bytes is a no-op past the first write) and
    /// return that digest.
    async fn put_artifact(&self, bytes: &[u8]) -> StoreResult<Digest>;
    async fn get_artifact(&self, digest: &Digest) -> StoreResult<Option<Vec<u8>>>;
}
