//! Golden tests against `example/inventory` - the same fixture v1 uses.
//!
//! These pin the on-disk naming convention described in the migration plan:
//! meta wrapping, named sections (`admin:manifest.json`), `.default` gap-fill,
//! and parent-chain resolution across the `dome -> env -> dc` hierarchy.
//! If this test needs to change, the naming convention itself changed - and
//! that's a one-adapter change thanks to `InventoryRepository`.

use cubtera_core::services::DimensionService;
use cubtera_domain::DimHierarchy;
use cubtera_persistence::fs::FsInventoryRepository;
use std::path::PathBuf;
use std::sync::Arc;

fn inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../example/inventory")
}

fn service() -> DimensionService {
    let repo = FsInventoryRepository::new(inventory_path());
    DimensionService::new(Arc::new(repo), DimHierarchy::new(vec!["dome", "env", "dc"]))
}

#[tokio::test]
async fn dc_gap_fills_region_from_defaults() {
    let service = service();
    let dim = service
        .get_by_name("cubtera", "dc", "prod-use1")
        .await
        .unwrap();

    // prod-use1.json has no "region" of its own -> gap-filled from .default:meta.json
    assert_eq!(dim.meta().unwrap()["region"], "us-east-1");
    assert_eq!(dim.meta().unwrap()["vpc_cidr"], "10.11.0.0/16");
}

#[tokio::test]
async fn dc_own_region_wins_over_defaults() {
    let service = service();
    let dim = service
        .get_by_name("cubtera", "dc", "stg1-use2")
        .await
        .unwrap();

    assert_eq!(dim.meta().unwrap()["region"], "us-east-2");
}

#[tokio::test]
async fn dc_resolves_full_parent_chain() {
    let service = service();
    let dim = service
        .get_by_name("cubtera", "dc", "prod-use1")
        .await
        .unwrap();

    assert_eq!(dim.parent_ref, Some("env:prod".to_string()));
    assert_eq!(
        dim.key_path,
        vec![
            "dome:prod".to_string(),
            "env:prod".to_string(),
            "dc:prod-use1".to_string()
        ]
    );
}

#[tokio::test]
async fn service_named_section_is_exposed() {
    let service = service();
    let dim = service
        .get_by_name("cubtera", "service", "admin")
        .await
        .unwrap();

    // admin.json (bare) -> "meta" section
    assert_eq!(dim.meta().unwrap()["owners"][0], "team1");
    assert_eq!(dim.meta().unwrap()["owners"][1], "team2");

    // admin:manifest.json -> "manifest" section
    let manifest = dim
        .section("manifest")
        .expect("manifest section from admin:manifest.json");
    assert_eq!(manifest["cmd"], "node run admin");
    assert_eq!(manifest["prod"]["max_capacity"], 3);
}

#[tokio::test]
async fn service_gap_fills_manifest_from_defaults_when_missing() {
    let service = service();
    // app.json has no app:manifest.json -> "manifest" section comes entirely
    // from .default:manifest.json
    let dim = service
        .get_by_name("cubtera", "service", "app")
        .await
        .unwrap();

    let manifest = dim
        .section("manifest")
        .expect("manifest gap-filled from defaults");
    assert_eq!(manifest["cmd"], "node run app");
    assert_eq!(manifest["description"], "Default service description");
    assert_eq!(manifest["prod"]["max_capacity"], 10);
}

#[tokio::test]
async fn dc_children_of_env_are_resolved_via_hierarchy() {
    let service = service();
    let children = service
        .get_children("cubtera", "env", "prod")
        .await
        .unwrap();
    let names: Vec<&str> = children.iter().map(|d| d.name.as_str()).collect();

    assert!(names.contains(&"prod-use1"));
    assert!(names.contains(&"prod-use2"));
    assert!(names.contains(&"prod-euw1"));
}

#[tokio::test]
async fn list_names_excludes_defaults_and_schema() {
    let service = service();
    let names = service.get_all_names("cubtera", "service").await.unwrap();

    assert!(names.contains(&"admin".to_string()));
    assert!(names.contains(&"api".to_string()));
    assert!(!names.iter().any(|n| n.starts_with('.')));
}

#[tokio::test]
async fn list_types_and_orgs_reflect_directory_layout() {
    let service = service();
    let orgs = service.get_orgs().await.unwrap();
    assert!(orgs.contains(&"cubtera".to_string()));

    let types = service.get_types("cubtera").await.unwrap();
    for expected in ["dome", "env", "dc", "service", "mongodb"] {
        assert!(
            types.contains(&expected.to_string()),
            "missing type {expected}"
        );
    }
}

#[tokio::test]
async fn validate_reports_existing_and_missing_dimensions() {
    let service = service();
    assert!(service
        .validate("cubtera", "dc", "prod-use1")
        .await
        .unwrap());
    assert!(!service
        .validate("cubtera", "dc", "does-not-exist")
        .await
        .unwrap());
}

#[tokio::test]
async fn dc_has_a_schema_and_all_fixture_dcs_satisfy_it() {
    use cubtera_core::services::SchemaValidation;

    let service = service();
    let schema = service.get_schema("cubtera", "dc").await.unwrap();
    assert!(
        schema.is_some(),
        "example/inventory/cubtera/dc/.schema:meta.json should be picked up"
    );

    for name in service.get_all_names("cubtera", "dc").await.unwrap() {
        let outcome = service
            .validate_schema("cubtera", "dc", &name)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            SchemaValidation::Valid,
            "dc:{name} should satisfy .schema:meta.json (gap-filled `region` included)"
        );
    }
}

#[tokio::test]
async fn dome_has_no_schema() {
    use cubtera_core::services::SchemaValidation;

    let service = service();
    let outcome = service
        .validate_schema("cubtera", "dome", "prod")
        .await
        .unwrap();
    assert_eq!(outcome, SchemaValidation::NoSchema);
}
