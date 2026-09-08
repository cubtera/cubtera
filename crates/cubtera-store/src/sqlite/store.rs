use crate::error::{StoreError, StoreResult};
use crate::legacy::{LegacyDeploymentLogRow, LegacyUnitStateRow};
use crate::port::Store;
use async_trait::async_trait;
use cubtera_kernel::{Digest, Ident, InstanceId};
use cubtera_model::{
    Instance, Lease, OutputSet, Plan, PlanId, Revision, Run, RunFilter, RunId, RunPatch,
    StaleConsumer,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The default [`Store`] adapter: a single SQLite database file (or an
/// in-memory database for tests), accessed through a mutex + one dedicated
/// blocking task per call. `rusqlite`'s `Connection` is `Send` but not
/// `Sync`, so the mutex is what makes `SqliteStore` itself `Sync` -
/// `spawn_blocking` is what keeps every call off the async reactor thread
/// (the workspace's async-discipline rule: adapters must not block tokio).
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    /// Disambiguates lease tokens minted within the same millisecond -
    /// tokens only need to be unique, not cryptographically random.
    token_seq: Arc<AtomicU64>,
}

impl SqliteStore {
    /// Open (creating if necessary) a SQLite database at `path`, creating
    /// its parent directory too - callers hand this a config-derived path
    /// like `~/.cubtera/store.sqlite`, which usually doesn't exist yet on
    /// a fresh install.
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Backend(format!("failed to create {parent:?}: {e}")))?;
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> StoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> StoreResult<Self> {
        // `cubtera` is a short-lived CLI process invoked repeatedly against
        // the *same* on-disk database file (`~/.cubtera/store.sqlite` by
        // default) - unlike the in-process `Arc<Mutex<Connection>>` here,
        // there is no coordination between separate `cubtera` processes.
        // WAL lets readers and a writer proceed concurrently, and
        // `busy_timeout` makes a genuine writer/writer conflict retry for
        // a bit instead of immediately failing with `SQLITE_BUSY`.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        super::schema::apply(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            token_seq: Arc::new(AtomicU64::new(0)),
        })
    }

    fn next_token(&self, key: &InstanceId, owner: &str, now: i64) -> String {
        let seq = self.token_seq.fetch_add(1, Ordering::Relaxed);
        Digest::of_parts([
            key.digest().to_hex(),
            owner.to_string(),
            now.to_string(),
            seq.to_string(),
        ])
        .to_hex()
    }

    /// Run `f` against the shared connection on a blocking task, per the
    /// workspace's async-discipline rule. `f` gets `&mut Connection` so it
    /// can open a transaction.
    async fn with_conn<F, T>(&self, f: F) -> StoreResult<T>
    where
        F: FnOnce(&mut Connection) -> StoreResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut guard)
        })
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[async_trait]
impl Store for SqliteStore {
    async fn upsert_instance(
        &self,
        inst: &Instance,
        expected: Option<Revision>,
    ) -> StoreResult<Revision> {
        let inst = inst.clone();
        self.with_conn(move |conn| {
            let digest = inst.id.digest().to_hex();
            let tx = conn.transaction()?;
            let current: Option<i64> = tx
                .query_row(
                    "SELECT revision FROM instances WHERE instance_digest = ?1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?;

            let new_revision: i64 = match (expected, current) {
                (None, None) => 1,
                (Some(exp), Some(cur)) if exp.value() as i64 == cur => cur + 1,
                (expected, current) => {
                    return Err(StoreError::RevisionConflict {
                        expected,
                        actual: current.map(|c| Revision::from_raw(c as u64)),
                    });
                }
            };

            let mut stored = inst.clone();
            stored.spec_revision = Revision::from_raw(new_revision as u64);
            let data = serde_json::to_string(&stored)?;
            let canonical = inst.id.canonical();
            let org = inst.id.org().as_str();

            tx.execute(
                "INSERT INTO instances (instance_digest, canonical, org, data, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(instance_digest) DO UPDATE SET data = excluded.data, revision = excluded.revision",
                params![digest, canonical, org, data, new_revision],
            )?;
            tx.commit()?;
            Ok(Revision::from_raw(new_revision as u64))
        })
        .await
    }

    async fn get_instance(&self, id: &InstanceId) -> StoreResult<Option<Instance>> {
        let digest = id.digest().to_hex();
        self.with_conn(move |conn| {
            let data: Option<String> = conn
                .query_row(
                    "SELECT data FROM instances WHERE instance_digest = ?1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?;
            data.map(|d| serde_json::from_str(&d).map_err(StoreError::from))
                .transpose()
        })
        .await
    }

    async fn list_instances(&self, org: &Ident) -> StoreResult<Vec<Instance>> {
        let org = org.to_string();
        self.with_conn(move |conn| {
            let mut stmt =
                conn.prepare("SELECT data FROM instances WHERE org = ?1 ORDER BY canonical")?;
            let rows = stmt.query_map(params![org], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok(out)
        })
        .await
    }

    async fn put_plan(&self, plan: &Plan) -> StoreResult<()> {
        let plan = plan.clone();
        self.with_conn(move |conn| {
            let plan_id = plan.id.as_str().to_string();
            let instance_digest = plan.instance.digest().to_hex();
            let data = serde_json::to_string(&plan)?;
            conn.execute(
                "INSERT INTO plans (plan_id, instance_digest, data, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(plan_id) DO UPDATE SET data = excluded.data, expires_at = excluded.expires_at",
                params![plan_id, instance_digest, data, plan.created_at, plan.expires_at],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_plan(&self, id: &PlanId) -> StoreResult<Option<Plan>> {
        let plan_id = id.as_str().to_string();
        self.with_conn(move |conn| {
            let data: Option<String> = conn
                .query_row(
                    "SELECT data FROM plans WHERE plan_id = ?1",
                    params![plan_id],
                    |row| row.get(0),
                )
                .optional()?;
            data.map(|d| serde_json::from_str(&d).map_err(StoreError::from))
                .transpose()
        })
        .await
    }

    async fn append_run(&self, run: &Run) -> StoreResult<()> {
        let run = run.clone();
        self.with_conn(move |conn| {
            let run_id = run.id.as_str().to_string();
            let instance_digest = run.instance.digest().to_hex();
            let org = run.instance.org().as_str().to_string();
            let status = format!("{:?}", run.status);
            let data = serde_json::to_string(&run)?;
            conn.execute(
                "INSERT INTO runs (run_id, instance_digest, org, status, data, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![run_id, instance_digest, org, status, data, run.started_at],
            )?;
            Ok(())
        })
        .await
    }

    async fn update_run(&self, id: &RunId, patch: RunPatch) -> StoreResult<()> {
        let run_id = id.as_str().to_string();
        self.with_conn(move |conn| {
            let tx = conn.transaction()?;
            let data: Option<String> = tx
                .query_row(
                    "SELECT data FROM runs WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(data) = data else {
                return Err(StoreError::NotFound(format!("run {run_id}")));
            };
            let mut run: Run = serde_json::from_str(&data)?;
            patch.apply_to(&mut run);
            let status = format!("{:?}", run.status);
            let updated = serde_json::to_string(&run)?;
            tx.execute(
                "UPDATE runs SET data = ?1, status = ?2 WHERE run_id = ?3",
                params![updated, status, run_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn list_runs(&self, filter: RunFilter) -> StoreResult<Vec<Run>> {
        self.with_conn(move |conn| {
            let mut sql = String::from("SELECT data FROM runs WHERE 1 = 1");
            let mut bind: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(org) = &filter.org {
                sql.push_str(&format!(" AND org = ?{}", bind.len() + 1));
                bind.push(Box::new(org.to_string()));
            }
            if let Some(instance) = &filter.instance {
                sql.push_str(&format!(" AND instance_digest = ?{}", bind.len() + 1));
                bind.push(Box::new(instance.digest().to_hex()));
            }
            if let Some(status) = &filter.status {
                sql.push_str(&format!(" AND status = ?{}", bind.len() + 1));
                bind.push(Box::new(format!("{status:?}")));
            }
            sql.push_str(" ORDER BY started_at DESC");
            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }

            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> = bind.iter().map(|b| b.as_ref()).collect();
            let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str::<Run>(&row?)?);
            }
            Ok(out)
        })
        .await
    }

    async fn put_output_set(&self, key: &InstanceId, set: &OutputSet) -> StoreResult<Revision> {
        let key = key.clone();
        let set = set.clone();
        self.with_conn(move |conn| {
            let digest = key.digest().to_hex();
            let tx = conn.transaction()?;
            let current_max: Option<i64> = tx
                .query_row(
                    "SELECT MAX(revision) FROM output_sets WHERE instance_digest = ?1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            let new_revision = current_max.unwrap_or(0) + 1;

            let mut stored = set.clone();
            stored.revision = Revision::from_raw(new_revision as u64);
            let data = serde_json::to_string(&stored)?;

            tx.execute(
                "INSERT INTO output_sets (instance_digest, revision, data) VALUES (?1, ?2, ?3)",
                params![digest, new_revision, data],
            )?;
            tx.commit()?;
            Ok(Revision::from_raw(new_revision as u64))
        })
        .await
    }

    async fn get_output_set(&self, key: &InstanceId) -> StoreResult<Option<OutputSet>> {
        let digest = key.digest().to_hex();
        self.with_conn(move |conn| {
            let data: Option<String> = conn
                .query_row(
                    "SELECT data FROM output_sets WHERE instance_digest = ?1 ORDER BY revision DESC LIMIT 1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?;
            data.map(|d| serde_json::from_str(&d).map_err(StoreError::from))
                .transpose()
        })
        .await
    }

    async fn mark_consumed(
        &self,
        consumer: &InstanceId,
        producer: &InstanceId,
        revision: Revision,
    ) -> StoreResult<()> {
        let consumer = consumer.clone();
        let producer = producer.clone();
        self.with_conn(move |conn| {
            let consumer_digest = consumer.digest().to_hex();
            let producer_digest = producer.digest().to_hex();
            let consumer_json = serde_json::to_string(&consumer)?;
            let producer_json = serde_json::to_string(&producer)?;
            let org = consumer.org().as_str().to_string();
            conn.execute(
                "INSERT INTO consumed_state
                    (consumer_digest, producer_digest, consumer_json, producer_json, org, consumed_revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(consumer_digest, producer_digest)
                    DO UPDATE SET consumed_revision = excluded.consumed_revision",
                params![
                    consumer_digest,
                    producer_digest,
                    consumer_json,
                    producer_json,
                    org,
                    revision.value() as i64
                ],
            )?;
            Ok(())
        })
        .await
    }

    async fn list_stale_consumers(&self, org: &Ident) -> StoreResult<Vec<StaleConsumer>> {
        let org = org.to_string();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT consumer_json, producer_json, producer_digest, consumed_revision
                 FROM consumed_state WHERE org = ?1",
            )?;
            let rows = stmt.query_map(params![org], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;

            let mut out = Vec::new();
            for row in rows {
                let (consumer_json, producer_json, producer_digest, consumed_revision) = row?;
                let current_max: Option<i64> = conn
                    .query_row(
                        "SELECT MAX(revision) FROM output_sets WHERE instance_digest = ?1",
                        params![producer_digest],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();

                // No output set at all for this producer any more - there is
                // nothing to compare freshness against, so it's not reported
                // as "stale" (that would conflate "moved on" with "gone").
                let Some(current_max) = current_max else {
                    continue;
                };
                if current_max > consumed_revision {
                    out.push(StaleConsumer {
                        consumer: serde_json::from_str(&consumer_json)?,
                        producer: serde_json::from_str(&producer_json)?,
                        consumed_revision: Revision::from_raw(consumed_revision as u64),
                        current_revision: Revision::from_raw(current_max as u64),
                    });
                }
            }
            Ok(out)
        })
        .await
    }

    async fn acquire_lease(
        &self,
        key: &InstanceId,
        owner: &str,
        ttl: Duration,
    ) -> StoreResult<Lease> {
        let key = key.clone();
        let owner = owner.to_string();
        let ttl_ms = ttl.as_millis() as i64;
        let now = now_millis();
        let token = self.next_token(&key, &owner, now);

        self.with_conn(move |conn| {
            let digest = key.digest().to_hex();
            let tx = conn.transaction()?;
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT expires_at FROM leases WHERE instance_digest = ?1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(expires_at) = existing {
                if expires_at > now {
                    return Err(StoreError::LeaseHeld { expires_at });
                }
            }

            let expires_at = now + ttl_ms;
            tx.execute(
                "INSERT INTO leases (instance_digest, owner, token, acquired_at, ttl_ms, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(instance_digest) DO UPDATE SET
                    owner = excluded.owner, token = excluded.token,
                    acquired_at = excluded.acquired_at, ttl_ms = excluded.ttl_ms,
                    expires_at = excluded.expires_at",
                params![digest, owner, token, now, ttl_ms, expires_at],
            )?;
            tx.commit()?;

            Ok(Lease {
                instance: key,
                owner,
                token,
                acquired_at: now,
                ttl_ms,
                expires_at,
            })
        })
        .await
    }

    async fn renew_lease(&self, lease: &Lease) -> StoreResult<Lease> {
        let lease = lease.clone();
        let now = now_millis();
        self.with_conn(move |conn| {
            let digest = lease.instance.digest().to_hex();
            let tx = conn.transaction()?;
            let current_token: Option<String> = tx
                .query_row(
                    "SELECT token FROM leases WHERE instance_digest = ?1",
                    params![digest],
                    |row| row.get(0),
                )
                .optional()?;
            if current_token.as_deref() != Some(lease.token.as_str()) {
                return Err(StoreError::LeaseLost);
            }
            let expires_at = now + lease.ttl_ms;
            tx.execute(
                "UPDATE leases SET expires_at = ?1 WHERE instance_digest = ?2 AND token = ?3",
                params![expires_at, digest, lease.token],
            )?;
            tx.commit()?;
            Ok(Lease {
                expires_at,
                ..lease
            })
        })
        .await
    }

    async fn release_lease(&self, lease: Lease) -> StoreResult<()> {
        self.with_conn(move |conn| {
            let digest = lease.instance.digest().to_hex();
            let affected = conn.execute(
                "DELETE FROM leases WHERE instance_digest = ?1 AND token = ?2",
                params![digest, lease.token],
            )?;
            if affected == 0 {
                return Err(StoreError::LeaseLost);
            }
            Ok(())
        })
        .await
    }

    async fn put_artifact(&self, bytes: &[u8]) -> StoreResult<Digest> {
        let bytes = bytes.to_vec();
        self.with_conn(move |conn| {
            let digest = Digest::of(&bytes);
            let hex = digest.to_hex();
            conn.execute(
                "INSERT OR IGNORE INTO artifacts (digest, bytes) VALUES (?1, ?2)",
                params![hex, bytes],
            )?;
            Ok(digest)
        })
        .await
    }

    async fn get_artifact(&self, digest: &Digest) -> StoreResult<Option<Vec<u8>>> {
        let hex = digest.to_hex();
        self.with_conn(move |conn| {
            let bytes: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT bytes FROM artifacts WHERE digest = ?1",
                    params![hex],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(bytes)
        })
        .await
    }
}

/// Migration seam for v2's `DeploymentLogRepository`/`UnitStateRepository`
/// ports - see [`crate::legacy`]. Not part of the [`Store`] trait: these
/// shapes are v2-only and retired in P7, so they stay as plain inherent
/// methods rather than polluting the v3 port every future adapter has to
/// implement.
impl SqliteStore {
    /// Append one deployment-log entry. Append-only, like the v2 fs-jsonl
    /// adapter it replaces - there is no update/delete for log entries.
    pub async fn append_legacy_deployment_log(
        &self,
        row: LegacyDeploymentLogRow,
    ) -> StoreResult<()> {
        self.with_conn(move |conn| {
            let data = serde_json::to_string(&row)?;
            conn.execute(
                "INSERT INTO legacy_deployment_log (org, timestamp, data) VALUES (?1, ?2, ?3)",
                params![row.org, row.timestamp, data],
            )?;
            Ok(())
        })
        .await
    }

    /// Every entry ever saved for `org`, in no particular order - callers
    /// apply their own query-matching and newest-first/limit logic on top
    /// (see `cubtera-persistence`'s adapter), matching the v2 fs-jsonl
    /// adapter's own "read everything, then filter in process" shape so
    /// the two can't drift on what a query actually matches.
    pub async fn find_legacy_deployment_log(
        &self,
        org: &str,
    ) -> StoreResult<Vec<LegacyDeploymentLogRow>> {
        let org = org.to_string();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare("SELECT data FROM legacy_deployment_log WHERE org = ?1")?;
            let rows = stmt.query_map(params![org], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn get_legacy_unit_state(
        &self,
        state_key: &str,
    ) -> StoreResult<Option<LegacyUnitStateRow>> {
        let state_key = state_key.to_string();
        self.with_conn(move |conn| {
            let data: Option<String> = conn
                .query_row(
                    "SELECT data FROM legacy_unit_state WHERE state_key = ?1",
                    params![state_key],
                    |row| row.get(0),
                )
                .optional()?;
            data.map(|d| serde_json::from_str(&d).map_err(StoreError::from))
                .transpose()
        })
        .await
    }

    /// Publish (or overwrite) a unit-state row under `state_key` - matches
    /// v2's "put overwrites, never appends" semantics.
    pub async fn put_legacy_unit_state(
        &self,
        state_key: String,
        row: LegacyUnitStateRow,
    ) -> StoreResult<()> {
        self.with_conn(move |conn| {
            let data = serde_json::to_string(&row)?;
            conn.execute(
                "INSERT INTO legacy_unit_state (state_key, org, unit, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(state_key) DO UPDATE SET data = excluded.data",
                params![state_key, row.org, row.unit, data],
            )?;
            Ok(())
        })
        .await
    }

    /// Delete a unit-state row. A no-op (not an error) if it never existed
    /// - matches v2's `UnitStateRepository::delete` contract.
    pub async fn delete_legacy_unit_state(&self, state_key: &str) -> StoreResult<()> {
        let state_key = state_key.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM legacy_unit_state WHERE state_key = ?1",
                params![state_key],
            )?;
            Ok(())
        })
        .await
    }

    /// Every unit-state row published for `org`/`unit`, across every
    /// dims/ext combination it has ever published under.
    pub async fn list_legacy_unit_state(
        &self,
        org: &str,
        unit: &str,
    ) -> StoreResult<Vec<LegacyUnitStateRow>> {
        let org = org.to_string();
        let unit = unit.to_string();
        self.with_conn(move |conn| {
            let mut stmt =
                conn.prepare("SELECT data FROM legacy_unit_state WHERE org = ?1 AND unit = ?2")?;
            let rows = stmt.query_map(params![org, unit], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok(out)
        })
        .await
    }
}
