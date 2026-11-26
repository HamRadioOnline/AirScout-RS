//! Thin SQLite helpers shared by the AirScout-RS data crates.
#![forbid(unsafe_code)]

mod maintenance;

use rusqlite::Connection;
use std::path::Path;

pub use maintenance::{clear_table, row_count, select_non_blob, table_list};

pub use rusqlite;

/// Open (or create) a SQLite database file, creating parent directories.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        // Errors surface on open; ignore here to keep the rusqlite error type.
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

/// Open an in-memory database (tests, temporary caches).
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    Connection::open_in_memory()
}

/// Read `PRAGMA user_version`.
pub fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

/// Apply schema steps in order. Step `i` (0-based) runs only if `user_version <= i`;
/// afterwards `user_version` equals `steps.len()`. Runs in one transaction.
pub fn migrate(conn: &Connection, steps: &[&str]) -> rusqlite::Result<()> {
    let current = user_version(conn)?.max(0) as usize;
    if current >= steps.len() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for step in &steps[current..] {
        tx.execute_batch(step)?;
    }
    tx.pragma_update(None, "user_version", steps.len() as i64)?;
    tx.commit()
}
