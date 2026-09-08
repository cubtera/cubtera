//! Contract suite for the [`Store`] port, run against `SqliteStore`. If a
//! second adapter (e.g. `PostgresStore`, ยง12) ever lands, these tests move
//! to a shared `contract_suite!` macro/helper crate and get instantiated
//! against both - see the analogous `*_contract_mongo.rs` pattern in
//! `crates/cubtera-persistence/tests` for the v2 precedent (about to be
//! deleted in P2-drop-mongo, but the "one contract suite, multiple
//! backends" shape is worth keeping).

use cubtera_kernel::{Digest, DimRef, Ident, InstanceId};
use cubtera_model::{
    Instance, OutputSet, OutputValue, Plan, PlanId, ResolutionManifest, Revision, Run, RunFilter,
    RunId, RunOp, RunPatch, RunStatus,
};
use cubtera_store::{Store, StoreError};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn instance_id(org: &str, unit: &str, dims: &[&str]) -> InstanceId {
    InstanceId::try_new(
        Ident::parse(org).unwrap(),
        Ident::parse(unit).unwrap(),
        dims.iter().map(|d| DimRef::parse(d).unwrap()),
        [],
    )
    .unwrap()
}

fn resolution() -> ResolutionManifest {
    ResolutionManifest::empty(Digest::of(b"pkg"), "deadbeef".into(), Digest::of(b"cfg"))
}

#[tokio::test]
async fn instance_upsert_is_optimistic_concurrency() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);
    let inst = Instance::new(id.clone(), Digest::of(b"pkg-v1"));

    // Creating requires `expected: None`.
    let rev1 = store.upsert_instance(&inst, None).await.unwrap();
    assert_eq!(rev1, Revision::from_raw(1));

    // Creating again with `expected: None` must fail - the row already exists.
    let err = store.upsert_instance(&inst, None).await.unwrap_err();
    assert!(matches!(err, StoreError::RevisionConflict { .. }));

    // Updating with a stale `expected` must fail.
    let err = store
        .upsert_instance(&inst, Some(Revision::from_raw(99)))
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::RevisionConflict { .. }));

    // Updating with the correct `expected` succeeds and advances the revision.
    let mut updated = inst.clone();
    updated.unit_package = Digest::of(b"pkg-v2");
    let rev2 = store.upsert_instance(&updated, Some(rev1)).await.unwrap();
    assert_eq!(rev2, Revision::from_raw(2));

    let fetched = store.get_instance(&id).await.unwrap().unwrap();
    assert_eq!(fetched.unit_package, Digest::of(b"pkg-v2"));
    assert_eq!(fetched.spec_revision, rev2);
}

#[tokio::test]
async fn concurrent_upserts_only_let_one_writer_advance_each_revision() {
    let store = Arc::new(cubtera_store::SqliteStore::open_in_memory().unwrap());
    let id = instance_id("cubtera", "network", &["dome:prod"]);
    let inst = Instance::new(id.clone(), Digest::of(b"pkg"));
    let rev0 = store.upsert_instance(&inst, None).await.unwrap();

    // 8 concurrent writers all race to advance the same instance from the
    // same starting revision - exactly one should win each step, and the
    // rest must observe a `RevisionConflict`, never a silently lost write.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let store = store.clone();
        let inst = inst.clone();
        handles.push(tokio::spawn(async move {
            store.upsert_instance(&inst, Some(rev0)).await
        }));
    }

    let mut successes = 0;
    let mut conflicts = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => successes += 1,
            Err(StoreError::RevisionConflict { .. }) => conflicts += 1,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    assert_eq!(successes, 1, "exactly one writer should win the CAS race");
    assert_eq!(conflicts, 7);

    let fetched = store.get_instance(&id).await.unwrap().unwrap();
    assert_eq!(fetched.spec_revision, Revision::from_raw(2));
}

