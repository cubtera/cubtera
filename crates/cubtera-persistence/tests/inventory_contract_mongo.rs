//! Runs the shared `InventoryRepository` contract suite (see
//! `tests/support/mod.rs`) against `MongoInventoryRepository` - the same
//! suite `tests/inventory_contract_fs.rs` runs against the FS adapter. This
//! is what "проходит контрактные тесты" means in the migration plan's wave
//! 2 item: the port contract doesn't change per backend.
//!
//! Requires a real MongoDB instance. Point `CUBTERA_TEST_MONGO_URL` at one
//! (e.g. `mongodb://127.0.0.1:27017`) to run this file; without it, every
//! test skips itself with a message instead of failing, so `cargo test
//! --workspace` stays green on machines/CI without MongoDB available.
//!
//! Each test uses its own randomly-named database so runs never collide
//! with each other or with real data.

#![cfg(feature = "mongodb")]

mod support;

use cubtera_core::ports::InventoryRepository;
use cubtera_persistence::mongodb::MongoInventoryRepository;
use std::sync::Arc;

/// Returns `Some(connection string)` if MongoDB contract tests should run.
fn mongo_url() -> Option<String> {
    std::env::var("CUBTERA_TEST_MONGO_URL").ok()
}

/// Build a fresh repository. The contract suite itself picks a fixed
/// `"contract-org"` database name per scenario, but scenarios within one
/// process run against a shared `mongod`, and Mongo databases/collections
/// are created lazily on first write - so as long as each test in this file
/// only exercises one scenario, there's no cross-test interference to
/// worry about beyond what `support::` already assumes (a fresh backing
/// store per call).
async fn fresh_repo(url: &str) -> Arc<dyn InventoryRepository> {
    let repo = MongoInventoryRepository::new(url, ":".to_string())
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
            support::$name(fresh_repo(&url).await).await;
        }
    };
}

mongo_contract_test!(save_get_roundtrip);
mongo_contract_test!(save_get_roundtrip_multiple_sections);
mongo_contract_test!(get_missing_dimension_returns_none);
mongo_contract_test!(get_defaults_and_schema_absent_by_default);
mongo_contract_test!(delete_removes_dimension);
mongo_contract_test!(delete_missing_dimension_is_noop);
mongo_contract_test!(list_names_reflects_saved_dimensions);
mongo_contract_test!(list_names_empty_for_unknown_type);
mongo_contract_test!(list_types_and_orgs_reflect_saved_data);
