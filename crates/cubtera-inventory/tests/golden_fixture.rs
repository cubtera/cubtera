//! Golden tests against `example/inventory` (the same fixture
//! `cubtera-persistence/tests/golden_inventory.rs` pins for v2) - proof
//! that `FsInventoryPort`/`FsUnitPort` read the on-disk naming convention
//! identically to the v2 adapter they're meant to eventually replace.
//! If this test needs to change, the naming convention itself changed.

use cubtera_app::ports::InventoryPort;
use cubtera_inventory::{FsInventoryPort, FsUnitPort};
use std::path::PathBuf;

fn inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../example/inventory")
}

fn units_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../example/units")
}

fn port() -> FsInventoryPort {
    FsInventoryPort::new(inventory_path())
}

#[tokio::test]
async fn dc_bare_json_and_defaults_gap_fill_data() {
    let port = port();
    let raw = port.get_raw("cubtera", "dc", "prod-use1").await.unwrap();
    assert!(raw.is_some(), "prod-use1 should have its own meta record");

    let defaults = port
        .get_raw_defaults("cubtera", "dc")
        .await
        .unwrap()
        .expect("dc should have .default:meta.json");
    assert!(defaults["meta"].get("region").is_some());
}

#[tokio::test]
async fn dc_own_region_overrides_when_present() {
    let port = port();
    let raw = port
        .get_raw("cubtera", "dc", "stg1-use2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw["meta"]["region"], "us-east-2");
}

#[tokio::test]
async fn service_named_section_is_exposed_as_a_separate_key() {
    let port = port();
    let raw = port
        .get_raw("cubtera", "service", "admin")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(raw["meta"]["owners"][0], "team1");
    assert_eq!(raw["manifest"]["cmd"], "node run admin");
}

#[tokio::test]
async fn service_without_own_manifest_has_no_manifest_section() {
    // The gap-fill itself is cubtera-model::Dimension::assemble's job, not
    // the port's - the raw port must report exactly what's on disk, no more.
    let port = port();
    let raw = port
        .get_raw("cubtera", "service", "app")
        .await
        .unwrap()
        .unwrap();
    assert!(
        !raw.contains_key("manifest"),
        "app has no app:manifest.json of its own - only .default supplies it"
    );

    let defaults = port
        .get_raw_defaults("cubtera", "service")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(defaults["manifest"]["cmd"], "node run app");
}

#[tokio::test]
async fn dc_has_a_schema() {
    let port = port();
    let schema = port
        .get_raw_schema("cubtera", "dc")
        .await
        .unwrap()
        .expect("example/inventory/cubtera/dc/.schema:meta.json should be picked up");
    assert!(schema.get("required").is_some());
}

#[tokio::test]
async fn dome_has_no_schema() {
    let port = port();
    assert!(port
        .get_raw_schema("cubtera", "dome")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn list_names_excludes_defaults_and_schema_and_sections() {
    let port = port();
    let names = port.list_names("cubtera", "service").await.unwrap();
    assert!(names.contains(&"admin".to_string()));
    assert!(names.contains(&"app".to_string()));
    assert!(!names.iter().any(|n| n.starts_with('.')));
}

#[tokio::test]
async fn includes_are_collected_for_a_dc_with_its_own_and_default_files() {
    let port = port();
    let own = port
        .list_includes("cubtera", "dc", "stg1-use2")
        .await
        .unwrap();
    let defaults = port.list_default_includes("cubtera", "dc").await.unwrap();

    // stg1-use2 has its own "extra" include with no default counterpart.
    assert!(own.iter().any(|i| i.name == "extra" && i.is_dir));
    // .default ships readme.txt/shared for every dc that doesn't override them.
    assert!(defaults.iter().any(|i| i.name == "readme.txt" && !i.is_dir));
    assert!(defaults.iter().any(|i| i.name == "shared" && i.is_dir));
}

#[tokio::test]
async fn prod_use1_has_no_includes_of_its_own() {
    let port = port();
    let own = port
        .list_includes("cubtera", "dc", "prod-use1")
        .await
        .unwrap();
    assert!(
        own.is_empty(),
        "prod-use1 ships no includes of its own: {own:?}"
    );
}

#[tokio::test]
async fn list_types_and_orgs_reflect_directory_layout() {
    let port = port();
    let types = port_list_types(&port).await;
    for expected in ["dome", "env", "dc", "service"] {
        assert!(
            types.contains(&expected.to_string()),
            "missing type {expected}"
        );
    }
}

// `InventoryPort` has no `list_types`/`list_orgs` (cubtera-app's use cases
// never needed them - `cubtera im get-types`/`config`'s org list still go
// through v2). Reading the directory names directly here just to assert
// the fixture layout is what the rest of this test file assumes.
async fn port_list_types(_port: &FsInventoryPort) -> Vec<String> {
    std::fs::read_dir(inventory_path().join("cubtera"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect()
}

#[tokio::test]
async fn fs_unit_port_finds_manifests_under_example_units() {
    let port = FsUnitPort::new(units_path());
    let units = port.list_units_for_test().await;
    assert!(units.contains(&"tf_unit02".to_string()));
    assert!(units.contains(&"bash_unit01".to_string()));
}

// `UnitPort::list_units` is async-trait, not directly callable without
// importing the trait - a tiny local extension avoids pulling in
// `cubtera_app::ports::UnitPort` just for this one assertion above.
#[async_trait::async_trait]
trait ListUnitsForTest {
    async fn list_units_for_test(&self) -> Vec<String>;
}

#[async_trait::async_trait]
impl ListUnitsForTest for FsUnitPort {
    async fn list_units_for_test(&self) -> Vec<String> {
        use cubtera_app::ports::UnitPort;
        self.list_units("cubtera").await.unwrap()
    }
}