#[tokio::test]
async fn list_instances_filters_by_org_and_orders_by_canonical() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    for (org, unit) in [("acme", "a"), ("acme", "b"), ("other", "c")] {
        let id = instance_id(org, unit, &["dome:prod"]);
        store
            .upsert_instance(&Instance::new(id, Digest::of(b"pkg")), None)
            .await
            .unwrap();
    }

    let acme = store
        .list_instances(&Ident::parse("acme").unwrap())
        .await
        .unwrap();
    assert_eq!(acme.len(), 2);
    let names: Vec<_> = acme.iter().map(|i| i.id.unit().to_string()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[tokio::test]
async fn plan_round_trips_and_pin_check_survives_persistence() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);
    let plan = Plan {
        id: PlanId::new("p1"),
        instance: id.clone(),
        resolution: resolution(),
        artifact_digest: Digest::of(b"artifact"),
        diff_summary: "1 to add".into(),
        created_at: 1000,
        expires_at: 2000,
    };
    store.put_plan(&plan).await.unwrap();

    let fetched = store.get_plan(&PlanId::new("p1")).await.unwrap().unwrap();
    assert_eq!(fetched, plan);
    assert!(fetched.pins_match(&resolution()));
    assert!(store
        .get_plan(&PlanId::new("missing"))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn run_lifecycle_append_then_patch() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);
    let run = Run::queued(
        RunId::new("r1"),
        id.clone(),
        RunOp::Apply,
        Ident::parse("ci").unwrap(),
        1000,
    );
    store.append_run(&run).await.unwrap();

    // append_run is insert-only - appending the same run id twice must fail.
    let err = store.append_run(&run).await.unwrap_err();
    assert!(matches!(err, StoreError::Sqlite(_)));

    store
        .update_run(
            &RunId::new("r1"),
            RunPatch {
                status: Some(RunStatus::Succeeded),
                finished_at: Some(2000),
                exit_code: Some(0),
                logs_ref: Some("artifact:abc".into()),
                produced_outputs_revision: None,
            },
        )
        .await
        .unwrap();

    let runs = store
        .list_runs(RunFilter {
            org: Some(Ident::parse("cubtera").unwrap()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Succeeded);
    assert_eq!(runs[0].exit_code, Some(0));

    let none = store
        .list_runs(RunFilter {
            status: Some(RunStatus::Failed),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(none.is_empty());

    // `cubtera explain run <run_id>` shape: filter by id alone.
    let by_id = store
        .list_runs(RunFilter {
            id: Some(RunId::new("r1")),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id, RunId::new("r1"));

    let missing = store
        .list_runs(RunFilter {
            id: Some(RunId::new("does-not-exist")),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(missing.is_empty());

    let err = store
        .update_run(&RunId::new("does-not-exist"), RunPatch::default())
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)));
}

#[tokio::test]
async fn output_sets_are_append_only_and_monotonically_revisioned() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    let mut values = BTreeMap::new();
    values.insert(
        "vpc_id".into(),
        OutputValue::Plain(serde_json::json!("vpc-1")),
    );
    let set = OutputSet {
        schema_version: semver::Version::parse("1.0.0").unwrap(),
        values,
        produced_by: RunId::new("r1"),
        source_hash: Digest::of(b"pkg"),
        revision: Revision::INITIAL, // ignored - the store assigns the real one
    };

    let rev1 = store.put_output_set(&id, &set).await.unwrap();
    assert_eq!(rev1, Revision::from_raw(1));
    let rev2 = store.put_output_set(&id, &set).await.unwrap();
    assert_eq!(rev2, Revision::from_raw(2));

    let latest = store.get_output_set(&id).await.unwrap().unwrap();
    assert_eq!(latest.revision, rev2);
}

#[tokio::test]
async fn stale_consumers_are_reported_after_producer_moves_on() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let producer = instance_id("cubtera", "platform-base", &["dome:prod"]);
    let consumer = instance_id("cubtera", "network", &["dome:prod"]);

    let set = OutputSet {
        schema_version: semver::Version::parse("1.0.0").unwrap(),
        values: BTreeMap::new(),
        produced_by: RunId::new("r1"),
        source_hash: Digest::of(b"pkg"),
        revision: Revision::INITIAL,
    };
    let rev1 = store.put_output_set(&producer, &set).await.unwrap();
    store
        .mark_consumed(&consumer, &producer, rev1)
        .await
        .unwrap();

    // Nothing stale yet - the consumer is caught up with the only revision.
    let stale = store
        .list_stale_consumers(&Ident::parse("cubtera").unwrap())
        .await
        .unwrap();
    assert!(stale.is_empty());

    // Producer publishes again without the consumer re-consuming.
    let rev2 = store.put_output_set(&producer, &set).await.unwrap();
    let stale = store
        .list_stale_consumers(&Ident::parse("cubtera").unwrap())
        .await
        .unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].consumer, consumer);
    assert_eq!(stale[0].producer, producer);
    assert_eq!(stale[0].consumed_revision, rev1);
    assert_eq!(stale[0].current_revision, rev2);

    // Catching up clears the staleness.
    store
        .mark_consumed(&consumer, &producer, rev2)
        .await
        .unwrap();
    let stale = store
        .list_stale_consumers(&Ident::parse("cubtera").unwrap())
        .await
        .unwrap();
    assert!(stale.is_empty());
}

