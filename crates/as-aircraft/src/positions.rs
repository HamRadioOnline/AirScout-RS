//! Port of `AirScout.AircraftPositions/AircraftPositionDatabase.cs` and
//! `AircraftPositionDatabaseMaintainer.cs`: the position history in SQLite
//! (`aircraftpositions.db3`). Every query uses bound parameters.

use crate::check::{self, Lookup};
use crate::{AircraftError, PlaneInfo};
use as_geo::latlon;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

const MAINTAIN_DELETE_BATCH: usize = 1000;

const SCHEMA: &[&str] = &["CREATE TABLE IF NOT EXISTS AircraftPositions (
        Hex TEXT NOT NULL DEFAULT '', Call TEXT NOT NULL DEFAULT '', Lat DOUBLE, Lon DOUBLE,
        Alt DOUBLE, Track DOUBLE, Speed DOUBLE, LastUpdated INT32 NOT NULL DEFAULT 0,
        PRIMARY KEY (Hex, LastUpdated));
    CREATE INDEX IF NOT EXISTS idx_AircraftPositions_Hex ON AircraftPositions (Hex);
    CREATE INDEX IF NOT EXISTS idx_AircraftPositions_Call ON AircraftPositions (Call)"];

const COLS: &str = "Hex, Call, Lat, Lon, Alt, Track, Speed, LastUpdated";

/// One stored position report (`AircraftPositionDesignator`). Altitude in ft, speed in kts.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftPosition {
    pub hex: String,
    pub call: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub track: f64,
    pub speed_kts: f64,
    /// Stored as unix seconds; sub-second parts are dropped.
    pub last_updated: DateTime<Utc>,
}

impl AircraftPosition {
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            hex: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            call: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            lat: r.get::<_, Option<f64>>(2)?.unwrap_or_default(),
            lon: r.get::<_, Option<f64>>(3)?.unwrap_or_default(),
            alt_ft: r.get::<_, Option<f64>>(4)?.unwrap_or_default(),
            track: r.get::<_, Option<f64>>(5)?.unwrap_or_default(),
            speed_kts: r.get::<_, Option<f64>>(6)?.unwrap_or_default(),
            last_updated: from_secs(r.get(7)?),
        })
    }
}

fn from_secs(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap_or_default()
}

/// The aircraft position history.
pub struct PositionDb {
    conn: Mutex<Connection>,
}

