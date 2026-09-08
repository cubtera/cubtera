//! `BindingUseCase`: P5 (docs/specs/2026-09-03-cubtera-v3-architecture.md
//! section 5.4). A [`Binding`] names a unit and a [`Selector`] over the
//! inventory; `expand` walks every leaf dimension name, resolves its
//! full ancestor chain through [`ResolveUseCase`], builds the per-leaf
//! [`SelectorContext`], and turns every match into an [`InstanceId`].
//! `status` diffs that desired set against `Store::list_instances` to
//! report drift, and `group_by_wave` batches several bindings by their
//! declared `wave` for ordered rollout.
//!
//! No auto-DAG here either, same as `[inputs]`/`[outputs]` (see the
//! crate-level notes): `wave` is an operator-assigned ordering hint on
//! `Binding` itself, never inferred from a unit's `[inputs]` declarations.

use crate::error::AppResult;
use crate::resolve::ResolveUseCase;
use crate::run::compute_package;
use cubtera_kernel::{DimRef, Ident, InstanceId};
use cubtera_model::{Binding, SelectorContext};
use cubtera_source::SourceRepo;
use cubtera_store::Store;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Where one candidate `InstanceId` currently stands relative to a
/// [`Binding`]'s desired state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DriftState {
    /// Matches the binding's selector, but `Store` has never recorded an
    /// instance for it - it should exist and doesn't yet.
    Desired,
    /// Matches the selector and `Store`'s recorded `unit_package` hash
    /// equals what the unit's files hash to right now.
    UpToDate,
    /// Matches the selector, `Store` has a recorded instance, but its
    /// `unit_package` hash no longer matches the unit's current files -
    /// the unit changed since the last successful apply.
    PackageDrifted,
    /// `Store` has a recorded instance for this unit that no longer
    /// matches the binding's selector (or was excluded) - it exists and
    /// arguably shouldn't anymore.
    Orphaned,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceDrift {
    pub id: InstanceId,
    pub state: DriftState,
}

pub struct BindingUseCase {
    resolve: ResolveUseCase,
    source: Arc<dyn SourceRepo>,
    store: Arc<dyn Store>,
    /// The dimension type at the bottom of the configured `dimRelations`
    /// chain (e.g. `dc`) - a `Binding` expands over every name of this
    /// type and walks each one's resolved ancestor chain, exactly the
    /// same "leaf dimension, full chain of ancestors" shape
    /// `run_support::prepare` builds for an ad hoc `cubtera plan -d ...`
    /// invocation.
    leaf_dim_type: Ident,
}

impl BindingUseCase {
    pub fn new(
        resolve: ResolveUseCase,
        source: Arc<dyn SourceRepo>,
        store: Arc<dyn Store>,
        leaf_dim_type: Ident,
    ) -> Self {
        Self {
            resolve,
            source,
            store,
            leaf_dim_type,
        }
    }

    /// Every `InstanceId` `binding` currently matches: for each name of
    /// the configured leaf dimension type, resolve its full ancestor
    /// chain, build a [`SelectorContext`] from it (one flat entry per
    /// dimension type, `"name"` injected), and keep it if
    /// `binding.matches` says yes.
    pub async fn expand(&self, org: &str, binding: &Binding) -> AppResult<Vec<InstanceId>> {
        let org_ident = Ident::parse(org)?;
        let leaf_names = self.resolve.list_names(org, &self.leaf_dim_type).await?;

        let mut matches = Vec::new();
        for leaf_name in leaf_names {
            let leaf_ident = Ident::parse(&leaf_name)?;
            let leaf = self
                .resolve
                .resolve(org, &self.leaf_dim_type, &leaf_ident)
                .await?;

            let mut ctx = SelectorContext::new();
            let mut dims = Vec::new();
            for dim_ref in &leaf.key_path {
                let dim = self
                    .resolve
                    .resolve(org, &dim_ref.dim_type, &dim_ref.name)
                    .await?;
                let mut fields = dim.sections.get("meta").cloned().unwrap_or_default();
                if let Some(obj) = fields.as_object_mut() {
                    obj.insert(
                        "name".to_string(),
                        serde_json::Value::String(dim_ref.name.to_string()),
                    );
                } else {
                    fields = serde_json::json!({ "name": dim_ref.name.to_string() });
                }
                ctx.insert(dim_ref.dim_type.to_string(), fields);
                dims.push(dim_ref.clone());
            }

            let id = InstanceId::try_new(
                org_ident.clone(),
                binding.unit.clone(),
                dims,
                Vec::<DimRef>::new(),
            )?;

            if binding.matches(&id, &ctx) {
                matches.push(id);
            }
        }
        Ok(matches)
    }

