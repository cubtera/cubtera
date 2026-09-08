use serde::{Deserialize, Serialize};
use std::fmt;

/// A monotonic version counter for one row identity (an `Instance`'s spec,
/// one `InstanceId`'s `OutputSet` history, ...). `cubtera-store` is the only
/// thing that ever mints a new [`Revision`] (`upsert_instance`/
/// `put_output_set`'s return value) - callers only ever compare against a
/// value they were previously handed, never construct one out of thin air
/// (other than [`Revision::INITIAL`], the value a brand-new row starts at).
///
/// This is what makes `Store::upsert_instance`'s `expected: Option<Revision>`
/// a real optimistic-concurrency check: "I last saw revision N, only apply
/// my write if nobody else has moved it since" - the exact guarantee v2's
/// unversioned JSON-per-key unit state never gave (H4/H6 in the prior
/// architecture review: best-effort, unordered, non-transactional writes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Revision(u64);

impl Revision {
    /// The revision a row has before it has ever been written.
    pub const INITIAL: Revision = Revision(0);

    /// Only `cubtera-store` should call this - it is the thing that knows
    /// what "the next revision" means for a given row. Exposed so the
    /// adapter (which lives in a different crate) can construct one from a
    /// value it just read out of SQLite.
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_is_zero() {
        assert_eq!(Revision::INITIAL.value(), 0);
    }

    #[test]
    fn next_increments() {
        assert_eq!(Revision::INITIAL.next().value(), 1);
        assert_eq!(Revision::from_raw(41).next(), Revision::from_raw(42));
    }
}
