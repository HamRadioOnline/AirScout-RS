//! SQLite tile store with cached elevation lookup (port of the tile part of
//! `ElevationDatabase.cs`). One database file per elevation model.

use crate::cache::TileCache;
use crate::{ElevationError, ElevationModel, ElevationTile, TILE_MISSING};
use as_geo::latlon::GRect;
use as_geo::maidenhead;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Database status bits (`DATABASESTATUS` of the original SQLiteDatabase library).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DbStatus(pub u32);

impl DbStatus {
    pub const UNDEFINED: DbStatus = DbStatus(0);
    pub const EMPTY: DbStatus = DbStatus(1);
    pub const COMPLETE: DbStatus = DbStatus(2);
    pub const UPDATING: DbStatus = DbStatus(4);
    pub const UPTODATE: DbStatus = DbStatus(8);
    pub const ERROR: DbStatus = DbStatus(128);
}

/// C# tile sizes in bytes (`*_tile_size`), used to derive cache capacities.
fn tile_size_bytes(model: ElevationModel) -> usize {
    match model {
        ElevationModel::Globe => 138,
        ElevationModel::Srtm3 | ElevationModel::Aster3 => 10_038,
        ElevationModel::Srtm1 | ElevationModel::Aster1 => 90_038,
        ElevationModel::None => 1,
    }
}

/// Default memory budget for the tile cache per model.
pub const DEFAULT_CACHE_BUDGET_BYTES: usize = 100 * 1024 * 1024;

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS Elevation (
        TileIndex TEXT NOT NULL PRIMARY KEY,
        MinElv INTEGER, MinLat REAL, MinLon REAL,
        MaxElv INTEGER, MaxLat REAL, MaxLon REAL,
        Rows INTEGER NOT NULL, Columns INTEGER NOT NULL,
        Elv BLOB NOT NULL, LastUpdated INTEGER NOT NULL)",
    crate::path::PATH_SCHEMA,
    crate::obstruction::OBSTRUCTION_SCHEMA,
];

struct ModelDb {
    conn: Mutex<Connection>,
    cache: Mutex<TileCache>,
    status: Mutex<DbStatus>,
}

/// Elevation tile store for all models.
pub struct ElevationStore {
    dir: PathBuf,
    dbs: HashMap<ElevationModel, ModelDb>,
    bounds: Mutex<GRect>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock only means another thread panicked mid-operation; the data
    // (connections, cache) stays usable.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ElevationStore {
    /// Open (or create) the store in `dir` with the default cache budget.
    pub fn open(dir: &Path) -> Result<Self, ElevationError> {
        Self::open_inner(dir, None)
    }

    /// Open with an explicit per-model cache capacity (number of tiles).
    pub fn open_with_cache_capacity(dir: &Path, capacity: usize) -> Result<Self, ElevationError> {
        Self::open_inner(dir, Some(capacity))
    }

