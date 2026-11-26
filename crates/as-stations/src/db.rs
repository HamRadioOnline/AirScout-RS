//! Port of `StationDatabase.cs`: locations and QRV info in SQLite.

use crate::location::normalise_call;
use crate::qrv::normalise_qrv_loc;
use crate::{
    default_antenna_gain, default_antenna_height, default_power, from_ns, to_ns, GeoSource,
    Location, Qrv, StationError,
};
use as_geo::band::Band;
use as_geo::latlon::GRect;
use as_geo::maidenhead::{gpoint_from_loc, PositionInRectangle};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// `Table_last_updated` key for the user's own QRV edits (Options, Stations tab). Kept apart from
/// the `QRV` key, which the stations updater uses as its last-import marker: a user edit must not
/// make the updater skip the server's `qrv.zip`.
pub const QRV_USER_EDIT_MARKER: &str = "QRV_UserEdit";

const SCHEMA: &[&str] = &["CREATE TABLE IF NOT EXISTS Location (
        Call TEXT NOT NULL DEFAULT '', Loc TEXT NOT NULL DEFAULT '', Lat REAL, Lon REAL,
        Source INTEGER, Hits INTEGER, LastUpdated INTEGER NOT NULL, PRIMARY KEY (Call, Loc));
    CREATE TABLE IF NOT EXISTS QRV (
        Call TEXT NOT NULL DEFAULT '', Loc TEXT NOT NULL DEFAULT '', Band INTEGER NOT NULL DEFAULT 0,
        AntennaHeight REAL, AntennaGain REAL, Power REAL, LastUpdated INTEGER NOT NULL,
        PRIMARY KEY (Call, Loc, Band));
    CREATE TABLE IF NOT EXISTS Table_last_updated (
        TableName TEXT PRIMARY KEY NOT NULL, LastUpdated INTEGER NOT NULL DEFAULT 0)"];

const LOCATION_COLS: &str = "Call, Loc, Lat, Lon, Source, Hits, LastUpdated";
const QRV_COLS: &str = "Call, Loc, Band, AntennaHeight, AntennaGain, Power, LastUpdated";

const LOCATION_UPSERT: &str =
    "INSERT INTO Location (Call, Loc, Lat, Lon, Source, Hits, LastUpdated)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
    ON CONFLICT (Call, Loc) DO UPDATE SET Lat = excluded.Lat, Lon = excluded.Lon,
      Source = excluded.Source, Hits = excluded.Hits, LastUpdated = excluded.LastUpdated
    WHERE excluded.LastUpdated > Location.LastUpdated";

const QRV_UPSERT: &str =
    "INSERT INTO QRV (Call, Loc, Band, AntennaHeight, AntennaGain, Power, LastUpdated)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
    ON CONFLICT (Call, Loc, Band) DO UPDATE SET AntennaHeight = excluded.AntennaHeight,
      AntennaGain = excluded.AntennaGain, Power = excluded.Power, LastUpdated = excluded.LastUpdated
    WHERE excluded.LastUpdated > QRV.LastUpdated";

fn location_from_row(r: &Row) -> rusqlite::Result<Location> {
    Ok(Location {
        call: r.get(0)?,
        loc: r.get(1)?,
        lat: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
        lon: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
        source: GeoSource::from_i32(r.get::<_, Option<i32>>(4)?.unwrap_or(0)),
        hits: r.get::<_, Option<i32>>(5)?.unwrap_or(0),
        last_updated: from_ns(r.get(6)?),
    })
}

fn qrv_from_row(r: &Row) -> rusqlite::Result<Qrv> {
    let band: i32 = r.get(2)?;
    Ok(Qrv {
        call: r.get(0)?,
        loc: r.get(1)?,
        band: Band::from_i32(band).unwrap_or(Band::None),
        antenna_height: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
        antenna_gain: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
        power: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
        last_updated: from_ns(r.get(6)?),
    })
}

/// The station database (one SQLite file).
pub struct StationDb {
    conn: Mutex<Connection>,
}

impl StationDb {
    /// Open (and migrate) the database file.
    pub fn open(path: &Path) -> Result<Self, StationError> {
        Self::init(as_db::open(path)?)
    }