impl PositionDb {
    /// Open or create `<dir>/aircraftpositions.db3`.
    pub fn open(dir: &Path) -> Result<Self, AircraftError> {
        // C# relies on auto_vacuum so that deletes shrink the file; it only takes effect on a
        // fresh database.
        let conn = as_db::open_versioned_with(
            &dir.join("aircraftpositions.db3"),
            SCHEMA,
            as_db::InitOptions::default().with_auto_vacuum_full(),
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory database (tests, temporary use).
    pub fn open_in_memory() -> Result<Self, AircraftError> {
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

    fn insert_on(conn: &Connection, p: &AircraftPosition) -> rusqlite::Result<usize> {
        // `AircraftPositionInsertOrUpdateIfNewer` looks up the row by its primary key
        // (Hex, LastUpdated), so the "newer" branch can never run: an existing row stays
        // (logic-findings #28).
        conn.prepare_cached(&format!(
            "INSERT OR IGNORE INTO AircraftPositions ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        ))?
        .execute(params![
            p.hex,
            p.call,
            p.lat,
            p.lon,
            p.alt_ft,
            p.track,
            p.speed_kts,
            p.last_updated.timestamp()
        ])
    }

    /// `AircraftPositionInsertOrUpdateIfNewer`. Returns the rows written (0 or 1).
    pub fn insert_or_update_if_newer(&self, p: &AircraftPosition) -> Result<usize, AircraftError> {
        Ok(Self::insert_on(&self.conn(), p)?)
    }

    /// `PlaneInfoBulkInsertOrUpdateIfNewer`: store every plane with a valid hex and call in
    /// one transaction. Returns the rows written; on error the transaction rolls back.
    /// (C# always returns 0, and returns -1 without rollback on error: logic-findings #29.)
    pub fn bulk_insert_plane_infos(
        &self,
        planes: &[PlaneInfo],
        lookup: &dyn Lookup,
    ) -> Result<usize, AircraftError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut n = 0;
        for p in planes {
            if !(check::hex(&p.hex) && check::call(&p.call, lookup)) {
                continue;
            }
            n += Self::insert_on(
                &tx,
                &AircraftPosition {
                    hex: p.hex.clone(),
                    call: p.call.clone(),
                    lat: p.lat,
                    lon: p.lon,
                    alt_ft: p.alt_ft,
                    track: p.track,
                    speed_kts: p.speed_kts,
                    last_updated: p.time,
                },
            )?;
        }
        tx.commit()?;
        Ok(n)
    }

    fn select(
        &self,
        filter: &str,
        args: impl rusqlite::Params,
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {COLS} FROM AircraftPositions {filter} ORDER BY LastUpdated ASC"
        ))?;
        let rows = st.query_map(args, AircraftPosition::from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `AircraftPositionGetAll(from, to)`: inclusive range, oldest first.
    pub fn get_all(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        self.select(
            "WHERE LastUpdated >= ?1 AND LastUpdated <= ?2",
            params![from.timestamp(), to.timestamp()],
        )
    }

    /// `AircraftPositionGetAllByHex(hex, from, to)`: inclusive range, oldest first.
    pub fn get_by_hex_in(
        &self,
        hex: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        self.select(
            "WHERE Hex = ?1 AND LastUpdated >= ?2 AND LastUpdated <= ?3",
            params![hex, from.timestamp(), to.timestamp()],
        )
    }

    /// `AircraftPositionGetAllByCall(call, from, to)`: inclusive range, oldest first.
    pub fn get_by_call_in(
        &self,
        call: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        self.select(
            "WHERE Call = ?1 AND LastUpdated >= ?2 AND LastUpdated <= ?3",
            params![call, from.timestamp(), to.timestamp()],
        )
    }

    fn distinct(
        &self,
        column: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<String>, AircraftError> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {column} FROM AircraftPositions
             WHERE LastUpdated >= ?1 AND LastUpdated <= ?2 GROUP BY {column}"
        ))?;
        let rows = st.query_map(params![from.timestamp(), to.timestamp()], |r| {
            Ok(r.get::<_, Option<String>>(0)?.unwrap_or_default())
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `AircraftPositionGetAllHex(from, to)`: every hex with a row in the inclusive range.
    pub fn get_all_hex(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<String>, AircraftError> {
        self.distinct("Hex", from, to)
    }

    /// `AircraftPositionGetAllCalls(from, to)`: every call with a row in the inclusive range.
    pub fn get_all_calls(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<String>, AircraftError> {
        self.distinct("Call", from, to)
    }

    /// `AircraftPositionGetAll(BackgroundWorker)`: the whole table, oldest first.
    ///
    /// `progress` gets the row count after every 1000th row. `cancel` is checked after every
    /// row; once set, the result is empty (as in C#). The connection lock is held for the whole
    /// scan (one connection serves all users), but a set `cancel` releases it within one row.
    pub fn get_all_with(
        &self,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize),
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {COLS} FROM AircraftPositions ORDER BY LastUpdated ASC"
        ))?;
        let mut rows = st.query([])?;
        let mut l = Vec::new();
        while let Some(r) = rows.next()? {
            l.push(AircraftPosition::from_row(r)?);
            if cancel.load(Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            if l.len() % 1000 == 0 {
                progress(l.len());
            }
        }
        Ok(l)
    }

    /// `AircraftPositionDeleteAll`; number of rows deleted.
    pub fn delete_all(&self) -> Result<usize, AircraftError> {
        Ok(self.conn().execute("DELETE FROM AircraftPositions", [])?)
    }

    /// `AircraftPositionBulkInsertOrUpdateIfNewer`: store every row in one transaction;
    /// returns the rows written (existing `(Hex, LastUpdated)` rows stay, logic-findings #28).
    /// On error the transaction rolls back and the error is returned (C# swallows it and
    /// commits what it has, as in logic-findings #29).
    pub fn bulk_insert(&self, aps: &[AircraftPosition]) -> Result<usize, AircraftError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut n = 0;
        for p in aps {
            n += Self::insert_on(&tx, p)?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// `AircraftPositionGetAllByHex(hex)`, oldest first.
    pub fn get_by_hex(&self, hex: &str) -> Result<Vec<AircraftPosition>, AircraftError> {
        self.select("WHERE Hex = ?1", params![hex])
    }

    /// `AircraftPositionGetAllByCall(call)`, oldest first.
    pub fn get_by_call(&self, call: &str) -> Result<Vec<AircraftPosition>, AircraftError> {
        self.select("WHERE Call = ?1", params![call])
    }

    /// `AircraftPositionGetAllLatest(at, ttl)`: the newest row per hex within
    /// `[at - ttl_min, at]`, dead-reckoned to `at` with speed and track. `last_updated` keeps
    /// the report time, as in C#.
    pub fn get_all_latest(
        &self,
        at: DateTime<Utc>,
        ttl_min: i64,
    ) -> Result<Vec<AircraftPosition>, AircraftError> {
        let to = at.timestamp();
        let from = to - ttl_min * 60;
        let conn = self.conn();
        // SQLite returns the bare columns of the row holding max(LastUpdated).
        let mut st = conn.prepare(
            "SELECT Hex, Call, Lat, Lon, Alt, Track, Speed, max(LastUpdated) AS LastUpdated
             FROM AircraftPositions WHERE LastUpdated >= ?1 AND LastUpdated <= ?2 GROUP BY Hex",
        )?;
        let rows = st.query_map(params![from, to], AircraftPosition::from_row)?;
        let mut l = Vec::new();
        for r in rows {
            let mut p = r?;
            let speed = p.speed_kts * 1.852;
            let hours = (at - p.last_updated).num_milliseconds() as f64 / 3_600_000.0;
            let np = latlon::destination_point(p.lat, p.lon, p.track, speed * hours);
            p.lat = np.lat;
            p.lon = np.lon;
            l.push(p);
        }
        Ok(l)
    }

    fn time_agg(&self, f: &str) -> Result<Option<DateTime<Utc>>, AircraftError> {
        let s: Option<i64> = self
            .conn()
            .query_row(
                &format!("SELECT {f}(LastUpdated) FROM AircraftPositions"),
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(s.map(from_secs))
    }

    /// `AircraftPositionOldestEntry`; `None` on an empty table.
    pub fn oldest(&self) -> Result<Option<DateTime<Utc>>, AircraftError> {
        self.time_agg("min")
    }

    /// `AircraftPositionYoungestEntry`; `None` on an empty table.
    pub fn youngest(&self) -> Result<Option<DateTime<Utc>>, AircraftError> {
        self.time_agg("max")
    }

    /// `AircraftPositionBulkDeleteOlderThan`: delete rows with `LastUpdated < t`.
    pub fn delete_older_than(&self, t: DateTime<Utc>) -> Result<usize, AircraftError> {
        Ok(self.conn().execute(
            "DELETE FROM AircraftPositions WHERE LastUpdated < ?1",
            params![t.timestamp()],
        )?)
    }

    /// `AircraftPositionBulkDeleteFirst`: delete the oldest `n` rows.
    /// Ties on `LastUpdated` are broken by rowid so repeated passes are stable.
    pub fn delete_oldest(&self, n: usize) -> Result<usize, AircraftError> {
        Ok(self.conn().execute(
            "DELETE FROM AircraftPositions WHERE rowid IN
             (SELECT rowid FROM AircraftPositions
              ORDER BY LastUpdated ASC, rowid ASC LIMIT ?1)",
            params![n as i64],
        )?)
    }

    /// `AircraftPositionCount`.
    pub fn count(&self) -> Result<u64, AircraftError> {
        let n: i64 = self
            .conn()
            .query_row("SELECT count(*) FROM AircraftPositions", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Database size in MB (`GetDBSize`), from page count and page size.
    pub fn file_size_mb(&self) -> Result<f64, AircraftError> {
        let conn = self.conn();
        let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((pages * size) as f64 / 1024.0 / 1024.0)
    }

    /// Occupied database size in MB (`page_count - freelist_count`).
    pub fn occupied_size_mb(&self) -> Result<f64, AircraftError> {
        let conn = self.conn();
        let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let free: i64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(((pages - free) * size) as f64 / 1024.0 / 1024.0)
    }
}

/// `AircraftPositionDatabaseMaintainerStartOptions`.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintainerConfig {
    pub max_count: u64,
    pub max_size_mb: f64,
    pub max_days: i64,
}

impl Default for MaintainerConfig {
    fn default() -> Self {
        Self {
            max_count: 10_000_000,
            max_size_mb: 1500.0,
            max_days: 7,
        }
    }
}

/// Rows deleted by one [`maintain`] run, per rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaintainStats {
    pub deleted_count: usize,
    pub deleted_size: usize,
    pub deleted_old: usize,
}

/// One pass of `AircraftPositionDatabaseMaintainer.OnDoWork` (the caller repeats it every
/// maintenance period). Retention runs in this order:
/// 1. delete rows older than `max_days` when it is positive and the cutoff is representable;
/// 2. while row count exceeds `max_count`, delete oldest rows in bounded batches;
/// 3. if occupied size (`page_count - freelist_count`) exceeds `max_size_mb`, delete one bounded
///    oldest batch.
pub fn maintain(
    db: &PositionDb,
    cfg: &MaintainerConfig,
    last_max_size: &mut f64,
) -> Result<MaintainStats, AircraftError> {
    let mut s = MaintainStats::default();
    if cfg.max_days > 0 {
        if let Some(cutoff) =
            Duration::try_days(cfg.max_days).and_then(|age| Utc::now().checked_sub_signed(age))
        {
            s.deleted_old = db.delete_older_than(cutoff)?;
        }
    }

    loop {
        let count = db.count()?;
        if count <= cfg.max_count {
            break;
        }
        let surplus = (count - cfg.max_count) as usize;
        let to_delete = surplus.min(MAINTAIN_DELETE_BATCH);
        let deleted = db.delete_oldest(to_delete)?;
        s.deleted_count += deleted;
        if deleted == 0 {
            break;
        }
    }

    let occupied_size = db.occupied_size_mb()?;
    *last_max_size = occupied_size;
    if occupied_size > cfg.max_size_mb {
        s.deleted_size = db.delete_oldest(MAINTAIN_DELETE_BATCH)?;
    }
    Ok(s)
}
