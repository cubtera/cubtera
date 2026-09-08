/// Errors from the [`crate::Store`] port. Every variant is something a
/// caller (`cubtera-app`, once it lands in P3) needs to branch on, not a
/// generic "something went wrong in SQLite" - optimistic-concurrency
/// conflicts and lease contention are expected, routine outcomes, not
/// exceptional adapter failures.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// `upsert_instance`'s `expected` revision didn't match what's
    /// currently stored - someone else wrote in between. Carries both
    /// sides so the caller can decide whether to retry with the fresh
    /// value or surface a conflict to a human.
    #[error("revision conflict: expected {expected:?}, found {actual:?}")]
    RevisionConflict {
        expected: Option<cubtera_model::Revision>,
        actual: Option<cubtera_model::Revision>,
    },

    /// `acquire_lease` found an unexpired lease held by someone else.
    #[error("lease already held on this instance until {expires_at}")]
    LeaseHeld { expires_at: i64 },

    /// `renew_lease`/`release_lease` presented a token that no longer
    /// matches the current holder (already released, expired and
    /// re-acquired by someone else, or never existed).
    #[error("lease token is stale or unknown")]
    LeaseLost,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("store data corruption: {0}")]
    Corrupt(String),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("store backend task failed: {0}")]
    Backend(String),
}

pub type StoreResult<T> = Result<T, StoreError>;
