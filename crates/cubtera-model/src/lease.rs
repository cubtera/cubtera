use cubtera_kernel::InstanceId;
use serde::{Deserialize, Serialize};

/// A mutual-exclusion lease over one [`InstanceId`], held for
/// `acquired_at..expires_at`. Any mutating operation (`plan`, `apply`,
/// `destroy`) must hold one - closing v2's "only `init` was locked, and
/// only via a TCP port" gap (nothing stopped two concurrent `apply`s
/// against the same instance).
///
/// `token` is an opaque fencing token minted by `cubtera-store` at
/// `acquire_lease` time: `renew_lease`/`release_lease` must present the
/// exact token they were handed, so a lease that already expired and was
/// re-acquired by someone else can never be renewed/released by the
/// original, now-stale holder (the classic "renew after your lease already
/// expired" race).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub instance: InstanceId,
    pub owner: String,
    pub token: String,
    pub acquired_at: i64,
    /// The duration (milliseconds) this lease renews by - `renew_lease`
    /// recomputes `expires_at` as `now + ttl_ms` rather than requiring the
    /// caller to pass a TTL on every renewal.
    pub ttl_ms: i64,
    pub expires_at: i64,
}

impl Lease {
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease() -> Lease {
        Lease {
            instance: crate::test_support::instance("cubtera", "network", &["dome:prod"]),
            owner: "ci".into(),
            token: "tok-1".into(),
            acquired_at: 1000,
            ttl_ms: 500,
            expires_at: 1500,
        }
    }

    #[test]
    fn expiry_is_inclusive_of_the_boundary() {
        let l = lease();
        assert!(!l.is_expired(1499));
        assert!(l.is_expired(1500));
        assert!(l.is_expired(1501));
    }
}
