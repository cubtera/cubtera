//! Runs the shared `UnitStateRepository` contract suite (see
//! `tests/support_unit_state/mod.rs`) against `MongoUnitStateRepository`.
//!
//! Requires a real MongoDB instance. Point `CUBTERA_TEST_MONGO_URL` at one
//! (e.g. `mongodb://127.0.0.1:27017`) to run this file; without it, every
//! test skips itself with a message, same as `tests/deployment_log_contract_mongo.rs`.

#![cfg(feature = "mongodb")]

#[path = "support_unit_state/mod.rs"]
mod support_unit_state;

use cubtera_core::ports::UnitStateRepository;
use cubtera_persistence::mongodb::MongoUnitStateRepository;
use std::sync::Arc;

fn mongo_url() -> Option<String> {
    std::env::var("CUBTERA_TEST_MONGO_URL").ok()
}

/// Build a fresh repository against a database dedicated to this contract
/// suite. Records are already scoped by their own `org` field within it
/// (see `records_are_scoped_to_their_own_org`), so one shared
/// database/collection is fine across every test in this file.
async fn fresh_repo(url: &str) -> Arc<dyn UnitStateRepository> {
    let repo =
        MongoUnitStateRepository::new(url, "cubtera_unit_state_contract_tests", "unit_state")
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
            support_unit_state::$name(fresh_repo(&url).await).await;
        }
    };
}

mongo_contract_test!(put_then_get_round_trips);
mongo_contract_test!(get_missing_key_returns_none);
mongo_contract_test!(put_overwrites_existing_record_for_same_key);
mongo_contract_test!(delete_removes_record);
mongo_contract_test!(list_returns_every_record_for_unit);
mongo_contract_test!(records_are_scoped_to_their_own_org);
mongo_contract_test!(get_is_order_independent_for_dims);
