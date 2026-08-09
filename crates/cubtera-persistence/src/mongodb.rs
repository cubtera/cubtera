//! MongoDB persistence adapter (wave 2)
//!
//! `MongoInventoryRepository` implements the same [`InventoryRepository`]
//! port as [`crate::fs::FsInventoryRepository`], so it gets `DimensionService`'s
//! defaults gap-fill/parent-chain/hashing behavior "for free" instead of
//! re-implementing it - the entire reason the port returns [`RawDimension`]
//! instead of an assembled `Dimension`. It passes the same contract test
//! suite as the FS adapter (`crates/cubtera-persistence/tests/support/mod.rs`),
//! run against a real MongoDB instance when `$CUBTERA_TEST_MONGO_URL` is set
//! - see `tests/inventory_contract_mongo.rs`.
//!
//! ## Document shape
//!
//! One document per record (a real dimension, or a reserved `.default`/
//! `.schema` record) in database `{org}`, collection `{dim_type}`:
//!
//! ```json
//! { "name": "prod-use1", "sections": { "meta": { "region": "us-east-1" } } }
//! ```
//!
//! `includes` (file/folder attachments) are not stored - same limitation as
//! `FsInventoryRepository::save_raw`, which only persists JSON sections.
//! Includes are inherently file-based; a document store round-tripping them
//! would need a different transport (GridFS, S3, ...) which is out of scope
//! here.
//!
//! `list_orgs`/`list_types` enumerate database/collection names, filtering
//! out MongoDB's own system databases.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{
    entry_matches, matches_all_dimensions, DeploymentLogEntry, DeploymentLogRepository,
    InventoryRepository, UnitStateRepository,
};
use cubtera_domain::{RawDimension, UnitStateKey, UnitStateRecord};
use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::{Client, Collection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Databases that are never a Cubtera org, regardless of what's configured.
const RESERVED_DATABASES: &[&str] = &["admin", "local", "config"];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DimDoc {
    name: String,
    #[serde(default)]
    sections: HashMap<String, Value>,
}

impl From<&RawDimension> for DimDoc {
    fn from(raw: &RawDimension) -> Self {
        Self {
            name: raw.name.clone(),
            sections: raw.sections.clone(),
        }
    }
}

impl From<DimDoc> for RawDimension {
    fn from(doc: DimDoc) -> Self {
        let mut raw = RawDimension::new(doc.name);
        for (section, value) in doc.sections {
            raw = raw.with_section(section, value);
        }
        raw
    }
}

/// MongoDB-backed inventory repository
pub struct MongoInventoryRepository {
    client: Client,
}

impl MongoInventoryRepository {
    /// Connect to `connection_string`. `_separator` is accepted for
    /// signature parity with [`crate::fs::FsInventoryRepository::with_separator`]
    /// but unused: Mongo documents are keyed by their bare `name` field, so
    /// there's no file-name suffix convention to configure.
    pub async fn new(connection_string: &str, _separator: String) -> Result<Self, String> {
        let client = Client::with_uri_str(connection_string)
            .await
            .map_err(|e| format!("Failed to connect to MongoDB: {e}"))?;
        Ok(Self { client })
    }

    fn collection(&self, org: &str, dim_type: &str) -> Collection<Document> {
        self.client.database(org).collection(dim_type)
    }

    async fn find_record(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawDimension>> {
        let filter = doc! { "name": name };
        let found = self
            .collection(org, dim_type)
            .find_one(filter)
            .await
            .map_err(|e| AppError::repository(format!("MongoDB find_one failed: {e}")))?;

        let Some(document) = found else {
            return Ok(None);
        };
        let dim_doc: DimDoc = mongodb::bson::from_document(document).map_err(|e| {
            AppError::repository(format!("Invalid document for {dim_type}:{name}: {e}"))
        })?;
        Ok(Some(dim_doc.into()))
    }
}

#[async_trait]
impl InventoryRepository for MongoInventoryRepository {
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawDimension>> {
        match self.find_record(org, dim_type, name).await? {
            // Matches FS semantics: a record only "exists" as a dimension if
            // it has its own "meta" section - defaults alone don't count.
            Some(raw) if raw.sections.contains_key("meta") => Ok(Some(raw)),
            _ => Ok(None),
        }
    }

    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>> {
        self.find_record(org, dim_type, ".default").await
    }

    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>> {
        self.find_record(org, dim_type, ".schema").await
    }

    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        let filter = doc! { "name": { "$not": { "$regex": "^[.#]" } } };
        let cursor = self
            .collection(org, dim_type)
            .find(filter)
            .await
            .map_err(|e| AppError::repository(format!("MongoDB find failed: {e}")))?;

        let docs: Vec<Document> = cursor
            .try_collect()
            .await
            .map_err(|e| AppError::repository(format!("MongoDB cursor failed: {e}")))?;

        let mut names: Vec<String> = docs
            .into_iter()
            .filter_map(|d| d.get_str("name").ok().map(|s| s.to_string()))
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    async fn list_types(&self, org: &str) -> AppResult<Vec<String>> {
        let mut types = self
            .client
            .database(org)
            .list_collection_names()
            .await
            .map_err(|e| {
                AppError::repository(format!("MongoDB list_collection_names failed: {e}"))
            })?;
        types.sort();
        Ok(types)
    }

    async fn list_orgs(&self) -> AppResult<Vec<String>> {
        let mut orgs = self.client.list_database_names().await.map_err(|e| {
            AppError::repository(format!("MongoDB list_database_names failed: {e}"))
        })?;
        orgs.retain(|name| !RESERVED_DATABASES.contains(&name.as_str()));
        orgs.sort();
        Ok(orgs)
    }

    async fn save_raw(&self, org: &str, dim_type: &str, raw: &RawDimension) -> AppResult<()> {
        let dim_doc = DimDoc::from(raw);
        let document = mongodb::bson::to_document(&dim_doc)
            .map_err(|e| AppError::repository(format!("Failed to encode {}: {e}", raw.name)))?;

        self.collection(org, dim_type)
            .replace_one(doc! { "name": &raw.name }, document)
            .upsert(true)
            .await
            .map_err(|e| AppError::repository(format!("MongoDB replace_one failed: {e}")))?;
        Ok(())
    }

    async fn delete_raw(&self, org: &str, dim_type: &str, name: &str) -> AppResult<()> {
        self.collection(org, dim_type)
            .delete_one(doc! { "name": name })
            .await
            .map_err(|e| AppError::repository(format!("MongoDB delete_one failed: {e}")))?;
        Ok(())
    }
}

/// MongoDB-backed deployment log repository: one document per entry, all
/// orgs sharing a single database/collection (`config.toml`'s
/// `[deploymentLog]` - `database`/`collection`, defaulting to `"cubtera"`/
/// `"deployments"`), distinguished by each document's own `org` field
/// (unlike inventory, which gets one database per org - dlog volume doesn't
/// usually warrant per-org databases, and v1 kept all orgs' logs together
/// too).
///
/// Filtering beyond `org` happens in-process via `cubtera_core::ports::
/// {entry_matches, matches_all_dimensions}` rather than a native Mongo
/// query - the same logic `FsDeploymentLogRepository` uses, so the two
/// backends can't drift on what `-q key:value` matches. This is a
/// reasonable tradeoff at wave-2's expected deployment-log volume; if it
/// ever needs to scale further, add indexed native queries here without
/// changing the port.
pub struct MongoDeploymentLogRepository {
    collection: Collection<DeploymentLogEntry>,
}

impl MongoDeploymentLogRepository {
    /// Connect to `connection_string` and use `database`/`collection` for
    /// every org's entries.
    pub async fn new(
        connection_string: &str,
        database: &str,
        collection: &str,
    ) -> Result<Self, String> {
        let client = Client::with_uri_str(connection_string)
            .await
            .map_err(|e| format!("Failed to connect to MongoDB: {e}"))?;
        Ok(Self {
            collection: client.database(database).collection(collection),
        })
    }

    async fn org_entries(&self, org: &str) -> AppResult<Vec<DeploymentLogEntry>> {
        let cursor = self
            .collection
            .find(doc! { "org": org })
            .await
            .map_err(|e| AppError::repository(format!("MongoDB find failed: {e}")))?;
        cursor
            .try_collect()
            .await
            .map_err(|e| AppError::repository(format!("MongoDB cursor failed: {e}")))
    }

    fn newest_first(
        mut entries: Vec<DeploymentLogEntry>,
        limit: Option<usize>,
    ) -> Vec<DeploymentLogEntry> {
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        if let Some(limit) = limit {
            entries.truncate(limit);
        }
        entries
    }
}

#[async_trait]
impl DeploymentLogRepository for MongoDeploymentLogRepository {
    async fn save(&self, entry: &DeploymentLogEntry) -> AppResult<()> {
        self.collection
            .insert_one(entry)
            .await
            .map_err(|e| AppError::repository(format!("MongoDB insert_one failed: {e}")))?;
        Ok(())
    }

    async fn find(
        &self,
        org: &str,
        query: &HashMap<String, String>,
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries = self
            .org_entries(org)
            .await?
            .into_iter()
            .filter(|entry| entry_matches(entry, query))
            .collect();
        Ok(Self::newest_first(entries, limit))
    }

    async fn find_by_dimensions(
        &self,
        org: &str,
        dimensions: &[String],
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries = self
            .org_entries(org)
            .await?
            .into_iter()
            .filter(|entry| matches_all_dimensions(entry, dimensions))
            .collect();
        Ok(Self::newest_first(entries, limit))
    }
}

/// On-disk (BSON) shape for a unit state record: the same fields as
/// [`UnitStateRecord`], plus a computed `dim_key` (== `UnitStateKey::canonical()`)
/// that `MongoUnitStateRepository` upserts/looks-up by - a single indexed
/// string comparison instead of an array-order-sensitive multi-field match
/// (`dims`/`ext` arrays would otherwise need to always be inserted in the
/// same sorted order to compare equal in a Mongo filter).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnitStateDoc {
    dim_key: String,
    org: String,
    unit: String,
    dims: Vec<String>,
    ext: Vec<String>,
    outputs: Value,
    updated_at: i64,
}

impl From<&UnitStateRecord> for UnitStateDoc {
    fn from(record: &UnitStateRecord) -> Self {
        Self {
            dim_key: record.key().canonical(),
            org: record.org.clone(),
            unit: record.unit.clone(),
            dims: record.dims.clone(),
            ext: record.ext.clone(),
            outputs: record.outputs.clone(),
            updated_at: record.updated_at,
        }
    }
}

impl From<UnitStateDoc> for UnitStateRecord {
    fn from(doc: UnitStateDoc) -> Self {
        Self {
            org: doc.org,
            unit: doc.unit,
            dims: doc.dims,
            ext: doc.ext,
            outputs: doc.outputs,
            updated_at: doc.updated_at,
        }
    }
}

/// MongoDB-backed unit state repository: one document per published
/// `UnitStateKey`, all orgs/units sharing a single database/collection
/// (`config.toml`'s `[unitState]` - `database`/`collection`, defaulting to
/// `"cubtera"`/`"unit_state"`), distinguished by each document's own
/// `org`/`unit`/`dim_key` fields - same "shared collection, filter by
/// fields" shape as [`MongoDeploymentLogRepository`].
pub struct MongoUnitStateRepository {
    collection: Collection<UnitStateDoc>,
}

impl MongoUnitStateRepository {
    /// Connect to `connection_string` and use `database`/`collection` for
    /// every org/unit's published state.
    pub async fn new(
        connection_string: &str,
        database: &str,
        collection: &str,
    ) -> Result<Self, String> {
        let client = Client::with_uri_str(connection_string)
            .await
            .map_err(|e| format!("Failed to connect to MongoDB: {e}"))?;
        Ok(Self {
            collection: client.database(database).collection(collection),
        })
    }
}

#[async_trait]
impl UnitStateRepository for MongoUnitStateRepository {
    async fn get(&self, key: &UnitStateKey) -> AppResult<Option<UnitStateRecord>> {
        let found = self
            .collection
            .find_one(doc! { "dim_key": key.canonical() })
            .await
            .map_err(|e| AppError::repository(format!("MongoDB find_one failed: {e}")))?;
        Ok(found.map(UnitStateRecord::from))
    }

    async fn put(&self, record: &UnitStateRecord) -> AppResult<()> {
        let unit_state_doc = UnitStateDoc::from(record);
        self.collection
            .replace_one(doc! { "dim_key": &unit_state_doc.dim_key }, &unit_state_doc)
            .upsert(true)
            .await
            .map_err(|e| AppError::repository(format!("MongoDB replace_one failed: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &UnitStateKey) -> AppResult<()> {
        self.collection
            .delete_one(doc! { "dim_key": key.canonical() })
            .await
            .map_err(|e| AppError::repository(format!("MongoDB delete_one failed: {e}")))?;
        Ok(())
    }

    async fn list(&self, org: &str, unit: &str) -> AppResult<Vec<UnitStateRecord>> {
        let cursor = self
            .collection
            .find(doc! { "org": org, "unit": unit })
            .await
            .map_err(|e| AppError::repository(format!("MongoDB find failed: {e}")))?;
        let docs: Vec<UnitStateDoc> = cursor
            .try_collect()
            .await
            .map_err(|e| AppError::repository(format!("MongoDB cursor failed: {e}")))?;
        Ok(docs.into_iter().map(UnitStateRecord::from).collect())
    }
}
