//! Thin SQLite helpers shared by the AirScout-RS data crates.
#![forbid(unsafe_code)]

mod maintenance;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::fmt;
use std::path::Path;

pub use maintenance::{clear_table, row_count, select_non_blob, table_list};

pub use rusqlite;

/// Errors returned by versioned database open and migration helpers.
#[derive(Debug)]
pub enum DbError {
    /// The database's `PRAGMA user_version` is newer than this build supports.
    SchemaAhead { actual: i64, supported: i64 },
    /// Underlying SQLite error.
    Sqlite(rusqlite::Error),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaAhead { actual, supported } => write!(
                f,
                "database schema version {actual} is newer than supported version {supported}"
            ),
            Self::Sqlite(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SchemaAhead { .. } => None,
            Self::Sqlite(err) => Some(err),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

/// Initialization options for versioned databases.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InitOptions {
    /// Set `PRAGMA auto_vacuum = FULL` when initializing a brand-new database (`page_count == 0`).
    /// Existing databases keep their current `auto_vacuum` mode.
    pub auto_vacuum_full: bool,
}

impl InitOptions {
    /// Enable `PRAGMA auto_vacuum = FULL` when initializing a fresh database.
    pub const fn with_auto_vacuum_full(mut self) -> Self {
        self.auto_vacuum_full = true;
        self
    }
}

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

fn page_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA page_count", [], |r| r.get(0))
}

fn init_schema(conn: &Connection, steps: &[&str], options: InitOptions) -> Result<(), DbError> {
    let supported = i64::try_from(steps.len()).unwrap_or(i64::MAX);
    // Inspect user_version on the raw connection before any app-changing pragmas.
    let initial = user_version(conn)?;
    if initial > supported {
        return Err(DbError::SchemaAhead {
            actual: initial,
            supported,
        });
    }
    // On a fresh database (page_count == 0), PRAGMA auto_vacuum = FULL must run before
    // BEGIN IMMEDIATE because BEGIN IMMEDIATE establishes page 1 with auto_vacuum = 0.
    // If another opener already wrote page 1 before this pragma executes, SQLite leaves
    // the existing file untouched and the locked recheck below rejects schema-ahead.
    if options.auto_vacuum_full && page_count(conn)? == 0 {
        conn.pragma_update(None, "auto_vacuum", "FULL")?;
    }
    // Always recheck under an immediate transaction, including when initial == supported,
    // so a concurrent upgrade behind a migration lock is observed before startup completes.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let current = user_version(&tx)?;
    if current > supported {
        return Err(DbError::SchemaAhead {
            actual: current,
            supported,
        });
    }
    let current = current.max(0) as usize;
    if current < steps.len() {
        for step in &steps[current..] {
            tx.execute_batch(step)?;
        }
        tx.pragma_update(None, "user_version", supported)?;
        tx.commit()?;
    } else {
        drop(tx);
    }
    Ok(())
}

/// Open (or create) a versioned SQLite database file, creating parent directories,
/// rejecting schemas ahead of `steps.len()` before any mutating pragmas, running pending
/// migrations under an immediate transaction, and applying WAL/synchronous defaults afterwards.
pub fn open_versioned(path: &Path, steps: &[&str]) -> Result<Connection, DbError> {
    open_versioned_with(path, steps, InitOptions::default())
}

/// Like [`open_versioned`], with explicit [`InitOptions`].
pub fn open_versioned_with(
    path: &Path,
    steps: &[&str],
    options: InitOptions,
) -> Result<Connection, DbError> {
    if let Some(parent) = path.parent() {
        // Errors surface on open; ignore here to keep the error contract consistent.
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    init_schema(&conn, steps, options)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

/// Open and initialize a versioned in-memory SQLite database.
pub fn open_in_memory_versioned(steps: &[&str]) -> Result<Connection, DbError> {
    open_in_memory_versioned_with(steps, InitOptions::default())
}

/// Like [`open_in_memory_versioned`], with explicit [`InitOptions`].
pub fn open_in_memory_versioned_with(
    steps: &[&str],
    options: InitOptions,
) -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn, steps, options)?;
    Ok(conn)
}

/// Apply schema steps in order. Step `i` (0-based) runs only if `user_version <= i`;
/// afterwards `user_version` equals `steps.len()`. Rejects any schema newer than
/// `steps.len()` with [`DbError::SchemaAhead`] before mutating the database.
pub fn migrate(conn: &Connection, steps: &[&str]) -> Result<(), DbError> {
    init_schema(conn, steps, InitOptions::default())
}
