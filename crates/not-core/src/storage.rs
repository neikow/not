//! SQLite layout: Loro doc blobs, key/value sync state, and the read model.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;

/// Bump when the read-model tables change; a mismatch triggers a full rebuild from the docs.
pub const READ_MODEL_VERSION: i64 = 10;

/// Local update rows per doc before they are folded into a snapshot on open.
const COMPACT_AFTER: i64 = 200;

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS doc_snapshot (
            doc  TEXT PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS doc_update (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            doc  TEXT NOT NULL,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS kv (
            key   TEXT PRIMARY KEY,
            value BLOB NOT NULL
        );
        -- Images in notes, by content id (see `images`); kept apart from the docs so they stay small.
        CREATE TABLE IF NOT EXISTS blob (
            id       TEXT PRIMARY KEY,
            data     BLOB NOT NULL,
            -- 0 until the server has it.
            uploaded INTEGER NOT NULL DEFAULT 0
        );
        ",
    )?;
    Ok(())
}

/// Drops and recreates read-model tables when their version changed. Returns true if it did.
pub fn ensure_read_model(db: &Connection) -> Result<bool> {
    let current = kv_get(db, "read_model_version")?.and_then(|v| v.try_into().ok().map(i64::from_le_bytes));
    if current == Some(READ_MODEL_VERSION) {
        return Ok(false);
    }
    db.execute_batch(
        "
        DROP TABLE IF EXISTS list;
        DROP TABLE IF EXISTS task;
        DROP TABLE IF EXISTS habit;
        DROP TABLE IF EXISTS habit_log;
        DROP TABLE IF EXISTS folder;
        DROP TABLE IF EXISTS note;
        DROP TABLE IF EXISTS workout;
        DROP TABLE IF EXISTS workout_exercise;

        CREATE TABLE list (
            id       TEXT PRIMARY KEY,
            title    TEXT NOT NULL,
            position TEXT NOT NULL
        );
        CREATE TABLE task (
            id           TEXT PRIMARY KEY,
            list_id      TEXT NOT NULL,
            parent_id    TEXT,
            title        TEXT NOT NULL,
            notes        TEXT NOT NULL,
            completed_at INTEGER,
            position     TEXT NOT NULL,
            due          TEXT,
            due_time     INTEGER,
            priority     INTEGER NOT NULL,
            repeat       TEXT
        );
        CREATE INDEX task_by_list ON task (list_id, parent_id, position);
        CREATE INDEX task_by_due ON task (due) WHERE due IS NOT NULL AND completed_at IS NULL;
        CREATE TABLE habit (
            id       TEXT PRIMARY KEY,
            title    TEXT NOT NULL,
            goal     INTEGER NOT NULL,
            days     INTEGER NOT NULL,
            period   TEXT NOT NULL,
            position TEXT NOT NULL
        );
        CREATE TABLE habit_log (
            habit_id TEXT NOT NULL,
            date     TEXT NOT NULL,
            count    INTEGER NOT NULL,
            PRIMARY KEY (habit_id, date)
        );
        CREATE TABLE folder (
            id        TEXT PRIMARY KEY,
            parent_id TEXT,
            title     TEXT NOT NULL,
            manual    INTEGER NOT NULL DEFAULT 0,
            sort      TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE note (
            id         TEXT PRIMARY KEY,
            folder_id  TEXT,
            title      TEXT NOT NULL,
            preview    TEXT NOT NULL,
            body       TEXT NOT NULL,
            pinned     INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0,
            position   TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX note_by_folder ON note (folder_id, pinned, updated_at);
        CREATE TABLE workout (
            id       TEXT PRIMARY KEY,
            date     TEXT NOT NULL,
            title    TEXT NOT NULL,
            position TEXT NOT NULL
        );
        CREATE TABLE workout_exercise (
            id         TEXT PRIMARY KEY,
            workout_id TEXT NOT NULL,
            name       TEXT NOT NULL,
            name_key   TEXT NOT NULL,
            sets       TEXT NOT NULL,
            position   TEXT NOT NULL,
            set_count  INTEGER NOT NULL,
            done       INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX workout_exercise_by_workout ON workout_exercise (workout_id, position);
        CREATE INDEX workout_exercise_by_name ON workout_exercise (name_key);
        ",
    )?;
    kv_set(db, "read_model_version", &READ_MODEL_VERSION.to_le_bytes())?;
    Ok(true)
}

pub fn load_doc(db: &Connection, doc: &str) -> Result<Vec<Vec<u8>>> {
    let mut blobs = Vec::new();
    let snapshot: Option<Vec<u8>> = db
        .query_row("SELECT data FROM doc_snapshot WHERE doc = ?1", [doc], |r| {
            r.get(0)
        })
        .optional()?;
    blobs.extend(snapshot);
    let mut stmt = db.prepare("SELECT data FROM doc_update WHERE doc = ?1 ORDER BY id")?;
    for row in stmt.query_map([doc], |r| r.get::<_, Vec<u8>>(0))? {
        blobs.push(row?);
    }
    Ok(blobs)
}

pub fn append_update(db: &Connection, doc: &str, data: &[u8]) -> Result<()> {
    db.execute(
        "INSERT INTO doc_update (doc, data) VALUES (?1, ?2)",
        params![doc, data],
    )?;
    Ok(())
}

pub fn needs_compaction(db: &Connection, doc: &str) -> Result<bool> {
    let n: i64 = db.query_row("SELECT count(*) FROM doc_update WHERE doc = ?1", [doc], |r| {
        r.get(0)
    })?;
    Ok(n > COMPACT_AFTER)
}

/// Replaces all stored blobs of `doc` with one snapshot. Caller wraps this in a transaction.
pub fn replace_with_snapshot(db: &Connection, doc: &str, snapshot: &[u8]) -> Result<()> {
    db.execute("DELETE FROM doc_update WHERE doc = ?1", [doc])?;
    db.execute(
        "INSERT INTO doc_snapshot (doc, data) VALUES (?1, ?2)
         ON CONFLICT (doc) DO UPDATE SET data = excluded.data",
        params![doc, snapshot],
    )?;
    Ok(())
}

pub fn kv_get(db: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(db
        .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn kv_delete(db: &Connection, key: &str) -> Result<()> {
    db.execute("DELETE FROM kv WHERE key = ?1", [key])?;
    Ok(())
}

pub fn kv_set(db: &Connection, key: &str, value: &[u8]) -> Result<()> {
    db.execute(
        "INSERT INTO kv (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