    /// Diff `binding`'s desired set against everything `Store` has on
    /// record for `binding.unit` in `org`: [`DriftState::Desired`] for a
    /// match never applied, [`DriftState::UpToDate`]/
    /// [`DriftState::PackageDrifted`] for a match that has been, and
    /// [`DriftState::Orphaned`] for a stored instance the selector no
    /// longer matches.
    pub async fn status(&self, org: &str, binding: &Binding) -> AppResult<Vec<InstanceDrift>> {
        let desired = self.expand(org, binding).await?;
        let desired_set: std::collections::BTreeSet<_> = desired.iter().cloned().collect();

        let org_ident = Ident::parse(org)?;
        let stored = self.store.list_instances(&org_ident).await?;
        let stored_for_unit: BTreeMap<InstanceId, _> = stored
            .into_iter()
            .filter(|inst| inst.id.unit() == &binding.unit)
            .map(|inst| (inst.id.clone(), inst))
            .collect();

        let current_package = compute_package(&self.source, binding.unit.as_str()).await?;

        let mut report = Vec::new();
        for id in &desired {
            let state = match stored_for_unit.get(id) {
                None => DriftState::Desired,
                Some(inst) if inst.unit_package == current_package.content_hash => {
                    DriftState::UpToDate
                }
                Some(_) => DriftState::PackageDrifted,
            };
            report.push(InstanceDrift {
                id: id.clone(),
                state,
            });
        }
        for id in stored_for_unit.keys() {
            if !desired_set.contains(id) {
                report.push(InstanceDrift {
                    id: id.clone(),
                    state: DriftState::Orphaned,
                });
            }
        }
        Ok(report)
    }
}

