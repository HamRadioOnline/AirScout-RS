//! The GMap `.gmdb` SQLite tile store (port of `ScoutBase.Maps/MapDatabase.cs`).
//!
//! Two tables as the C# creates them: `Tiles` (key and `CacheTime`) and `TilesData` (the image
//! blob, same `id`). Files from elsewhere are untrusted: foreign schemas, missing data rows, odd
//! `CacheTime` values and huge blobs give an error or `None`, never a panic or a large allocation.

use chrono::{DateTime, Datelike, NaiveDateTime, Timelike};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// Largest tile blob the store reads or writes.
pub const MAX_TILE_BYTES: usize = 4 * 1024 * 1024;

const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// `MapDatabase.cs:110` (`TilesCreateTable`), verbatim.
const TILES_DDL: &str = "CREATE TABLE Tiles (id INTEGER NOT NULL PRIMARY KEY, X INTEGER NOT NULL, Y INTEGER NOT NULL, Zoom INTEGER NOT NULL, Type UNSIGNED INTEGER  NOT NULL, CacheTime DATETIME)";
/// `MapDatabase.cs:124` (`TilesDataCreateTable`), verbatim.
const TILES_DATA_DDL: &str = "CREATE TABLE TilesData (id INTEGER NOT NULL PRIMARY KEY CONSTRAINT fk_Tiles_id REFERENCES Tiles(id) ON DELETE CASCADE, Tile BLOB NULL)";
const INDEX_DDL: &str = "CREATE INDEX IF NOT EXISTS IndexOfTiles ON Tiles (X, Y, Zoom, Type)";

/// Key of one tile: column, row, zoom and the provider's GMap `DbId` (the `Type` column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub x: u32,
    pub y: u32,
    pub zoom: u8,
    pub db_id: i32,
}

/// One `Tiles` row (`MapTileInfo` in C#).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileInfo {
    pub id: i64,
    pub key: TileKey,
    /// `CacheTime` as stored (local time for C#- and Rust-written rows); `None` when NULL, not
    /// a readable time, or outside the years 1601 to 9999.
    pub cache_time: Option<NaiveDateTime>,
}

#[derive(Debug, thiserror::Error)]
pub enum MapsError {
    #[error("{}: {source}", path.display())]
    Sql {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("{}: not a GMap tile database (needs the tables Tiles and TilesData)", .0.display())]
    NotAGmdb(PathBuf),
    /// A tile over [`MAX_TILE_BYTES`]; `id` and `key` are given where known.
    #[error("tile of {len} bytes is larger than {MAX_TILE_BYTES} bytes (id {id:?}, key {key:?})")]
    TooLarge {
        id: Option<i64>,
        key: Option<TileKey>,
        len: usize,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `true` for the PNG signature or the JPEG start-of-image marker.
pub fn is_png_or_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(b"\xff\xd8\xff")
}

/// What `sqlite_master` holds.
enum Schema {
    /// No table, index or view (a new or empty file).
    Empty,
    /// Both `Tiles` and `TilesData`.
    Gmdb,
    Foreign,
}

/// A GMap tile database. One connection, shared behind a mutex.
pub struct TileStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for TileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

fn sql_err(path: &Path) -> impl Fn(rusqlite::Error) -> MapsError + '_ {
    move |source| {
        if source.sqlite_error_code() == Some(ErrorCode::NotADatabase) {
            MapsError::NotAGmdb(path.to_path_buf())
        } else {
            MapsError::Sql {
                path: path.to_path_buf(),
                source,
            }
        }
    }
}

/// Reads `sqlite_master` only, so a foreign file is left as it is.
fn schema(conn: &Connection, path: &Path) -> Result<Schema, MapsError> {
    let err = sql_err(path);
    let mut st = conn
        .prepare("SELECT name FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'")
        .map_err(&err)?;
    let names: Vec<String> = st
        .query_map([], |r| r.get(0))
        .map_err(&err)?
        .collect::<Result<_, _>>()
        .map_err(&err)?;
    let has = |t: &str| names.iter().any(|n| n.eq_ignore_ascii_case(t));
    Ok(if names.is_empty() {
        Schema::Empty
    } else if has("Tiles") && has("TilesData") {
        Schema::Gmdb
    } else {
        Schema::Foreign
    })
}

