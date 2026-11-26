//! Signal levels: port of `AirScout.Signals/SignalDatabase.cs` (`signals.db3`) and the
//! Spectrum Lab URL and FFT parsing of `AirScout/MapDlg.cs:7620-7710`. The poller itself
//! belongs to the GUI. Every query uses bound parameters.
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Errors of the signal crate.
#[derive(thiserror::Error, Debug)]
pub enum SignalError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema version {actual} is newer than supported version {supported}")]
    SchemaAhead { actual: i64, supported: i64 },
    #[error("cannot parse Spectrum Lab data: {0}")]
    Parse(String),
    #[error("signal level must be finite")]
    InvalidLevel,
}

impl From<as_db::DbError> for SignalError {
    fn from(err: as_db::DbError) -> Self {
        match err {
            as_db::DbError::SchemaAhead { actual, supported } => {
                Self::SchemaAhead { actual, supported }
            }
            as_db::DbError::Sqlite(err) => Self::Db(err),
        }
    }
}

// Use the same validity predicate for migration, lookup and replacement of
// invalid values inserted by an older writer.
const VALID_LEVEL: &str =
    "typeof(Level) IN ('real', 'integer') AND Level BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308";

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS SignalLevels (Level DOUBLE, LastUpdated INT32, PRIMARY KEY (LastUpdated))",
    "DELETE FROM SignalLevels
     WHERE Level IS NULL OR typeof(Level) NOT IN ('real', 'integer')
        OR Level NOT BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308",
];

/// The signal level history: one maximum level per second.
pub struct SignalDb {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SignalDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalDb").finish_non_exhaustive()
    }
}