/// Group several bindings by their `wave` (ascending), expanding each into
/// its `InstanceId`s and unioning duplicates within a wave - the ordering
/// primitive `cubtera apply -s ... --waves` (CLI, not yet wired - see the
/// crate-level P5 note) drives batches from.
pub async fn group_by_wave(
    uc: &BindingUseCase,
    org: &str,
    bindings: &[Binding],
) -> AppResult<Vec<(u32, Vec<InstanceId>)>> {
    let mut by_wave: BTreeMap<u32, std::collections::BTreeSet<InstanceId>> = BTreeMap::new();
    for binding in bindings {
        let ids = uc.expand(org, binding).await?;
        by_wave.entry(binding.wave).or_default().extend(ids);
    }
    Ok(by_wave
        .into_iter()
        .map(|(wave, ids)| (wave, ids.into_iter().collect()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{InventoryPort, RawSections};
    use async_trait::async_trait;
    use cubtera_model::Selector;
    use cubtera_source::FsSource;
    use cubtera_store::SqliteStore;
    use serde_json::{json, Value};
    use std::sync::Mutex;
    use tempfile::TempDir;

    type Data = BTreeMap<String, BTreeMap<String, BTreeMap<String, RawSections>>>;

    struct FakeInventory {
        data: Mutex<Data>,
    }

    impl FakeInventory {
        fn new() -> Self {
            Self {
                data: Mutex::new(BTreeMap::new()),
            }
        }

        fn insert(&self, org: &str, dim_type: &str, name: &str, sections: RawSections) {
            self.data
                .lock()
                .unwrap()
                .entry(org.to_string())
                .or_default()
                .entry(dim_type.to_string())
                .or_default()
                .insert(name.to_string(), sections);
        }
    }

    fn sections(v: Value) -> RawSections {
        match v {
            Value::Object(m) => m.into_iter().collect(),
            _ => panic!("expected object"),
        }
    }

    #[async_trait]
    impl InventoryPort for FakeInventory {
        async fn get_raw(
            &self,
            org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(name))
                .cloned())
        }

        async fn get_raw_defaults(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(None)
        }

        async fn get_raw_schema(&self, _org: &str, _dim_type: &str) -> AppResult<Option<Value>> {
            Ok(None)
        }

        async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .map(|n| n.keys().filter(|k| !k.starts_with('.')).cloned().collect())
                .unwrap_or_default())
        }
    }

    fn ident(s: &str) -> Ident {
        Ident::parse(s).unwrap()
    }

    /// dome:prod -> env:{prod,stg} -> dc:{prod-use1, stg-use1}, `env`
    /// carrying `name`/status data through `meta` and `dc` carrying a
    /// `status` field, matching the shape sections 5.4/6's example
    /// selectors assume.
    async fn fixture() -> (Arc<FakeInventory>, TempDir, Arc<FsSource>) {
        let inv = FakeInventory::new();
        inv.insert("cubtera", "dome", "prod", sections(json!({"meta": {}})));
        inv.insert(
            "cubtera",
            "env",
            "prod",
            sections(json!({"meta": {"parent": "dome:prod"}})),
        );
        inv.insert(
            "cubtera",
            "env",
            "stg",
            sections(json!({"meta": {"parent": "dome:prod"}})),
        );
        inv.insert(
            "cubtera",
            "dc",
            "prod-use1",
            sections(json!({"meta": {"parent": "env:prod", "status": "active"}})),
        );
        inv.insert(
            "cubtera",
            "dc",
            "stg-use1",
            sections(json!({"meta": {"parent": "env:stg", "status": "draining"}})),
        );

        let tmp = TempDir::new().unwrap();
        let unit_dir = tmp.path().join("network");
        tokio::fs::create_dir_all(&unit_dir).await.unwrap();
        tokio::fs::write(unit_dir.join("manifest.toml"), b"type=\"bash\"")
            .await
            .unwrap();
        tokio::fs::write(unit_dir.join("run.sh"), b"#!/bin/sh\n")
            .await
            .unwrap();
        let source = Arc::new(FsSource::new(tmp.path()));

        (Arc::new(inv), tmp, source)
    }

    async fn use_case(inv: Arc<FakeInventory>, source: Arc<FsSource>) -> BindingUseCase {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        BindingUseCase::new(ResolveUseCase::new(inv), source, store, ident("dc"))
    }

    fn binding(selector: &str, wave: u32) -> Binding {
        Binding {
            id: "b1".to_string(),
            unit: ident("network"),
            selector: Selector::parse(selector).unwrap(),
            exclude: vec![],
            wave,
        }
    }

    #[tokio::test]
    async fn expand_matches_only_dcs_whose_ancestor_env_satisfies_the_selector() {
        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source).await;
        let b = binding("env.name == 'prod'", 0);

        let ids = uc.expand("cubtera", &b).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids[0]
            .dims()
            .iter()
            .any(|d| d.to_string() == "dc:prod-use1"));
    }

    #[tokio::test]
    async fn expand_can_select_on_the_leaf_dimension_itself() {
        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source).await;
        let b = binding("dc.status == 'draining'", 0);

        let ids = uc.expand("cubtera", &b).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids[0].dims().iter().any(|d| d.to_string() == "dc:stg-use1"));
    }

    #[tokio::test]
    async fn exclude_removes_an_otherwise_matching_instance() {
        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv.clone(), source.clone()).await;
        let all = uc
            .expand(
                "cubtera",
                &binding("dc.status in ['active', 'draining']", 0),
            )
            .await
            .unwrap();
        assert_eq!(all.len(), 2);

        let mut b = binding("dc.status in ['active', 'draining']", 0);
        b.exclude = vec![all[0].clone()];

        let ids = uc.expand("cubtera", &b).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], all[1]);
    }

    #[tokio::test]
    async fn status_reports_desired_for_a_match_never_applied() {
        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source).await;
        let b = binding("dc.status == 'active'", 0);

        let report = uc.status("cubtera", &b).await.unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].state, DriftState::Desired);
    }

    #[tokio::test]
    async fn status_reports_up_to_date_when_stored_package_hash_matches() {
        use cubtera_model::Instance;

        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source.clone()).await;
        let b = binding("dc.status == 'active'", 0);

        let ids = uc.expand("cubtera", &b).await.unwrap();
        let package = compute_package(&(source.clone() as Arc<dyn SourceRepo>), "network")
            .await
            .unwrap();
        uc.store
            .upsert_instance(&Instance::new(ids[0].clone(), package.content_hash), None)
            .await
            .unwrap();

        let report = uc.status("cubtera", &b).await.unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].state, DriftState::UpToDate);
    }

    #[tokio::test]
    async fn status_reports_package_drifted_after_unit_files_change() {
        use cubtera_model::Instance;

        let (inv, tmp, source) = fixture().await;
        let uc = use_case(inv, source.clone()).await;
        let b = binding("dc.status == 'active'", 0);

        let ids = uc.expand("cubtera", &b).await.unwrap();
        let package = compute_package(&(source.clone() as Arc<dyn SourceRepo>), "network")
            .await
            .unwrap();
        uc.store
            .upsert_instance(&Instance::new(ids[0].clone(), package.content_hash), None)
            .await
            .unwrap();

        tokio::fs::write(
            tmp.path().join("network").join("run.sh"),
            b"#!/bin/sh\necho hi\n",
        )
        .await
        .unwrap();

        let report = uc.status("cubtera", &b).await.unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].state, DriftState::PackageDrifted);
    }

    #[tokio::test]
    async fn status_reports_orphaned_for_a_stored_instance_no_longer_selected() {
        use cubtera_model::Instance;

        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source.clone()).await;

        // Applied while `stg-use1` was still draining and selected...
        let ids_before = uc
            .expand("cubtera", &binding("dc.status == 'draining'", 0))
            .await
            .unwrap();
        let package = compute_package(&(source.clone() as Arc<dyn SourceRepo>), "network")
            .await
            .unwrap();
        uc.store
            .upsert_instance(
                &Instance::new(ids_before[0].clone(), package.content_hash),
                None,
            )
            .await
            .unwrap();

        // ...but the binding we check against now only wants `active`.
        let report = uc
            .status("cubtera", &binding("dc.status == 'active'", 0))
            .await
            .unwrap();
        assert!(report
            .iter()
            .any(|d| d.id == ids_before[0] && d.state == DriftState::Orphaned));
    }

    #[tokio::test]
    async fn group_by_wave_orders_ascending_and_unions_duplicates() {
        let (inv, _tmp, source) = fixture().await;
        let uc = use_case(inv, source).await;

        let late = binding("dc.status == 'draining'", 10);
        let early = binding("dc.status in ['active', 'draining']", 1);

        let waves = group_by_wave(&uc, "cubtera", &[late, early]).await.unwrap();
        assert_eq!(waves.len(), 2);
        assert_eq!(waves[0].0, 1);
        assert_eq!(waves[0].1.len(), 2);
        assert_eq!(waves[1].0, 10);
        assert_eq!(waves[1].1.len(), 1);
    }
}
