//! SQLite-backed [`UnitStateRepository`], via `cubtera-store`'s
//! [`SqliteStore`] legacy seam. Replaces v2's FS-json/Mongo choice - unit
//! state (cross-unit `[outputs]`/`[inputs]`) is now always backed by the
//! same SQLite `Store` file as the deployment log.

use async_trait::async_trait;
use cubtera_core::error::AppResult;
use cubtera_core::ports::UnitStateRepository;
use cubtera_domain::{UnitStateKey, UnitStateRecord};
use cubtera_store::{LegacyUnitStateRow, SqliteStore, StoreError};
use std::sync::Arc;

pub struct SqliteUnitStateRepository {
    store: Arc<SqliteStore>,
}

impl SqliteUnitStateRepository {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }
}

fn to_app_error(e: StoreError) -> cubtera_core::error::AppError {
    cubtera_core::error::AppError::repository(format!("SQLite store error: {e}"))
}

fn to_row(record: &UnitStateRecord) -> LegacyUnitStateRow {
    LegacyUnitStateRow {
        org: record.org.clone(),
        unit: record.unit.clone(),
        dims: record.dims.clone(),
        ext: record.ext.clone(),
        outputs: record.outputs.clone(),
        updated_at: record.updated_at,
    }
}

fn from_row(row: LegacyUnitStateRow) -> UnitStateRecord {
    UnitStateRecord {
        org: row.org,
        unit: row.unit,
        dims: row.dims,
        ext: row.ext,
        outputs: row.outputs,
        updated_at: row.updated_at,
    }
}

#[async_trait]
impl UnitStateRepository for SqliteUnitStateRepository {
    async fn get(&self, key: &UnitStateKey) -> AppResult<Option<UnitStateRecord>> {
        let state_key = LegacyUnitStateRow::state_key(&key.org, &key.unit, &key.dims, &key.ext);
        let row = self
            .store
            .get_legacy_unit_state(&state_key)
            .await
            .map_err(to_app_error)?;
        Ok(row.map(from_row))
    }

    async fn put(&self, record: &UnitStateRecord) -> AppResult<()> {
        let key = record.key();
        let state_key = LegacyUnitStateRow::state_key(&key.org, &key.unit, &key.dims, &key.ext);
        self.store
            .put_legacy_unit_state(state_key, to_row(record))
            .await
            .map_err(to_app_error)
    }

    async fn delete(&self, key: &UnitStateKey) -> AppResult<()> {
        let state_key = LegacyUnitStateRow::state_key(&key.org, &key.unit, &key.dims, &key.ext);
        self.store
            .delete_legacy_unit_state(&state_key)
            .await
            .map_err(to_app_error)
    }

    async fn list(&self, org: &str, unit: &str) -> AppResult<Vec<UnitStateRecord>> {
        let rows = self
            .store
            .list_legacy_unit_state(org, unit)
            .await
            .map_err(to_app_error)?;
        Ok(rows.into_iter().map(from_row).collect())
    }
}
