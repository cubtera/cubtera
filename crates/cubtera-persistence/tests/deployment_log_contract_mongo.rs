//! Runs the shared `DeploymentLogRepository` contract suite (see
//! `tests/support_dlog/mod.rs`) against `MongoDeploymentLogRepository`.
//!
//! Requires a real MongoDB instance. Point `CUBTERA_TEST_MONGO_URL` at one
//! (e.g. `mongodb://127.0.0.1:27017`) to run this file; without it, every
//! test skips itself with a message, same as `tests/inventory_contract_mongo.rs`.

#![cfg(feature = "mongodb")]

#[path = "support_dlog/mod.rs"]
mod support_dlog;

use cubtera_core::ports::DeploymentLogRepository;
use cubtera_persistence::mongodb::MongoDeploymentLogRepository;
use std::sync::Arc;

fn mongo_url() -> Option<String> {
    std::env::var("CUBTERA_TEST_MONGO_URL").ok()
}

/// Build a fresh repository against a database dedicated to this contract
/// suite. Entries are already scoped by their own `org` field within it
/// (see `entries_are_scoped_to_their_own_org`), so one shared
/// database/collection is fine across every test in this file.
async fn fresh_repo(url: &str) -> Arc<dyn DeploymentLogRepository> {
    let repo = MongoDeploymentLogRepository::new(url, "cubtera_dlog_contract_tests", "deployments")
        .await
        .expect("failed to connect to CUBTERA_TEST_MONGO_URL");
    Arc::new(repo)
}

macro_rules! mongo_contract_test {
    ($name:ident) => {
        #[tokio::test]
        async fn $name() {
            let Some(url) = mongo_url() else {
                eprintln!(
                    "skipping {}: set CUBTERA_TEST_MONGO_URL to run MongoDB contract tests",
                    stringify!($name)
                );
                return;
            };
            support_dlog::$name(fresh_repo(&url).await).await;
        }
    };
}

mongo_contract_test!(save_makes_entry_findable);
mongo_contract_test!(find_on_org_with_no_entries_returns_empty);
mongo_contract_test!(find_filters_by_unit_name);
mongo_contract_test!(find_filters_by_dimension);
mongo_contract_test!(find_orders_newest_first);
mongo_contract_test!(find_respects_limit);
mongo_contract_test!(find_by_dimensions_requires_every_dimension);
mongo_contract_test!(entries_are_scoped_to_their_own_org);