    fn open_inner(dir: &Path, capacity: Option<usize>) -> Result<Self, ElevationError> {
        let mut dbs = HashMap::new();
        for model in ElevationModel::ALL {
            let conn = as_db::open(&dir.join(model.db_file_name()))?;
            as_db::migrate(&conn, SCHEMA)?;
            let cap = capacity.unwrap_or(DEFAULT_CACHE_BUDGET_BYTES / tile_size_bytes(model));
            dbs.insert(
                model,
                ModelDb {
                    conn: Mutex::new(conn),
                    cache: Mutex::new(TileCache::new(cap)),
                    status: Mutex::new(DbStatus::UNDEFINED),
                },
            );
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            dbs,
            bounds: Mutex::new(GRect::new(0.0, 0.0, 0.0, 0.0)),
        })
    }

    /// Directory holding the database files.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn db(&self, model: ElevationModel) -> Result<&ModelDb, ElevationError> {
        self.dbs.get(&model).ok_or_else(|| {
            ElevationError::InvalidArgument(format!("no database for model {model:?}"))
        })
    }

    /// Run `f` with the model's connection (used by path/horizon caches in this crate and others).
    pub fn with_conn<R>(
        &self,
        model: ElevationModel,
        f: impl FnOnce(&Connection) -> Result<R, ElevationError>,
    ) -> Result<R, ElevationError> {
        let db = self.db(model)?;
        let conn = lock(&db.conn);
        f(&conn)
    }

    /// Insert the tile, or replace the stored one if this one is newer.
    pub fn insert_or_update_if_newer(
        &self,
        model: ElevationModel,
        t: &ElevationTile,
    ) -> Result<(), ElevationError> {
        self.with_conn(model, |conn| insert_or_update_tile(conn, t))
    }

    /// Insert many tiles in one transaction (all or nothing).
    pub fn insert_tiles_atomic(
        &self,
        model: ElevationModel,
        tiles: &[ElevationTile],
    ) -> Result<(), ElevationError> {
        self.with_conn(model, |conn| {
            let tx = conn.unchecked_transaction()?;
            for t in tiles {
                insert_or_update_tile(&tx, t)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Find a tile by its 6-character index.
    pub fn find(
        &self,
        model: ElevationModel,
        index: &str,
    ) -> Result<Option<ElevationTile>, ElevationError> {
        self.with_conn(model, |conn| {
            let row = conn
                .query_row(
                    "SELECT TileIndex, MinElv, MinLat, MinLon, MaxElv, MaxLat, MaxLon, Rows, Columns, Elv, LastUpdated
                     FROM Elevation WHERE TileIndex = ?1",
                    params![index],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i32>(1)?,
                            r.get::<_, f64>(2)?,
                            r.get::<_, f64>(3)?,
                            r.get::<_, i32>(4)?,
                            r.get::<_, f64>(5)?,
                            r.get::<_, f64>(6)?,
                            r.get::<_, i64>(7)?,
                            r.get::<_, i64>(8)?,
                            r.get::<_, Vec<u8>>(9)?,
                            r.get::<_, i64>(10)?,
                        ))
                    },
                )
                .optional()?;
            let Some((idx, min_elv, min_lat, min_lon, max_elv, max_lat, max_lon, rows, cols, blob, lu)) = row else {
                return Ok(None);
            };
            let (rows, cols) = (rows.max(0) as usize, cols.max(0) as usize);
            let raw = ElevationTile::from_blob(&idx, rows, cols, &blob)?;
            let t = ElevationTile::new(
                &idx,
                min_elv,
                min_lat,
                min_lon,
                max_elv,
                max_lat,
                max_lon,
                rows,
                cols,
                raw.elv,
                DateTime::<Utc>::from_timestamp_nanos(lu),
            )?;
            Ok(Some(t))
        })
    }

    /// `ElevationTileExists`: is the 6-character tile stored (`ElevationDatabase.cs:949-969`)?
    pub fn tile_exists(&self, model: ElevationModel, index6: &str) -> Result<bool, ElevationError> {
        self.with_conn(model, |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT LastUpdated FROM Elevation WHERE TileIndex = ?1)",
                params![index6],
                |r| r.get::<_, i64>(0),
            )? > 0)
        })
    }

    /// Number of tiles stored for the model.
    pub fn count(&self, model: ElevationModel) -> Result<u64, ElevationError> {
        self.with_conn(model, |conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM Elevation", [], |r| r.get::<_, i64>(0))?
                    as u64,
            )
        })
    }

    /// Database bounds (`MinLat..MaxLon` of the original), used to judge missing tiles.
    pub fn bounds(&self) -> GRect {
        *lock(&self.bounds)
    }

    /// Set the database bounds.
    pub fn set_bounds(&self, r: GRect) {
        *lock(&self.bounds) = r;
    }

    /// Current status of a model database.
    pub fn status(&self, model: ElevationModel) -> DbStatus {
        self.db(model).map(|d| *lock(&d.status)).unwrap_or_default()
    }

    /// Replace the status.
    pub fn set_status(&self, model: ElevationModel, s: DbStatus) {
        if let Ok(d) = self.db(model) {
            *lock(&d.status) = s;
        }
    }

    /// True if any bit of `bit` is set (`GetDBStatusBit`).
    pub fn status_bit(&self, model: ElevationModel, bit: DbStatus) -> bool {
        self.status(model).0 & bit.0 > 0
    }

    /// Set bit(s).
    pub fn set_status_bit(&self, model: ElevationModel, bit: DbStatus) {
        if let Ok(d) = self.db(model) {
            lock(&d.status).0 |= bit.0;
        }
    }

    /// Clear bit(s).
    pub fn reset_status_bit(&self, model: ElevationModel, bit: DbStatus) {
        if let Ok(d) = self.db(model) {
            lock(&d.status).0 &= !bit.0;
        }
    }

    /// `COMPLETE && !ERROR`, as checked before path calculations.
    pub fn is_complete(&self, model: ElevationModel) -> bool {
        self.status_bit(model, DbStatus::COMPLETE) && !self.status_bit(model, DbStatus::ERROR)
    }

    /// Number of tiles in the in-memory cache.
    pub fn cache_len(&self, model: ElevationModel) -> usize {
        self.db(model).map(|d| lock(&d.cache).len()).unwrap_or(0)
    }

    /// Clear all caches (`CacheClearAll`).
    pub fn cache_clear_all(&self) {
        for d in self.dbs.values() {
            lock(&d.cache).clear();
        }
    }

    /// `GetElevationTile(model, lat, lon)`.
    fn tile_for(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
    ) -> Result<Option<Arc<ElevationTile>>, ElevationError> {
        let db = self.db(model)?;
        {
            let cache = lock(&db.cache);
            if let Some(t) = &cache.last {
                if t.contains(lat, lon) {
                    return Ok(Some(t.clone()));
                }
            }
        }
        let loc = maidenhead::loc_from_lat_lon(lat, lon, false, 3, false)?
            .ok_or(ElevationError::InvalidCoordinate { lat, lon })?;
        let mut td = lock(&db.cache).get(&loc);
        if td.is_none() {
            if let Some(t) = self.find(model, &loc)? {
                let t = Arc::new(t);
                lock(&db.cache).offer(&loc, t.clone());
                td = Some(t);
            }
        }
        if let Some(t) = &td {
            lock(&db.cache).last = Some(t.clone());
        }
        Ok(td)
    }

    /// `GetElevation(model, lat, lon, setinvalidtozero)`.
    pub fn elevation(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
        set_invalid_to_zero: bool,
    ) -> Result<i16, ElevationError> {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(ElevationError::InvalidCoordinate { lat, lon });
        }
        let mut e = match self.tile_for(model, lat, lon)? {
            Some(t) => t.get_elevation(lat, lon),
            None => TILE_MISSING,
        };
        if set_invalid_to_zero && e <= TILE_MISSING {
            e = 0;
        }
        Ok(e)
    }
}

fn insert_or_update_tile(conn: &Connection, t: &ElevationTile) -> Result<(), ElevationError> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT LastUpdated FROM Elevation WHERE TileIndex = ?1",
            params![t.index],
            |r| r.get(0),
        )
        .optional()?;
    let lu = t.last_updated.timestamp_nanos_opt().unwrap_or(i64::MAX);
    if existing.is_some_and(|e| e >= lu) {
        return Ok(());
    }
    conn.execute(
        "INSERT OR REPLACE INTO Elevation
         (TileIndex, MinElv, MinLat, MinLon, MaxElv, MaxLat, MaxLon, Rows, Columns, Elv, LastUpdated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            t.index,
            t.min_elv,
            t.min_lat,
            t.min_lon,
            t.max_elv,
            t.max_lat,
            t.max_lon,
            t.rows as i64,
            t.columns as i64,
            t.to_blob(),
            lu
        ],
    )?;
    Ok(())
}
