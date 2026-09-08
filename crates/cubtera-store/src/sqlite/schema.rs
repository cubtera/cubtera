use rusqlite::Connection;

/// DDL for every table `SqliteStore` uses. Idempotent (`CREATE TABLE IF NOT
/// EXISTS`), so it's safe to call on every `SqliteStore::open`.
///
/// - `output_sets`'s `PRIMARY KEY (instance_digest, revision)` is the
///   uniqueness guarantee the spec calls out (ยง9): two publishes for the
///   same instance can never collide on the same revision, closing v2's
///   H4/H6/H10/H11 "no unique index" bug class.
/// - `leases`' `PRIMARY KEY (instance_digest)` means at most one lease per
///   instance can exist at a time, by construction.
/// - `WAL` mode is enabled for concurrent readers alongside the single
///   writer this adapter serializes through its connection mutex.
pub fn apply(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS instances (
            instance_digest TEXT PRIMARY KEY,
            canonical       TEXT NOT NULL,
            org             TEXT NOT NULL,
            data            TEXT NOT NULL,
            revision        INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_instances_org ON instances(org);

        CREATE TABLE IF NOT EXISTS plans (
            plan_id         TEXT PRIMARY KEY,
            instance_digest TEXT NOT NULL,
            data            TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            expires_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_plans_instance ON plans(instance_digest);

        CREATE TABLE IF NOT EXISTS runs (
            run_id          TEXT PRIMARY KEY,
            instance_digest TEXT NOT NULL,
            org             TEXT NOT NULL,
            status          TEXT NOT NULL,
            data            TEXT NOT NULL,
            started_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_runs_instance ON runs(instance_digest);
        CREATE INDEX IF NOT EXISTS idx_runs_org ON runs(org);

        CREATE TABLE IF NOT EXISTS output_sets (
            instance_digest TEXT NOT NULL,
            revision        INTEGER NOT NULL,
            data            TEXT NOT NULL,
            PRIMARY KEY (instance_digest, revision)
        );

        CREATE TABLE IF NOT EXISTS consumed_state (
            consumer_digest  TEXT NOT NULL,
            producer_digest  TEXT NOT NULL,
            consumer_json    TEXT NOT NULL,
            producer_json    TEXT NOT NULL,
            org              TEXT NOT NULL,
            consumed_revision INTEGER NOT NULL,
            PRIMARY KEY (consumer_digest, producer_digest)
        );
        CREATE INDEX IF NOT EXISTS idx_consumed_state_org ON consumed_state(org);

        CREATE TABLE IF NOT EXISTS leases (
            instance_digest TEXT PRIMARY KEY,
            owner           TEXT NOT NULL,
            token           TEXT NOT NULL,
            acquired_at     INTEGER NOT NULL,
            ttl_ms          INTEGER NOT NULL,
            expires_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            digest TEXT PRIMARY KEY,
            bytes  BLOB NOT NULL
        );
        "#,
    )
}
