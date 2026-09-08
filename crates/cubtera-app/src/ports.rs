//! Ports `cubtera-app`'s use cases depend on. Every I/O boundary this
//! crate needs lives here as a trait; adapters live in leaf crates (a
//! thin bridge onto the existing `cubtera-persistence` FS adapter for
//! P3, a native SQLite/FS adapter of its own once v2's `cubtera-core` is
//! retired - see docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9).

use crate::error::AppResult;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

/// Raw, adapter-supplied sections for one dimension record - deliberately
/// "dumb" like v2's `cubtera_domain::RawDimension`: no gap-fill, no parent
/// resolution, no schema checking. All of that is `ResolveUseCase`'s job,
/// operating on `cubtera_model::Dimension::assemble`.
pub type RawSections = BTreeMap<String, Value>;

/// Read-only inventory access `cubtera-app`'s use cases need. A subset of
/// v2's `cubtera_core::ports::InventoryRepository` (no `save_raw`/
/// `delete_raw` - resolve/validate never write) using this crate's own
/// types so `cubtera-app` never has to depend on `cubtera-core`.
#[async_trait]
pub trait InventoryPort: Send + Sync {
    /// Fetch the raw record for a dimension by type and name.
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawSections>>;

    /// Fetch the raw defaults record for a dimension type (".default").
    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawSections>>;

    /// Fetch the JSON-schema for a dimension type (its ".schema" record's
    /// "meta" section), if one is defined.
    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<Value>>;

    /// List all dimension names of a given type (excludes reserved names).
    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>>;
}
