use crate::ids::RunId;
use crate::revision::Revision;
use cubtera_kernel::{Digest, Ident, InstanceId};
use serde::{Deserialize, Serialize};

/// The durable record `cubtera-store` keeps per [`InstanceId`]: what package
/// it was last resolved from, what its last run/outputs were. This is the
/// thing that makes "what *should* be running" answerable without
/// replaying every `Run` - v2 had no equivalent (ยง0, "no first-class,
/// addressable... entity that persists across invocations").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    /// The `Binding` (P5) that produced this instance, if any - `None` for
    /// an instance resolved ad hoc via `cubtera plan -u ... -d ...` rather
    /// than expanded from desired state.
    pub binding_ref: Option<Ident>,
    /// Content hash of the `UnitPackage` this instance was last
    /// planned/applied against.
    pub unit_package: Digest,
    /// Optimistic-concurrency revision for this row - passed back to
    /// `Store::upsert_instance` as `expected` on the next write.
    pub spec_revision: Revision,
    pub last_run: Option<RunId>,
    pub last_outputs_revision: Option<Revision>,
}

impl Instance {
    /// A brand-new instance record, not yet persisted - `spec_revision` is
    /// `Revision::INITIAL` until `Store::upsert_instance` returns the first
    /// real revision.
    pub fn new(id: InstanceId, unit_package: Digest) -> Self {
        Self {
            id,
            binding_ref: None,
            unit_package,
            spec_revision: Revision::INITIAL,
            last_run: None,
            last_outputs_revision: None,
        }
    }
}