    /// In-memory database (tests, temporary use).
    pub fn open_in_memory() -> Result<Self, StationError> {
        Self::init(as_db::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StationError> {
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

    /// `LocationCount`.
    pub fn location_count(&self) -> Result<u64, StationError> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM Location", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

    /// `LocationInsertOrUpdateIfNewer`. Returns `true` if a row was written.
    pub fn location_insert_or_update_if_newer(&self, l: &Location) -> Result<bool, StationError> {
        Ok(upsert_location(&self.conn(), l)? > 0)
    }

    /// `LocationBulkInsertOrUpdateIfNewer` in one transaction. Returns the number of rows written.
    pub fn location_bulk_insert_or_update_if_newer(
        &self,
        ls: &[Location],
    ) -> Result<usize, StationError> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        let mut n = 0;
        for l in ls {
            n += upsert_location(&tx, l)?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// `LocationFind(call, loc)`; with `loc == None` `LocationFind(call)`, which returns the most
    /// recently updated entry of the call. Empty call or locator gives `None` like C#.
    pub fn location_find(
        &self,
        call: &str,
        loc: Option<&str>,
    ) -> Result<Option<Location>, StationError> {
        let call = normalise_call(call);
        if call.is_empty() {
            return Ok(None);
        }
        let conn = self.conn();
        let r = match loc {
            Some("") => None,
            Some(loc) => conn
                .query_row(
                    &format!("SELECT {LOCATION_COLS} FROM Location WHERE Call = ?1 AND Loc = ?2"),
                    params![call, loc],
                    location_from_row,
                )
                .optional()?,
            None => conn
                .query_row(
                    &format!("SELECT {LOCATION_COLS} FROM Location WHERE Call = ?1 ORDER BY LastUpdated DESC LIMIT 1"),
                    params![call],
                    location_from_row,
                )
                .optional()?,
        };
        Ok(r)
    }

    /// `LocationFindOrCreate` (StationDatabase.cs:428-439): the stored location of `call` and
    /// `loc`, else a new one at the centre of the locator square (`FromLoc`), which is stored.
    /// `None` for an empty call or locator.
    pub fn location_find_or_create(
        &self,
        call: &str,
        loc: &str,
    ) -> Result<Option<Location>, StationError> {
        if call.is_empty() || loc.is_empty() {
            return Ok(None);
        }
        if let Some(l) = self.location_find(call, Some(loc))? {
            return Ok(Some(l));
        }
        let centre = gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle)
            .map_err(|_| StationError::InvalidLocator(loc.to_string()))?;
        let l = Location::new(
            call,
            loc,
            centre.lat,
            centre.lon,
            GeoSource::FromLoc,
            0,
            Utc::now(),
        );
        self.location_insert_or_update_if_newer(&l)?;
        Ok(Some(l))
    }

    /// `LocationFindAll(call)`: all locations of a call, most recent first.
    pub fn locations_by_call(&self, call: &str) -> Result<Vec<Location>, StationError> {
        let call = normalise_call(call);
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {LOCATION_COLS} FROM Location WHERE Call = ?1 ORDER BY LastUpdated DESC"
        ))?;
        let v = st
            .query_map(params![call], location_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// `LocationGetAll()` (`rect == None`) or `LocationGetAll(minlat, minlon, maxlat, maxlon)`:
    /// locations ordered by call; the rect bounds are inclusive.
    pub fn location_get_all(&self, rect: Option<GRect>) -> Result<Vec<Location>, StationError> {
        let conn = self.conn();
        let v = match rect {
            None => conn
                .prepare(&format!(
                    "SELECT {LOCATION_COLS} FROM Location ORDER BY Call ASC"
                ))?
                .query_map([], location_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            Some(r) => conn
                .prepare(&format!(
                    "SELECT {LOCATION_COLS} FROM Location WHERE Lat >= ?1 AND Lat <= ?2 \
                     AND Lon >= ?3 AND Lon <= ?4 ORDER BY Call ASC"
                ))?
                .query_map(
                    params![r.min_lat, r.max_lat, r.min_lon, r.max_lon],
                    location_from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(v)
    }

    /// `LocationGetAll(call)` with a LIKE pattern (`StationDatabase.cs:645-655`), ordered by call.
    /// The pattern is a bound parameter (the C# concatenates it into the SQL).
    pub fn locations_like(&self, pattern: &str) -> Result<Vec<Location>, StationError> {
        let conn = self.conn();
        let v = conn
            .prepare(&format!(
                "SELECT {LOCATION_COLS} FROM Location WHERE Call LIKE ?1 ORDER BY Call ASC"
            ))?
            .query_map(params![pattern], location_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// `LocationDelete(call, loc)`; an empty `loc` deletes all locations of the call, like C#.
    /// Returns `true` if a row was removed.
    pub fn location_delete(&self, call: &str, loc: &str) -> Result<bool, StationError> {
        let call = normalise_call(call);
        let conn = self.conn();
        let n = if loc.is_empty() {
            conn.execute("DELETE FROM Location WHERE Call = ?1", params![call])?
        } else {
            conn.execute(
                "DELETE FROM Location WHERE Call = ?1 AND Loc = ?2",
                params![call, loc],
            )?
        };
        Ok(n > 0)
    }

    /// `QRVDelete(call, loc, band)`; `Band::All` deletes every band of the station.
    /// Returns `true` if a row was removed.
    pub fn qrv_delete(&self, call: &str, loc: &str, band: Band) -> Result<bool, StationError> {
        let loc = normalise_qrv_loc(loc)?;
        let call = normalise_call(call);
        let conn = self.conn();
        let n = if band == Band::All {
            conn.execute(
                "DELETE FROM QRV WHERE Call = ?1 AND Loc = ?2",
                params![call, loc],
            )?
        } else {
            conn.execute(
                "DELETE FROM QRV WHERE Call = ?1 AND Loc = ?2 AND Band = ?3",
                params![call, loc, band as i32],
            )?
        };
        Ok(n > 0)
    }

    /// `QRVCount`.
    pub fn qrv_count(&self) -> Result<u64, StationError> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM QRV", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// `QRVInsertOrUpdateIfNewer`. Returns `true` if a row was written.
    pub fn qrv_insert_or_update_if_newer(&self, q: &Qrv) -> Result<bool, StationError> {
        Ok(upsert_qrv(&self.conn(), q)? > 0)
    }

    /// `QRVBulkInsertOrUpdateIfNewer` in one transaction. Returns the number of rows written.
    pub fn qrv_bulk_insert_or_update_if_newer(&self, qs: &[Qrv]) -> Result<usize, StationError> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        let mut n = 0;
        for q in qs {
            n += upsert_qrv(&tx, q)?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// `QRVFind(call, loc, band)`. The locator is normalised like `QRVDesignator` (error if
    /// shorter than 6 characters).
    pub fn qrv_find(&self, call: &str, loc: &str, band: Band) -> Result<Option<Qrv>, StationError> {
        let loc = normalise_qrv_loc(loc)?;
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {QRV_COLS} FROM QRV WHERE Call = ?1 AND Loc = ?2 AND Band = ?3"),
                params![normalise_call(call), loc, band as i32],
                qrv_from_row,
            )
            .optional()?)
    }

    /// `QRVFindOrCreateDefault`: the stored entry or one with the per-band defaults (not stored).
    pub fn qrv_find_or_create_default(
        &self,
        call: &str,
        loc: &str,
        band: Band,
    ) -> Result<Qrv, StationError> {
        if let Some(q) = self.qrv_find(call, loc, band)? {
            return Ok(q);
        }
        Qrv::new(
            call,
            loc,
            band,
            default_antenna_height(band),
            default_antenna_gain(band),
            default_power(band),
            Utc::now(),
        )
    }

    /// `QRVFind(call, loc)`: all bands of a station.
    pub fn qrv_get_all(&self, call: &str, loc: &str) -> Result<Vec<Qrv>, StationError> {
        let loc = normalise_qrv_loc(loc)?;
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {QRV_COLS} FROM QRV WHERE Call = ?1 AND Loc = ?2"
        ))?;
        let v = st
            .query_map(params![normalise_call(call), loc], qrv_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// `GetTableLastUpdatedTimeStamp`; `None` if never set.
    pub fn table_timestamp(&self, table: &str) -> Result<Option<DateTime<Utc>>, StationError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT LastUpdated FROM Table_last_updated WHERE TableName = ?1",
                params![table],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(from_ns))
    }

    /// `SetTableLastUpdatedTimeStamp`.
    pub fn set_table_timestamp(&self, table: &str, t: DateTime<Utc>) -> Result<(), StationError> {
        self.conn().execute(
            "INSERT INTO Table_last_updated (TableName, LastUpdated) VALUES (?1, ?2)
             ON CONFLICT (TableName) DO UPDATE SET LastUpdated = excluded.LastUpdated",
            params![table, to_ns(t)],
        )?;
        Ok(())
    }
}

fn upsert_location(conn: &Connection, l: &Location) -> Result<usize, StationError> {
    Ok(conn.prepare_cached(LOCATION_UPSERT)?.execute(params![
        l.call,
        l.loc,
        l.lat,
        l.lon,
        l.source as i32,
        l.hits,
        to_ns(l.last_updated)
    ])?)
}

fn upsert_qrv(conn: &Connection, q: &Qrv) -> Result<usize, StationError> {
    Ok(conn.prepare_cached(QRV_UPSERT)?.execute(params![
        q.call,
        q.loc,
        q.band as i32,
        q.antenna_height,
        q.antenna_gain,
        q.power,
        to_ns(q.last_updated)
    ])?)
}