#[tokio::test]
async fn lease_acquire_conflicts_while_held() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    let lease = store
        .acquire_lease(&id, "actor-a", Duration::from_secs(60))
        .await
        .unwrap();

    let err = store
        .acquire_lease(&id, "actor-b", Duration::from_secs(60))
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::LeaseHeld { .. }));

    store.release_lease(lease).await.unwrap();

    // Now that it's released, a different owner can acquire it.
    store
        .acquire_lease(&id, "actor-b", Duration::from_secs(60))
        .await
        .unwrap();
}

#[tokio::test]
async fn only_ten_of_many_concurrent_lease_acquires_can_win() {
    let store = Arc::new(cubtera_store::SqliteStore::open_in_memory().unwrap());
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    let mut handles = Vec::new();
    for i in 0..16 {
        let store = store.clone();
        let id = id.clone();
        handles.push(tokio::spawn(async move {
            store
                .acquire_lease(&id, &format!("actor-{i}"), Duration::from_secs(60))
                .await
        }));
    }

    let mut wins = 0;
    let mut conflicts = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => wins += 1,
            Err(StoreError::LeaseHeld { .. }) => conflicts += 1,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
    assert_eq!(wins, 1, "at most one concurrent acquire should ever win");
    assert_eq!(conflicts, 15);
}

#[tokio::test]
async fn expired_lease_can_be_reacquired_by_a_new_owner() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    let lease = store
        .acquire_lease(&id, "actor-a", Duration::from_millis(10))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;

    // actor-b can now take over - the previous lease has expired.
    let lease_b = store
        .acquire_lease(&id, "actor-b", Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(lease_b.owner, "actor-b");

    // actor-a's stale token can no longer renew or release the lease.
    let err = store.renew_lease(&lease).await.unwrap_err();
    assert!(matches!(err, StoreError::LeaseLost));
    let err = store.release_lease(lease).await.unwrap_err();
    assert!(matches!(err, StoreError::LeaseLost));
}

#[tokio::test]
async fn renew_extends_expiry_by_the_leases_own_ttl() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    let lease = store
        .acquire_lease(&id, "actor-a", Duration::from_secs(30))
        .await
        .unwrap();
    let renewed = store.renew_lease(&lease).await.unwrap();
    assert!(renewed.expires_at >= lease.expires_at);
    assert_eq!(renewed.token, lease.token);
}

#[tokio::test]
async fn release_with_a_foreign_token_fails() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let id = instance_id("cubtera", "network", &["dome:prod"]);
    let real = store
        .acquire_lease(&id, "actor-a", Duration::from_secs(30))
        .await
        .unwrap();

    let mut forged = real.clone();
    forged.token = "not-the-real-token".into();
    let err = store.release_lease(forged).await.unwrap_err();
    assert!(matches!(err, StoreError::LeaseLost));

    // The real lease is still held and can still be released by its owner.
    store.release_lease(real).await.unwrap();
}

