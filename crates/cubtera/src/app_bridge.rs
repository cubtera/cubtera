//! Bridges v2's `cubtera_core::ports::InventoryRepository` onto v3's
//! `cubtera_app::InventoryPort`.
//!
//! `cubtera-app` cannot depend on `cubtera-core` (see the crate table in
//! docs/specs/2026-09-03-cubtera-v3-architecture.md section 3), so its `resolve`/
//! `validate` use cases (P3) need their own thin adapter to reuse the
//! existing `FsInventoryRepository` this CLI already constructs via
//! `Repositories::from_config` - there is no reason to stand up a second,
//! parallel FS-inventory adapter this early in the migration. Once v2's
//! `cubtera-core`/`cubtera-persistence` are retired (P7), this bridge goes
//! away along with them.

use async_trait::async_trait;
use cubtera_app::ports::RawSections;
use cubtera_app::{AppError, AppResult, InventoryPort};
use cubtera_core::ports::InventoryRepository;
use serde_json::Value;
use std::sync::Arc;

pub struct InventoryPortBridge {
    inner: Arc<dyn InventoryRepository>,
}

impl InventoryPortBridge {
    pub fn new(inner: Arc<dyn InventoryRepository>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl InventoryPort for InventoryPortBridge {
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawSections>> {
        let raw = self
            .inner
            .get_raw(org, dim_type, name)
            .await
            .map_err(backend_error)?;
        Ok(raw.map(sections_of))
    }

    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawSections>> {
        let raw = self
            .inner
            .get_raw_defaults(org, dim_type)
            .await
            .map_err(backend_error)?;
        Ok(raw.map(sections_of))
    }

    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<Value>> {
        let raw = self
            .inner
            .get_raw_schema(org, dim_type)
            .await
            .map_err(backend_error)?;
        Ok(raw.and_then(|r| r.sections.get("meta").cloned()))
    }

    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        self.inner
            .list_names(org, dim_type)
            .await
            .map_err(backend_error)
    }

    async fn list_includes(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
        let raw = self
            .inner
            .get_raw(org, dim_type, name)
            .await
            .map_err(backend_error)?;
        Ok(raw.map(includes_of).unwrap_or_default())
    }

    async fn list_default_includes(
        &self,
        org: &str,
        dim_type: &str,
    ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
        let raw = self
            .inner
            .get_raw_defaults(org, dim_type)
            .await
            .map_err(backend_error)?;
        Ok(raw.map(includes_of).unwrap_or_default())
    }
}

fn sections_of(raw: cubtera_domain::RawDimension) -> RawSections {
    raw.sections.into_iter().collect()
}

fn includes_of(raw: cubtera_domain::RawDimension) -> Vec<cubtera_model::IncludeEntry> {
    raw.includes
        .into_iter()
        .map(|i| cubtera_model::IncludeEntry {
            name: i.name,
            source: i.source,
            is_dir: i.is_dir,
        })
        .collect()
}

fn backend_error(e: cubtera_core::error::AppError) -> AppError {
    AppError::backend(e.to_string())
}