fn format_cache_time(t: NaiveDateTime) -> String {
    // System.Data.SQLite's default text for a DateTime: seven fraction digits (100 ns ticks).
    // chrono has no `%.7f`, so the fraction is written by hand.
    let ticks = t.nanosecond().min(999_999_999) / 100;
    format!("{}.{ticks:07}", t.format("%Y-%m-%d %H:%M:%S"))
}

/// `CacheTime` as read; `None` when NULL, not a time, or outside the years 1601 to 9999 (no
/// file time: Windows `SystemTime` starts in 1601, and near chrono's limits time-zone arithmetic
/// panics), so no such time leaves the store.
fn parse_cache_time(v: ValueRef<'_>) -> Option<NaiveDateTime> {
    parse_cache_time_any(v).filter(|t| (1601..=9999).contains(&t.year()))
}

fn parse_cache_time_any(v: ValueRef<'_>) -> Option<NaiveDateTime> {
    match v {
        ValueRef::Text(t) => {
            let s = std::str::from_utf8(t).ok()?;
            ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
                .iter()
                .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
        }
        // SQLite's Julian day number, read to the millisecond.
        ValueRef::Integer(i) => julian_day(i as f64),
        ValueRef::Real(r) => julian_day(r),
        ValueRef::Null | ValueRef::Blob(_) => None,
    }
}

fn julian_day(jd: f64) -> Option<NaiveDateTime> {
    const UNIX_EPOCH_JD: f64 = 2_440_587.5;
    let ms = ((jd - UNIX_EPOCH_JD) * 86_400_000.0).round();
    // Out-of-range values saturate in the cast and are then refused by chrono.
    ms.is_finite()
        .then(|| DateTime::from_timestamp_millis(ms as i64))
        .flatten()
        .map(|d| d.naive_utc())
}

/// A blob column read under the size cap: the bytes, `TooLarge`, or `None` for NULL, a missing
/// row or a non-blob value.
fn capped_tile(
    id: Option<i64>,
    key: Option<TileKey>,
    kind: Option<String>,
    len: Option<i64>,
    blob: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, MapsError> {
    if blob.is_some() {
        return Ok(blob);
    }
    if kind.as_deref() == Some("blob") {
        let len = len
            .and_then(|l| usize::try_from(l).ok())
            .unwrap_or(usize::MAX);
        return Err(MapsError::TooLarge { id, key, len });
    }
    Ok(None)
}

/// The columns `capped_tile` takes; `typeof` and `octet_length` are checked before the blob is
/// read, so an oversized blob is never loaded.
fn capped_tile_columns() -> String {
    format!(
        "d.id, typeof(d.Tile), octet_length(d.Tile), \
         CASE WHEN typeof(d.Tile) = 'blob' AND octet_length(d.Tile) <= {MAX_TILE_BYTES} \
         THEN d.Tile END"
    )
}

type CappedRow = (Option<i64>, Option<String>, Option<i64>, Option<Vec<u8>>);

fn capped_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CappedRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn exists_in(conn: &Connection, k: TileKey) -> rusqlite::Result<bool> {
    // MapDatabase.cs:150 (TileExists)
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM Tiles WHERE X = ?1 AND Y = ?2 AND Zoom = ?3 AND Type = ?4)",
        params![k.x, k.y, k.zoom, k.db_id],
        |r| r.get(0),
    )
}

fn int<T: TryFrom<i64>>(v: ValueRef<'_>) -> Option<T> {
    match v {
        ValueRef::Integer(i) => T::try_from(i).ok(),
        _ => None,
    }
}