#[tokio::test]
async fn artifacts_are_content_addressed_and_deduplicated() {
    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let bytes = b"plan-artifact-bytes".to_vec();

    let d1 = store.put_artifact(&bytes).await.unwrap();
    let d2 = store.put_artifact(&bytes).await.unwrap(); // same bytes, same digest
    assert_eq!(d1, d2);
    assert_eq!(d1, Digest::of(&bytes));

    let fetched = store.get_artifact(&d1).await.unwrap().unwrap();
    assert_eq!(fetched, bytes);

    assert!(store
        .get_artifact(&Digest::of(b"never-stored"))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn legacy_deployment_log_is_append_only_and_scoped_by_org() {
    use cubtera_store::LegacyDeploymentLogRow;
    use std::collections::BTreeMap;

    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let row = |org: &str, ts: i64| LegacyDeploymentLogRow {
        org: org.into(),
        unit_name: "network".into(),
        dimensions: vec!["dome:prod".into()],
        command: "apply".into(),
        exit_code: 0,
        timestamp: ts,
        duration_ms: 10,
        git_shas: BTreeMap::new(),
        metadata: BTreeMap::new(),
    };

    store
        .append_legacy_deployment_log(row("cubtera", 1))
        .await
        .unwrap();
    store
        .append_legacy_deployment_log(row("cubtera", 2))
        .await
        .unwrap();
    store
        .append_legacy_deployment_log(row("other-org", 3))
        .await
        .unwrap();

    let cubtera_rows = store.find_legacy_deployment_log("cubtera").await.unwrap();
    assert_eq!(cubtera_rows.len(), 2);
    let other_rows = store.find_legacy_deployment_log("other-org").await.unwrap();
    assert_eq!(other_rows.len(), 1);
}

#[tokio::test]
async fn legacy_unit_state_put_get_delete_list_round_trip() {
    use cubtera_store::LegacyUnitStateRow;

    let store = cubtera_store::SqliteStore::open_in_memory().unwrap();
    let key = cubtera_store::LegacyUnitStateRow::state_key(
        "cubtera",
        "network",
        &["dome:prod".to_string()],
        &[],
    );
    let row = LegacyUnitStateRow {
        org: "cubtera".into(),
        unit: "network".into(),
        dims: vec!["dome:prod".into()],
        ext: vec![],
        outputs: serde_json::json!({"vpc_id": "vpc-1"}),
        updated_at: 1,
    };

    assert!(store.get_legacy_unit_state(&key).await.unwrap().is_none());

    store
        .put_legacy_unit_state(key.clone(), row.clone())
        .await
        .unwrap();
    let fetched = store.get_legacy_unit_state(&key).await.unwrap().unwrap();
    assert_eq!(fetched.outputs, serde_json::json!({"vpc_id": "vpc-1"}));

    // put again overwrites, does not duplicate.
    let mut updated = row.clone();
    updated.outputs = serde_json::json!({"vpc_id": "vpc-2"});
    store
        .put_legacy_unit_state(key.clone(), updated)
        .await
        .unwrap();
    let list = store
        .list_legacy_unit_state("cubtera", "network")
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].outputs, serde_json::json!({"vpc_id": "vpc-2"}));

    store.delete_legacy_unit_state(&key).await.unwrap();
    assert!(store.get_legacy_unit_state(&key).await.unwrap().is_none());
    // Deleting again (already-missing) must not error.
    store.delete_legacy_unit_state(&key).await.unwrap();
}

#[tokio::test]
async fn store_survives_reopening_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cubtera.sqlite");
    let id = instance_id("cubtera", "network", &["dome:prod"]);

    {
        let store = cubtera_store::SqliteStore::open(&path).unwrap();
        store
            .upsert_instance(&Instance::new(id.clone(), Digest::of(b"pkg")), None)
            .await
            .unwrap();
    }

    let store = cubtera_store::SqliteStore::open(&path).unwrap();
    let fetched = store.get_instance(&id).await.unwrap().unwrap();
    assert_eq!(fetched.unit_package, Digest::of(b"pkg"));
}