impl SignalDb {
    /// Open or create `<dir>/signals.db3`.
    pub fn open(dir: &Path) -> Result<Self, SignalError> {
        // C# insists on auto_vacuum FULL; it only takes effect on a fresh database.
        let conn = as_db::open_versioned_with(
            &dir.join("signals.db3"),
            SCHEMA,
            as_db::InitOptions::default().with_auto_vacuum_full(),
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory database (`Database_InMemory`, tests).
    pub fn open_in_memory() -> Result<Self, SignalError> {
        let conn = as_db::open_in_memory_versioned_with(
            SCHEMA,
            as_db::InitOptions::default().with_auto_vacuum_full(),
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` with the connection (database maintenance: table list, row counts, clear, view).
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.conn())
    }

    fn insert_on(conn: &Connection, level: f64, at: DateTime<Utc>) -> rusqlite::Result<usize> {
        conn.prepare_cached("INSERT INTO SignalLevels (Level, LastUpdated) VALUES (?1, ?2)")?
            .execute(params![level, at.timestamp()])
    }

    fn upsert_on(conn: &Connection, level: f64, at: DateTime<Utc>) -> Result<usize, SignalError> {
        if !level.is_finite() {
            return Err(SignalError::InvalidLevel);
        }
        let sql = format!(
            "INSERT INTO SignalLevels (Level, LastUpdated) VALUES (?1, ?2)
             ON CONFLICT (LastUpdated) DO UPDATE SET Level = excluded.Level
             WHERE Level IS NULL OR NOT ({VALID_LEVEL}) OR excluded.Level > Level"
        );
        Ok(conn
            .prepare_cached(&sql)?
            .execute(params![level, at.timestamp()])?)
    }

    /// Merge the strongest finite signal level for the UTC second containing `at`.
    pub fn insert_or_update_if_newer(
        &self,
        level: f64,
        at: DateTime<Utc>,
    ) -> Result<usize, SignalError> {
        Self::upsert_on(&self.conn(), level, at)
    }

    /// `SignalLevelBulkInsertOrUpdateIfNewer` (the poller's path): one transaction, returns
    /// the number of rows processed.
    pub fn bulk_insert_or_update_if_newer(
        &self,
        rows: &[(f64, DateTime<Utc>)],
    ) -> Result<usize, SignalError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for &(level, at) in rows {
            Self::upsert_on(&tx, level, at)?;
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// `SignalLevelBulkInsert`: plain inserts in one transaction; a failing row (an
    /// existing second) is skipped. Returns `-errors`, as C# does.
    pub fn bulk_insert(&self, rows: &[(f64, DateTime<Utc>)]) -> Result<i32, SignalError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut errors = 0i32;
        for &(level, at) in rows {
            if !level.is_finite() || Self::insert_on(&tx, level, at).is_err() {
                errors += 1;
            }
        }
        tx.commit()?;
        Ok(-errors)
    }

    /// `SignalLevelFind`: the level stored for the second of `at`.
    pub fn find(&self, at: DateTime<Utc>) -> Result<Option<f64>, SignalError> {
        let sql =
            format!("SELECT Level FROM SignalLevels WHERE LastUpdated = ?1 AND ({VALID_LEVEL})");
        Ok(self
            .conn()
            .prepare_cached(&sql)?
            .query_row(params![at.timestamp()], |r| r.get(0))
            .optional()?)
    }

    /// Delete every level before `t` (housekeeping; not in C#). Returns the rows deleted.
    pub fn delete_older_than(&self, t: DateTime<Utc>) -> Result<usize, SignalError> {
        Ok(self.conn().execute(
            "DELETE FROM SignalLevels WHERE LastUpdated < ?1",
            params![t.timestamp()],
        )?)
    }

    /// `SignalLevelCount`.
    pub fn count(&self) -> Result<u64, SignalError> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM SignalLevels", [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

// ------------------------------------------------------------------ Spectrum Lab

/// The Spectrum Lab request URL of the poller (`MapDlg.cs:7655-7672`), including the
/// clamping of the band limits. C# joins the second parameter with `?` instead of `&`
/// (a GUI-layer bug, logic-findings #78); the port joins with `&` so Spectrum Lab sees both
/// limits.
pub fn speclab_url(base: &str, file: &str, f1: i32, f2: i32) -> String {
    let f2 = f2.clamp(1, 3000);
    let mut f1 = f1.clamp(0, 3000);
    if f1 >= f2 {
        f1 = f2 - 1;
    }
    let mut url = base.to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    format!("{url}{file}?f1={f1}&f2={f2}")
}

fn perr(msg: &str) -> SignalError {
    SignalError::Parse(msg.to_string())
}

/// Parse an FFT export with a header object and an array of numeric bins.
pub fn parse_speclab_fft(json: &str) -> Result<(DateTime<Utc>, f64), SignalError> {
    // SQLite is already linked by this crate. Its bundled JSON parser handles
    // key order, escaping and numeric syntax without a second JSON dependency.
    let conn = as_db::open_in_memory()?;
    let (secs, count): (f64, i64) = conn
        .query_row(
            "SELECT json_extract(?1, '$[0].t_unix'), json_extract(?1, '$[0].nbins')
             WHERE CASE WHEN json_valid(?1) THEN
                 json_type(?1) = 'array' AND json_array_length(?1) = 2
                 AND json_type(?1, '$[0]') = 'object'
                 AND json_type(?1, '$[1]') = 'array'
                 AND json_type(?1, '$[0].t_unix') IN ('integer', 'real')
                 AND json_type(?1, '$[0].nbins') = 'integer'
             ELSE 0 END",
            [json],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| perr("bad FFT header"))?;
    let secs = secs.trunc();
    if !(-62_135_596_800.0..253_402_300_800.0).contains(&secs) {
        return Err(perr("time stamp out of range"));
    }
    let at =
        DateTime::from_timestamp(secs as i64, 0).ok_or_else(|| perr("time stamp out of range"))?;

    let (actual, valid, max): (i64, i64, Option<f64>) = conn
        .query_row(
            "SELECT count(*), count(finite), max(finite)
             FROM (
                 SELECT CASE WHEN type IN ('integer', 'real')
                                  AND value BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308
                             THEN value END AS finite
                 FROM json_each(?1, '$[1]')
             )",
            [json],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| perr("bad FFT bins"))?;
    if count <= 0 || count != actual {
        return Err(perr("bad bin count"));
    }
    let max = max
        .filter(|level| valid == actual && level.is_finite())
        .ok_or_else(|| perr("bad bin value"))?;
    Ok((at, max))
}
