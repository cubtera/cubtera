//! Contract test suite for [`UnitStateRepository`].
//!
//! Same idea as `tests/support_dlog/mod.rs`: these functions encode the
//! *port* contract, run against a freshly constructed repository from
//! `tests/unit_state_contract_fs.rs` (always) and
//! `tests/unit_state_contract_mongo.rs` (when `CUBTERA_TEST_MONGO_URL` is
//! set).

use cubtera_core::ports::UnitStateRepository;
use cubtera_domain::{UnitStateKey, UnitStateRecord};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A fresh, unique org name per call - matters against a shared backing
/// store like a real MongoDB instance (see `tests/support_dlog/mod.rs`'s
/// `unique_org` for the same reasoning).
fn unique_org() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("contract-org-{nanos}-{n}")
}

fn record(org: &str, unit: &str, dims: &[&str], outputs: serde_json::Value) -> UnitStateRecord {
    UnitStateRecord {
        org: org.to_string(),
        unit: unit.to_string(),
        dims: dims.iter().map(|s| s.to_string()).collect(),
        ext: vec![],
        outputs,
        updated_at: 1,
    }
}

/// A record saved via `put` must be retrievable via `get` with the same key.
pub async fn put_then_get_round_trips(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    let record = record(&org, "network", &["dome:prod"], json!({"vpc_id": "vpc-1"}));

    repo.put(&record).await.unwrap();
    let fetched = repo.get(&record.key()).await.unwrap().unwrap();
    assert_eq!(fetched.outputs, json!({"vpc_id": "vpc-1"}));
}

/// `get` on a key that was never published returns `None`, not an error.
pub async fn get_missing_key_returns_none(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    let key = UnitStateKey::new(org, "network", vec!["dome:prod".to_string()], vec![]);
    assert!(repo.get(&key).await.unwrap().is_none());
}

/// `put` twice for the same key overwrites rather than duplicating.
pub async fn put_overwrites_existing_record_for_same_key(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    let first = record(&org, "network", &["dome:prod"], json!({"vpc_id": "vpc-1"}));
    let second = record(&org, "network", &["dome:prod"], json!({"vpc_id": "vpc-2"}));

    repo.put(&first).await.unwrap();
    repo.put(&second).await.unwrap();

    let fetched = repo.get(&first.key()).await.unwrap().unwrap();
    assert_eq!(fetched.outputs, json!({"vpc_id": "vpc-2"}));

    let all = repo.list(&org, "network").await.unwrap();
    assert_eq!(all.len(), 1, "put must overwrite, not append");
}

/// `delete` removes a published record; deleting a missing key is a no-op.
pub async fn delete_removes_record(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    let record = record(&org, "network", &["dome:prod"], json!({}));
    repo.put(&record).await.unwrap();

    repo.delete(&record.key()).await.unwrap();
    assert!(repo.get(&record.key()).await.unwrap().is_none());

    // Deleting again (already-missing) must not error.
    repo.delete(&record.key()).await.unwrap();
}

/// `list` returns every record published for a unit, across every
/// dims/ext combination, but not another unit's records.
pub async fn list_returns_every_record_for_unit(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    repo.put(&record(&org, "network", &["dome:prod"], json!({"a": 1})))
        .await
        .unwrap();
    repo.put(&record(&org, "network", &["dome:staging"], json!({"a": 2})))
        .await
        .unwrap();
    repo.put(&record(&org, "other_unit", &["dome:prod"], json!({"a": 3})))
        .await
        .unwrap();

    let records = repo.list(&org, "network").await.unwrap();
    assert_eq!(records.len(), 2);
}

/// Records are scoped to their own org: publishing under one org must not
/// make them visible under another, even for the same unit/dims.
pub async fn records_are_scoped_to_their_own_org(repo: Arc<dyn UnitStateRepository>) {
    let org_a = unique_org();
    let org_b = unique_org();
    repo.put(&record(&org_a, "network", &["dome:prod"], json!({"a": 1})))
        .await
        .unwrap();
    repo.put(&record(&org_b, "network", &["dome:prod"], json!({"a": 2})))
        .await
        .unwrap();

    let a_records = repo.list(&org_a, "network").await.unwrap();
    let b_records = repo.list(&org_b, "network").await.unwrap();
    assert_eq!(a_records.len(), 1);
    assert_eq!(b_records.len(), 1);
    assert_eq!(a_records[0].outputs, json!({"a": 1}));
    assert_eq!(b_records[0].outputs, json!({"a": 2}));
}

/// A `UnitStateKey` built with dims/ext in a different order must resolve
/// to the same published record (`UnitStateKey::new` normalizes order).
pub async fn get_is_order_independent_for_dims(repo: Arc<dyn UnitStateRepository>) {
    let org = unique_org();
    let record = record(&org, "network", &["dome:prod", "env:prod"], json!({"a": 1}));
    repo.put(&record).await.unwrap();

    let reordered_key = UnitStateKey::new(
        &org,
        "network",
        vec!["env:prod".to_string(), "dome:prod".to_string()],
        vec![],
    );
    let fetched = repo.get(&reordered_key).await.unwrap();
    assert!(fetched.is_some(), "dims order must not affect lookup");
}
