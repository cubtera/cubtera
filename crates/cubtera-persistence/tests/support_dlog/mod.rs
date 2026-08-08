//! Contract test suite for [`DeploymentLogRepository`].
//!
//! Same idea as `tests/support/mod.rs` for `InventoryRepository`: these
//! functions encode the *port* contract, run against a freshly constructed
//! repository from `tests/deployment_log_contract_fs.rs` (always) and
//! `tests/deployment_log_contract_mongo.rs` (when `CUBTERA_TEST_MONGO_URL`
//! is set).

use cubtera_core::ports::{DeploymentLogEntry, DeploymentLogRepository};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A fresh, unique org name per call - see `tests/support/mod.rs`'s
/// `unique_org` for why this matters against a shared backing store like a
/// real MongoDB instance.
fn unique_org() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("contract-org-{nanos}-{n}")
}

fn entry(
    org: &str,
    unit_name: &str,
    dimensions: &[&str],
    command: &str,
    timestamp: i64,
) -> DeploymentLogEntry {
    DeploymentLogEntry {
        unit_name: unit_name.to_string(),
        org: org.to_string(),
        dimensions: dimensions.iter().map(|s| s.to_string()).collect(),
        command: command.to_string(),
        exit_code: 0,
        timestamp,
        duration_ms: 42,
        git_shas: HashMap::new(),
        metadata: HashMap::new(),
    }
}

/// An entry saved via `save` must show up in an unfiltered `find` for its org.
pub async fn save_makes_entry_findable(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 1))
        .await
        .unwrap();

    let results = repo.find(&org, &HashMap::new(), None).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].unit_name, "network");
}

/// `find` with no query on an org that never had anything saved returns an
/// empty list, not an error.
pub async fn find_on_org_with_no_entries_returns_empty(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    let results = repo.find(&org, &HashMap::new(), None).await.unwrap();
    assert!(results.is_empty());
}

/// `find` filters by unit name, exactly.
pub async fn find_filters_by_unit_name(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 1))
        .await
        .unwrap();
    repo.save(&entry(&org, "app", &["env:prod"], "apply", 2))
        .await
        .unwrap();

    let mut query = HashMap::new();
    query.insert("unit".to_string(), "network".to_string());
    let results = repo.find(&org, &query, None).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].unit_name, "network");
}

/// `find` filters by dimension key (`type:name`), matching entries whose
/// `dimensions` contains it.
pub async fn find_filters_by_dimension(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 1))
        .await
        .unwrap();
    repo.save(&entry(&org, "network", &["env:staging"], "apply", 2))
        .await
        .unwrap();

    let mut query = HashMap::new();
    query.insert("env".to_string(), "prod".to_string());
    let results = repo.find(&org, &query, None).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].timestamp, 1);
}

/// `find` results are ordered newest-first by timestamp.
pub async fn find_orders_newest_first(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 1))
        .await
        .unwrap();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 3))
        .await
        .unwrap();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 2))
        .await
        .unwrap();

    let results = repo.find(&org, &HashMap::new(), None).await.unwrap();
    let timestamps: Vec<i64> = results.iter().map(|e| e.timestamp).collect();
    assert_eq!(timestamps, vec![3, 2, 1]);
}

/// `find` truncates to `limit` most-recent entries.
pub async fn find_respects_limit(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    for i in 0..5 {
        repo.save(&entry(&org, "network", &["env:prod"], "apply", i))
            .await
            .unwrap();
    }

    let results = repo.find(&org, &HashMap::new(), Some(2)).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].timestamp, 4);
    assert_eq!(results[1].timestamp, 3);
}

/// `find_by_dimensions` requires every listed dimension to be present, not
/// just any of them.
pub async fn find_by_dimensions_requires_every_dimension(repo: Arc<dyn DeploymentLogRepository>) {
    let org = unique_org();
    repo.save(&entry(
        &org,
        "network",
        &["env:prod", "dc:use1"],
        "apply",
        1,
    ))
    .await
    .unwrap();
    repo.save(&entry(&org, "network", &["env:prod"], "apply", 2))
        .await
        .unwrap();

    let results = repo
        .find_by_dimensions(&org, &["env:prod".to_string(), "dc:use1".to_string()], None)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].timestamp, 1);
}

/// Entries are scoped to their own org: saving under one org must not make
/// them visible under another.
pub async fn entries_are_scoped_to_their_own_org(repo: Arc<dyn DeploymentLogRepository>) {
    let org_a = unique_org();
    let org_b = unique_org();
    repo.save(&entry(&org_a, "network", &["env:prod"], "apply", 1))
        .await
        .unwrap();
    repo.save(&entry(&org_b, "network", &["env:prod"], "apply", 2))
        .await
        .unwrap();

    let a_results = repo.find(&org_a, &HashMap::new(), None).await.unwrap();
    let b_results = repo.find(&org_b, &HashMap::new(), None).await.unwrap();

    assert_eq!(a_results.len(), 1);
    assert_eq!(b_results.len(), 1);
    assert_ne!(a_results[0].timestamp, b_results[0].timestamp);
}
