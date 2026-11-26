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
    #[error("cannot parse Spectrum Lab data: {0}")]
    Parse(String),
}

const SCHEMA: &[&str] =
    &["CREATE TABLE IF NOT EXISTS SignalLevels (Level DOUBLE, LastUpdated INT32, PRIMARY KEY (LastUpdated))"];

/// The signal level history: one maximum level per second.
pub struct SignalDb {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SignalDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalDb").finish_non_exhaustive()
    }
}

fn from_secs(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap_or_default()
}

impl SignalDb {
    /// Open or create `<dir>/signals.db3`.
    pub fn open(dir: &Path) -> Result<Self, SignalError> {
        let conn = as_db::open(&dir.join("signals.db3"))?;
        // C# insists on auto_vacuum FULL; it only takes effect on a fresh database
        conn.pragma_update(None, "auto_vacuum", "FULL")?;
        Self::init(conn)
    }

    /// In-memory database (`Database_InMemory`, tests).
    pub fn open_in_memory() -> Result<Self, SignalError> {
        Self::init(as_db::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, SignalError> {
        as_db::migrate(&conn, SCHEMA)?;
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

    /// `SignalLevelInsertOrUpdateIfNewer`: insert, or update when the stored time (whole
    /// seconds) is older than `at`. As the row is found by that very second, only a
    /// sub-second `at` counts as newer. Returns the rows written.
    fn upsert_on(conn: &Connection, level: f64, at: DateTime<Utc>) -> rusqlite::Result<usize> {
        let stored: Option<i64> = conn
            .prepare_cached("SELECT LastUpdated FROM SignalLevels WHERE LastUpdated = ?1")?
            .query_row(params![at.timestamp()], |r| r.get(0))
            .optional()?;
        match stored {
            None => Self::insert_on(conn, level, at),
            Some(s) if from_secs(s) < at => conn
                .prepare_cached("UPDATE SignalLevels SET Level = ?1 WHERE LastUpdated = ?2")?
                .execute(params![level, s]),
            Some(_) => Ok(0),
        }
    }

    /// `SignalLevelInsertOrUpdateIfNewer`.
    pub fn insert_or_update_if_newer(
        &self,
        level: f64,
        at: DateTime<Utc>,
    ) -> Result<usize, SignalError> {
        Ok(Self::upsert_on(&self.conn(), level, at)?)
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
            if Self::insert_on(&tx, level, at).is_err() {
                errors += 1;
            }
        }
        tx.commit()?;
        Ok(-errors)
    }

    /// `SignalLevelFind`: the level stored for the second of `at`.
    pub fn find(&self, at: DateTime<Utc>) -> Result<Option<f64>, SignalError> {
        Ok(self
            .conn()
            .prepare_cached("SELECT Level FROM SignalLevels WHERE LastUpdated = ?1")?
            .query_row(params![at.timestamp()], |r| r.get::<_, Option<f64>>(0))
            .optional()?
            .map(|l| l.unwrap_or(f64::MIN)))
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

/// `Double.Parse` of one bin after the C# swaps `,`→`;` and `.`→`,`, in a culture with
/// `,` as decimal separator (the one the code was written for).
fn parse_bin(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || !s.chars().all(|c| "0123456789+-,eE".contains(c)) {
        return None;
    }
    s.replace(',', ".").parse().ok()
}

/// Parse a Spectrum Lab `_fft_expt.json` like the poller does: the time stamp (header item
/// 1, unix seconds, fraction truncated) and the maximum of the bins. The header is the text
/// between the first and second `[`, split at `:` and `,`; item 13 must be an integer (the
/// bin count). The bins are the text after the second `[` up to the character *before* the
/// first `]` (logic-findings: the last character of the last bin is dropped).
pub fn parse_speclab_fft(json: &str) -> Result<(DateTime<Utc>, f64), SignalError> {
    if !json.starts_with('[') {
        return Err(perr("Format Exception: Not a JSON file."));
    }
    let a: Vec<&str> = json.split('[').collect();
    let (head, data) = match (a.get(1), a.get(2)) {
        (Some(h), Some(d)) => (*h, *d),
        _ => return Err(perr("no header and data arrays")),
    };
    let head = head.replace(':', ",");
    let h: Vec<&str> = head.split(',').collect();
    let data = data.replace("\r\n", "");
    // `Remove(IndexOf("]") - 1)`
    let end = data
        .chars()
        .position(|c| c == ']')
        .and_then(|p| p.checked_sub(1))
        .ok_or_else(|| perr("bin array not terminated"))?;
    let bins: String = data.chars().take(end).collect();
    let bins = bins.replace(',', ";").replace('.', ",");
    let d = bins
        .split(';')
        .map(parse_bin)
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(|| perr("bad bin value"))?;
    let secs = h
        .get(1)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|x| x.is_finite())
        .ok_or_else(|| perr("bad time stamp"))?
        .trunc();
    let at = DateTime::from_timestamp(secs as i64, 0)
        .filter(|_| (-62_135_596_800.0..253_402_300_800.0).contains(&secs))
        .ok_or_else(|| perr("time stamp out of range"))?;
    h.get(13)
        .and_then(|s| s.trim().parse::<i32>().ok())
        .ok_or_else(|| perr("bad bin count"))?;
    // `Enumerable.Max`; the bins are finite
    let max = d.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok((at, max))
}
