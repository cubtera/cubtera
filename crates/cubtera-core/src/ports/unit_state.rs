//! Unit state repository port (interface)
//!
//! Producers publish their outputs here after a successful apply/destroy
//! (`[outputs] publish = true`); consumers read them back via
//! `[inputs.<alias>]` - see `cubtera_domain::project_state_key` for the key
//! projection algorithm that turns "producer's required dims" + "consumer's
//! resolved chain" into the exact key looked up here.

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::{UnitStateKey, UnitStateRecord};

/// Stores/retrieves published unit outputs, keyed by [`UnitStateKey`].
#[async_trait]
pub trait UnitStateRepository: Send + Sync {
    /// Look up a producer's published outputs by key.
    async fn get(&self, key: &UnitStateKey) -> AppResult<Option<UnitStateRecord>>;

    /// Publish (or replace) a producer's outputs. Idempotent: publishing
    /// the same key twice overwrites, it does not append.
    async fn put(&self, record: &UnitStateRecord) -> AppResult<()>;

    /// Remove a producer's published outputs, if present.
    async fn delete(&self, key: &UnitStateKey) -> AppResult<()>;

    /// List every record published for `unit` in `org` (every dims/ext
    /// combination it has ever published under) - used by `cubtera state ls`.
    async fn list(&self, org: &str, unit: &str) -> AppResult<Vec<UnitStateRecord>>;
}
