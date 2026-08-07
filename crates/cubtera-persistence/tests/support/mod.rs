//! Contract test suite for [`InventoryRepository`].
//!
//! These functions encode the *port* contract - behavior any adapter must
//! satisfy regardless of backend - as opposed to backend-specific naming
//! quirks (those belong in the adapter's own `#[cfg(test)]` unit tests, e.g.
//! `crates/cubtera-persistence/src/fs/dimension.rs`, or in the golden tests
//! against `example/inventory`).
//!
//! Each function takes a freshly constructed, empty repository and exercises
//! one behavior end to end. Run the same suite against every adapter by
//! writing one `tests/<backend>_inventory_contract.rs` per backend that
//! constructs a fresh repo per test and calls into this module - see
//! `tests/inventory_contract_fs.rs` for the FS instantiation. When the
//! MongoDB adapter lands (wave 2), add `tests/inventory_contract_mongo.rs`
//! with the same call sites against a fresh collection/database per test.

use cubtera_core::ports::InventoryRepository;
use cubtera_domain::RawDimension;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A fresh, unique org name per call. The FS adapter doesn't care (each test
/// already gets its own empty temp directory regardless of org name), but a
/// real MongoDB-backed run shares one live server across every scenario in
/// this suite - and possibly across repeated/parallel test runs - so reusing
/// a fixed "contract-org" name would let scenarios interfere with each
/// other's data (e.g. `list_names_reflects_saved_dimensions` would also see
/// dimensions saved by `save_get_roundtrip`).
fn unique_org() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("contract-org-{nanos}-{n}")
}

/// A dimension saved via `save_raw` must be readable back via `get_raw` with
/// the same sections.
pub async fn save_get_roundtrip(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let raw = RawDimension::new("prod-use1").with_section(
        "meta",
        json!({"region": "us-east-1", "vpc_cidr": "10.0.0.0/16"}),
    );

    repo.save_raw(&org, "dc", &raw).await.unwrap();

    let loaded = repo
        .get_raw(&org, "dc", "prod-use1")
        .await
        .unwrap()
        .expect("dimension should exist after save_raw");

    assert_eq!(loaded.sections["meta"]["region"], "us-east-1");
    assert_eq!(loaded.sections["meta"]["vpc_cidr"], "10.0.0.0/16");
}

/// A dimension with more than one section (e.g. `meta` + a custom
/// `manifest` section) must round-trip every section independently.
pub async fn save_get_roundtrip_multiple_sections(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let raw = RawDimension::new("admin")
        .with_section("meta", json!({"owners": ["team1"]}))
        .with_section("manifest", json!({"cmd": "node run admin"}));

    repo.save_raw(&org, "service", &raw).await.unwrap();

    let loaded = repo
        .get_raw(&org, "service", "admin")
        .await
        .unwrap()
        .expect("dimension should exist after save_raw");

    assert_eq!(loaded.sections["meta"]["owners"][0], "team1");
    assert_eq!(loaded.sections["manifest"]["cmd"], "node run admin");
}

/// Fetching a dimension that was never saved returns `None`, not an error.
pub async fn get_missing_dimension_returns_none(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let result = repo.get_raw(&org, "dc", "does-not-exist").await.unwrap();
    assert!(result.is_none());
}

/// Fetching defaults/schema for a type that never had one saved returns
/// `None`, not an error - these records are optional per type.
pub async fn get_defaults_and_schema_absent_by_default(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    assert!(repo.get_raw_defaults(&org, "dc").await.unwrap().is_none());
    assert!(repo.get_raw_schema(&org, "dc").await.unwrap().is_none());
}

/// `delete_raw` removes a previously saved dimension; subsequent `get_raw`
/// calls return `None` and it disappears from `list_names`.
pub async fn delete_removes_dimension(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let raw = RawDimension::new("stg1-use2").with_section("meta", json!({"region": "us-east-2"}));
    repo.save_raw(&org, "dc", &raw).await.unwrap();
    assert!(repo
        .get_raw(&org, "dc", "stg1-use2")
        .await
        .unwrap()
        .is_some());

    repo.delete_raw(&org, "dc", "stg1-use2").await.unwrap();

    assert!(repo
        .get_raw(&org, "dc", "stg1-use2")
        .await
        .unwrap()
        .is_none());
    assert!(!repo
        .list_names(&org, "dc")
        .await
        .unwrap()
        .contains(&"stg1-use2".to_string()));
}

/// Deleting a dimension that doesn't exist is a no-op, not an error.
pub async fn delete_missing_dimension_is_noop(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    repo.delete_raw(&org, "dc", "does-not-exist").await.unwrap();
}

/// `list_names` reflects every dimension saved under a type, and nothing else.
pub async fn list_names_reflects_saved_dimensions(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    for name in ["prod-use1", "prod-use2", "stg1-use2"] {
        let raw = RawDimension::new(name).with_section("meta", json!({}));
        repo.save_raw(&org, "dc", &raw).await.unwrap();
    }

    let mut names = repo.list_names(&org, "dc").await.unwrap();
    names.sort();
    assert_eq!(
        names,
        vec![
            "prod-use1".to_string(),
            "prod-use2".to_string(),
            "stg1-use2".to_string(),
        ]
    );
}

/// A type with nothing saved yet has an empty `list_names`, not an error.
pub async fn list_names_empty_for_unknown_type(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let names = repo.list_names(&org, "does-not-exist").await.unwrap();
    assert!(names.is_empty());
}

/// `list_types`/`list_orgs` reflect saved data: an org/type only shows up
/// once something has actually been saved under it.
pub async fn list_types_and_orgs_reflect_saved_data(repo: Arc<dyn InventoryRepository>) {
    let org = unique_org();
    let dc = RawDimension::new("prod-use1").with_section("meta", json!({}));
    repo.save_raw(&org, "dc", &dc).await.unwrap();
    let env = RawDimension::new("prod").with_section("meta", json!({}));
    repo.save_raw(&org, "env", &env).await.unwrap();

    let types = repo.list_types(&org).await.unwrap();
    assert!(types.contains(&"dc".to_string()));
    assert!(types.contains(&"env".to_string()));

    let orgs = repo.list_orgs().await.unwrap();
    assert!(orgs.contains(&org));
}