impl TileStore {
    /// Open `path` read-write. A new or empty file gets the C# tables and the index
    /// `IndexOfTiles`; a file with other tables but not both `Tiles` and `TilesData` is
    /// [`MapsError::NotAGmdb`] and stays unchanged. A file whose tables are created here gets the
    /// WAL journal; an existing GMap file keeps its journal mode. 2 s busy timeout, foreign keys on.
    pub fn open(path: &Path) -> Result<Self, MapsError> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let err = sql_err(path);
        let mut conn = Connection::open(path).map_err(&err)?;
        conn.busy_timeout(BUSY_TIMEOUT).map_err(&err)?;
        // The schema check comes before any pragma or DDL that writes to the file.
        match schema(&conn, path)? {
            Schema::Foreign => return Err(MapsError::NotAGmdb(path.to_path_buf())),
            // An existing file keeps its journal mode (GMap leaves WAL off,
            // `SQLitePureImageCache.cs:176-180`).
            Schema::Gmdb => {}
            Schema::Empty => {
                conn.pragma_update(None, "journal_mode", "WAL")
                    .map_err(&err)?;
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(&err)?;
                // Another process may have created the tables meanwhile.
                match schema(&tx, path)? {
                    Schema::Empty => tx
                        .execute_batch(&format!("{TILES_DDL}; {TILES_DATA_DDL}; {INDEX_DDL};"))
                        .map_err(&err)?,
                    Schema::Gmdb => {}
                    Schema::Foreign => return Err(MapsError::NotAGmdb(path.to_path_buf())),
                }
                tx.commit().map_err(&err)?;
            }
        }
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(&err)?;
        Ok(Self {
            path: path.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    /// Open an existing `.gmdb` read-only: never creates the file or a table, sets no write
    /// pragma. Missing tables are [`MapsError::NotAGmdb`].
    pub fn open_read_only(path: &Path) -> Result<Self, MapsError> {
        let err = sql_err(path);
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(&err)?;
        conn.busy_timeout(BUSY_TIMEOUT).map_err(&err)?;
        match schema(&conn, path)? {
            Schema::Gmdb => Ok(Self {
                path: path.to_path_buf(),
                conn: Mutex::new(conn),
            }),
            Schema::Empty | Schema::Foreign => Err(MapsError::NotAGmdb(path.to_path_buf())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-call; the connection is usable.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn err(&self) -> impl Fn(rusqlite::Error) -> MapsError + '_ {
        sql_err(&self.path)
    }

    /// `TileExists` (`MapDatabase.cs:146-161`).
    pub fn exists(&self, k: TileKey) -> Result<bool, MapsError> {
        exists_in(&self.lock(), k).map_err(self.err())
    }

    /// The image of the tile `k` (the lowest `id` if the key is stored twice). `None` when there
    /// is no such tile, no `TilesData` row, or a NULL or non-blob value; [`MapsError::TooLarge`]
    /// above [`MAX_TILE_BYTES`], checked before the blob is read.
    pub fn get(&self, k: TileKey) -> Result<Option<Vec<u8>>, MapsError> {
        let row = self
            .lock()
            .query_row(
                &format!(
                    "SELECT {} FROM Tiles t LEFT JOIN TilesData d ON d.id = t.id \
                     WHERE t.X = ?1 AND t.Y = ?2 AND t.Zoom = ?3 AND t.Type = ?4 \
                     ORDER BY t.id LIMIT 1",
                    capped_tile_columns()
                ),
                params![k.x, k.y, k.zoom, k.db_id],
                capped_row,
            )
            .optional()
            .map_err(self.err())?;
        match row {
            Some((id, kind, len, blob)) => capped_tile(id, Some(k), kind, len, blob),
            None => Ok(None),
        }
    }

    /// The image of the `TilesData` row `id`, with the rules of [`TileStore::get`].
    pub fn get_by_id(&self, id: i64) -> Result<Option<Vec<u8>>, MapsError> {
        let row = self
            .lock()
            .query_row(
                &format!(
                    "SELECT {} FROM TilesData d WHERE d.id = ?1",
                    capped_tile_columns()
                ),
                [id],
                capped_row,
            )
            .optional()
            .map_err(self.err())?;
        match row {
            Some((id, kind, len, blob)) => capped_tile(id, None, kind, len, blob),
            None => Ok(None),
        }
    }

    /// Store a tile (`TileInsert`, `MapDatabase.cs:222-253`) in one immediate transaction:
    /// `false` and nothing written if the key is already there, [`MapsError::TooLarge`] above
    /// [`MAX_TILE_BYTES`]. `cache_time` (local time) is written as System.Data.SQLite text, NULL
    /// when `None`. The `id` is SQLite's next rowid, `MAX(id)+1` as in C#.
    pub fn insert(
        &self,
        k: TileKey,
        cache_time: Option<NaiveDateTime>,
        data: &[u8],
    ) -> Result<bool, MapsError> {
        if data.len() > MAX_TILE_BYTES {
            return Err(MapsError::TooLarge {
                id: None,
                key: Some(k),
                len: data.len(),
            });
        }
        let err = self.err();
        let mut conn = self.lock();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(&err)?;
        if exists_in(&tx, k).map_err(&err)? {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO Tiles (X, Y, Zoom, Type, CacheTime) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![k.x, k.y, k.zoom, k.db_id, cache_time.map(format_cache_time)],
        )
        .map_err(&err)?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO TilesData (id, Tile) VALUES (?1, ?2)",
            params![id, data],
        )
        .map_err(&err)?;
        tx.commit().map_err(&err)?;
        Ok(true)
    }

    /// All `Tiles` rows ordered by `id` (`TilesGetAll`, `MapDatabase.cs:289-308`). Rows whose
    /// `X`, `Y`, `Zoom` or `Type` is no tile key (negative, too large, not an integer) are skipped.
    pub fn list(&self) -> Result<Vec<TileInfo>, MapsError> {
        let err = self.err();
        let conn = self.lock();
        let mut st = conn
            .prepare("SELECT id, X, Y, Zoom, Type, CacheTime FROM Tiles ORDER BY id")
            .map_err(&err)?;
        let mut rows = st.query([]).map_err(&err)?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().map_err(&err)? {
            let col = |i| r.get_ref(i).map_err(&err);
            let (Some(id), Some(x), Some(y), Some(zoom), Some(db_id)) = (
                int::<i64>(col(0)?),
                int(col(1)?),
                int(col(2)?),
                int(col(3)?),
                int(col(4)?),
            ) else {
                continue;
            };
            out.push(TileInfo {
                id,
                key: TileKey { x, y, zoom, db_id },
                cache_time: parse_cache_time(col(5)?),
            });
        }
        Ok(out)
    }

    /// Number of `Tiles` rows (`TilesCount`).
    pub fn count(&self) -> Result<u64, MapsError> {
        let n: i64 = self
            .lock()
            .query_row("SELECT COUNT(*) FROM Tiles", [], |r| r.get(0))
            .map_err(self.err())?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Distinct `Type` values (provider `DbId`s) with their row counts, ascending; values that
    /// are no `i32` are skipped.
    pub fn types(&self) -> Result<Vec<(i32, u64)>, MapsError> {
        let err = self.err();
        let conn = self.lock();
        let mut st = conn
            .prepare("SELECT Type, COUNT(*) FROM Tiles GROUP BY Type ORDER BY Type")
            .map_err(&err)?;
        let mut rows = st.query([]).map_err(&err)?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().map_err(&err)? {
            if let (Some(t), Some(n)) = (
                int::<i32>(r.get_ref(0).map_err(&err)?),
                int::<u64>(r.get_ref(1).map_err(&err)?),
            ) {
                out.push((t, n));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::limits::Limit;

    /// With `SQLITE_LIMIT_LENGTH` set to the cap, SQLite refuses (`SQLITE_TOOBIG`) to load any
    /// longer blob. `get` and `get_by_id` still answer `TooLarge`, so they decide on the size
    /// without loading the blob; code that read the blob first would get the SQL error instead.
    #[test]
    fn size_is_checked_before_the_blob_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.gmdb");
        let store = TileStore::open(&path).unwrap();
        let k = TileKey {
            x: 1,
            y: 2,
            zoom: 3,
            db_id: 4,
        };
        {
            let c = store.lock();
            c.execute(
                "INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (1, 1, 2, 3, 4)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO TilesData (id, Tile) VALUES (1, zeroblob(?1))",
                [MAX_TILE_BYTES as i64 + 1],
            )
            .unwrap();
            c.set_limit(Limit::SQLITE_LIMIT_LENGTH, MAX_TILE_BYTES as i32);
            // The control: loading the blob now fails.
            let read: rusqlite::Result<Vec<u8>> =
                c.query_row("SELECT Tile FROM TilesData WHERE id = 1", [], |r| r.get(0));
            assert_eq!(
                read.unwrap_err().sqlite_error_code(),
                Some(ErrorCode::TooBig)
            );
        }
        let too_large = |r: Result<Option<Vec<u8>>, MapsError>| match r {
            Err(MapsError::TooLarge { id, len, .. }) => (id, len),
            other => panic!("expected TooLarge, got {other:?}"),
        };
        assert_eq!(too_large(store.get(k)), (Some(1), MAX_TILE_BYTES + 1));
        assert_eq!(too_large(store.get_by_id(1)), (Some(1), MAX_TILE_BYTES + 1));
    }
}
