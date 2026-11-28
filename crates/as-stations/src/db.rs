//! Port of `StationDatabase.cs`: locations and QRV info in SQLite.

use crate::location::normalise_call;
use crate::qrv::{normalise_qrv_loc, qrv_key_from_position, qrv_lookup_key};
use crate::updater::{ArchiveId, ArchiveKind};
use crate::{
    default_antenna_gain, default_antenna_height, default_power, from_ns, to_ns, GeoSource,
    Location, Qrv, StationError,
};
use as_geo::band::Band;
use as_geo::latlon::{GPoint, GRect};
use as_geo::maidenhead::{check_lat_lon, gpoint_from_loc, PositionInRectangle};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

/// `Table_last_updated` key for the user's own QRV edits (Options, Stations tab). Kept apart from
/// the `QRV` key, which the stations updater uses as its last-import marker: a user edit must not
/// make the updater skip the server's `qrv.zip`.
pub const QRV_USER_EDIT_MARKER: &str = "QRV_UserEdit";

/// The station-import bookkeeping tables the port creates for itself (#385). They are internal to
/// the import: the maintenance Tables dialog hides them and [`StationDb::clear_table`] refuses to
/// clear them, so the import state cannot be wiped by mistake.
pub const INTERNAL_TABLES: [&str; 3] = [
    "StationImportState",
    "StationArchiveValidators",
    "StationImport",
];

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS Location (
        Call TEXT NOT NULL DEFAULT '', Loc TEXT NOT NULL DEFAULT '', Lat REAL, Lon REAL,
        Source INTEGER, Hits INTEGER, LastUpdated INTEGER NOT NULL, PRIMARY KEY (Call, Loc));
    CREATE TABLE IF NOT EXISTS QRV (
        Call TEXT NOT NULL DEFAULT '', Loc TEXT NOT NULL DEFAULT '', Band INTEGER NOT NULL DEFAULT 0,
        AntennaHeight REAL, AntennaGain REAL, Power REAL, LastUpdated INTEGER NOT NULL,
        PRIMARY KEY (Call, Loc, Band));
    CREATE TABLE IF NOT EXISTS Table_last_updated (
        TableName TEXT PRIMARY KEY NOT NULL, LastUpdated INTEGER NOT NULL DEFAULT 0)",
    "ALTER TABLE Location ADD COLUMN Usable INTEGER NOT NULL DEFAULT 1;
     UPDATE Location SET Usable = 0 WHERE
         typeof(Lat) NOT IN ('integer','real') OR typeof(Lon) NOT IN ('integer','real')
         OR Lat NOT BETWEEN -90 AND 90 OR Lon NOT BETWEEN -180 AND 180;
     CREATE TABLE StationImportState (
         Id INTEGER NOT NULL PRIMARY KEY CHECK (Id = 1),
         LocationGeneration INTEGER NOT NULL,
         CacheSalt BLOB NOT NULL);
     INSERT INTO StationImportState VALUES (1, 0, randomblob(32));
     CREATE TABLE StationArchiveValidators (
         Source TEXT NOT NULL, Fingerprint TEXT NOT NULL, Etag BLOB, LastModified BLOB,
         PRIMARY KEY (Source, Fingerprint));
     CREATE TABLE StationImport (
         Kind TEXT NOT NULL, Source TEXT NOT NULL, Fingerprint TEXT NOT NULL,
         Scope TEXT NOT NULL, LocationGeneration INTEGER NOT NULL,
         PRIMARY KEY (Kind, Source, Fingerprint, Scope, LocationGeneration));
     CREATE TRIGGER StationLocationInsert AFTER INSERT ON Location
         BEGIN UPDATE StationImportState SET LocationGeneration = LocationGeneration + 1; END;
     CREATE TRIGGER StationLocationUpdate AFTER UPDATE ON Location
         BEGIN UPDATE StationImportState SET LocationGeneration = LocationGeneration + 1; END;
     CREATE TRIGGER StationLocationDelete AFTER DELETE ON Location
         BEGIN
           UPDATE StationImportState SET LocationGeneration = LocationGeneration + 1;
           DELETE FROM StationImport WHERE Kind = 'Location';
         END;
     CREATE TRIGGER StationQrvDelete AFTER DELETE ON QRV
         BEGIN DELETE FROM StationImport WHERE Kind = 'QRV'; END;
     CREATE TRIGGER StationLocationValidateInsert BEFORE INSERT ON Location
         WHEN typeof(NEW.Lat) NOT IN ('integer','real') OR typeof(NEW.Lon) NOT IN ('integer','real')
              OR NEW.Lat NOT BETWEEN -90 AND 90 OR NEW.Lon NOT BETWEEN -180 AND 180
         BEGIN SELECT RAISE(ABORT, 'invalid station coordinates'); END;
     CREATE TRIGGER StationLocationValidateUpdate BEFORE UPDATE OF Lat, Lon ON Location
         WHEN typeof(NEW.Lat) NOT IN ('integer','real') OR typeof(NEW.Lon) NOT IN ('integer','real')
              OR NEW.Lat NOT BETWEEN -90 AND 90 OR NEW.Lon NOT BETWEEN -180 AND 180
         BEGIN SELECT RAISE(ABORT, 'invalid station coordinates'); END;",
];

const LOCATION_COLS: &str = "Call, Loc, Lat, Lon, Source, Hits, LastUpdated";
const QRV_COLS: &str = "Call, Loc, Band, AntennaHeight, AntennaGain, Power, LastUpdated";

/// Raw `StationArchiveValidators` row: optional Etag and Last-Modified bytes.
type StoredValidators = (Option<Vec<u8>>, Option<Vec<u8>>);

const LOCATION_UPSERT: &str =
    "INSERT INTO Location (Call, Loc, Lat, Lon, Source, Hits, LastUpdated)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
    ON CONFLICT (Call, Loc) DO UPDATE SET Lat = excluded.Lat, Lon = excluded.Lon,
      Source = excluded.Source, Hits = excluded.Hits, LastUpdated = excluded.LastUpdated,
      Usable = 1
    WHERE Location.Usable = 0 OR excluded.LastUpdated > Location.LastUpdated";

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
        lat: r.get(2)?,
        lon: r.get(3)?,
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

/// Rows changed by an Options station transaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StationEditCounts {
    pub locations_upserted: usize,
    pub qrv_upserted: usize,
    pub qrv_deleted: usize,
}

/// The station database (one SQLite file).
pub struct StationDb {
    conn: Mutex<Connection>,
}

impl StationDb {
    /// Open (and migrate) the database file.
    pub fn open(path: &Path) -> Result<Self, StationError> {
        let conn = as_db::open_versioned(path, SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory database (tests, temporary use).
    pub fn open_in_memory() -> Result<Self, StationError> {
        let conn = as_db::open_in_memory_versioned(SCHEMA)?;
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
            .query_row("SELECT COUNT(*) FROM Location WHERE Usable = 1", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
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
                    &format!("SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 AND Call = ?1 AND Loc = ?2"),
                    params![call, loc],
                    location_from_row,
                )
                .optional()?,
            None => conn
                .query_row(
                    &format!("SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 AND Call = ?1 ORDER BY LastUpdated DESC LIMIT 1"),
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
        let conn = self.conn();
        // A locator fallback should leave a flagged key available for a precise repair.
        let flagged: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM Location WHERE Call=?1 AND Loc=?2 AND Usable=0)",
            params![l.call, l.loc],
            |row| row.get(0),
        )?;
        if !flagged {
            upsert_location(&conn, &l)?;
        }
        Ok(Some(l))
    }

    /// `LocationFind` with an in-memory locator-centre fallback that is never stored (read-only
    /// remote lookups, #265): the stored location of `call`/`loc`, else a new one at the centre of
    /// the locator square (`FromLoc`). `None` for an empty call or locator.
    pub fn location_find_or_locator(
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
        Ok(Some(Location::new(
            call,
            loc,
            centre.lat,
            centre.lon,
            GeoSource::FromLoc,
            0,
            Utc::now(),
        )))
    }

    /// `LocationFindAll(call)`: all locations of a call, most recent first.
    pub fn locations_by_call(&self, call: &str) -> Result<Vec<Location>, StationError> {
        let call = normalise_call(call);
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 AND Call = ?1 ORDER BY LastUpdated DESC"
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
                    "SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 ORDER BY Call ASC"
                ))?
                .query_map([], location_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            Some(r) => conn
                .prepare(&format!(
                    "SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 AND Lat >= ?1 AND Lat <= ?2 \
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
                "SELECT {LOCATION_COLS} FROM Location WHERE Usable = 1 AND Call LIKE ?1 ORDER BY Call ASC"
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

    /// Save locations and QRV edits atomically. A QRV deletion only applies to the exact
    /// version the dialog loaded. The calculator marker commits with the edited rows.
    pub fn apply_station_edits(
        &self,
        locations: &[Location],
        upserts: &[Qrv],
        deletes: &[(&str, &str, Band, DateTime<Utc>)],
    ) -> Result<StationEditCounts, StationError> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        let mut counts = StationEditCounts::default();
        for location in locations {
            counts.locations_upserted += upsert_location(&tx, location)?;
        }
        for qrv in upserts {
            counts.qrv_upserted += upsert_qrv(&tx, qrv)?;
        }
        for &(call, loc, band, updated_at) in deletes {
            counts.qrv_deleted += tx.prepare_cached(
                "DELETE FROM QRV WHERE Call = ?1 AND Loc = ?2 AND Band = ?3 AND LastUpdated = ?4",
            )?.execute(params![normalise_call(call), normalise_qrv_loc(loc)?, band as i32, to_ns(updated_at)])?;
        }
        if counts.qrv_upserted > 0 || counts.qrv_deleted > 0 {
            mark_timestamp(&tx, QRV_USER_EDIT_MARKER, Utc::now())?;
        }
        tx.commit()?;
        Ok(counts)
    }

    /// `QRVFind(call, loc, band)`. The locator is resolved to its six-character key; a valid
    /// coarse locator (2 or 4 characters) can never match a stored key and gives `None` (no
    /// error), while malformed text is an error.
    pub fn qrv_find(&self, call: &str, loc: &str, band: Band) -> Result<Option<Qrv>, StationError> {
        let Some(loc) = qrv_lookup_key(loc)? else {
            return Ok(None);
        };
        self.qrv_find_by_key(call, &loc, band)
    }

    /// Exact `QRVFind` on a resolved six-character key.
    fn qrv_find_by_key(
        &self,
        call: &str,
        loc: &str,
        band: Band,
    ) -> Result<Option<Qrv>, StationError> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {QRV_COLS} FROM QRV WHERE Call = ?1 AND Loc = ?2 AND Band = ?3"),
                params![normalise_call(call), loc, band as i32],
                qrv_from_row,
            )
            .optional()?)
    }

    /// `QRVFindOrCreateDefault`: the stored entry, or one with the per-band defaults (not stored).
    /// For a coarse locator, a trusted position inside its cell derives the six-character key for
    /// an exact lookup; without one the band defaults are returned and the coarse locator is kept
    /// as given (no six-character key is fabricated).
    pub fn qrv_find_or_create_default(
        &self,
        call: &str,
        loc: &str,
        band: Band,
        trusted: Option<GPoint>,
    ) -> Result<Qrv, StationError> {
        let key = match qrv_lookup_key(loc)? {
            Some(key) => Some(key),
            None => trusted.and_then(|p| qrv_key_from_position(loc, p)),
        };
        if let Some(key) = &key {
            if let Some(q) = self.qrv_find_by_key(call, key, band)? {
                return Ok(q);
            }
        }
        let loc = key.unwrap_or_else(|| loc.trim().to_ascii_uppercase());
        Ok(Qrv {
            call: normalise_call(call),
            loc,
            band,
            antenna_height: default_antenna_height(band),
            antenna_gain: default_antenna_gain(band),
            power: default_power(band),
            last_updated: Utc::now(),
        })
    }

    /// `QRVFind(call, loc)`: all bands of a station. A valid coarse locator gives no rows.
    pub fn qrv_get_all(&self, call: &str, loc: &str) -> Result<Vec<Qrv>, StationError> {
        let Some(loc) = qrv_lookup_key(loc)? else {
            return Ok(Vec::new());
        };
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {QRV_COLS} FROM QRV WHERE Call = ?1 AND Loc = ?2"
        ))?;
        let v = st
            .query_map(params![normalise_call(call), loc], qrv_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub(crate) fn archive_salt(&self) -> Result<Vec<u8>, StationError> {
        Ok(self.conn().query_row(
            "SELECT CacheSalt FROM StationImportState WHERE Id = 1",
            [],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn archive_validators(
        &self,
        id: &ArchiveId,
    ) -> Result<Option<as_net::Validators>, StationError> {
        let row: Option<StoredValidators> = self.conn().query_row(
            "SELECT Etag, LastModified FROM StationArchiveValidators WHERE Source=?1 AND Fingerprint=?2",
            params![id.source, id.fingerprint],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).optional()?;
        let Some((etag, modified)) = row else {
            return Ok(None);
        };
        let etag = etag.and_then(|value| String::from_utf8(value).ok());
        let last_modified = modified.and_then(|value| String::from_utf8(value).ok());
        Ok(
            (etag.is_some() || last_modified.is_some()).then_some(as_net::Validators {
                etag,
                last_modified,
            }),
        )
    }

    pub(crate) fn save_archive_validators(
        &self,
        id: &ArchiveId,
        validators: &as_net::Validators,
    ) -> Result<(), StationError> {
        self.conn().execute(
            "INSERT INTO StationArchiveValidators (Source, Fingerprint, Etag, LastModified)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (Source, Fingerprint) DO UPDATE SET
                Etag=excluded.Etag, LastModified=excluded.LastModified",
            params![
                id.source,
                id.fingerprint,
                validators.etag.as_deref().map(str::as_bytes),
                validators.last_modified.as_deref().map(str::as_bytes)
            ],
        )?;
        Ok(())
    }

    pub(crate) fn import_archive(
        &self,
        kind: ArchiveKind,
        id: &ArchiveId,
        area: Option<GRect>,
        cancel: Option<&AtomicBool>,
        load: impl FnOnce() -> Result<String, StationError>,
    ) -> Result<usize, StationError> {
        let scope = import_scope(area);
        let conn = self.conn();
        let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
        let generation = match (kind, area) {
            (ArchiveKind::Qrv, Some(_)) => tx.query_row(
                "SELECT LocationGeneration FROM StationImportState WHERE Id=1",
                [],
                |row| row.get(0),
            )?,
            _ => 0_i64,
        };
        let keys = params![kind.table(), id.source, id.fingerprint, scope, generation];
        let known: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM StationImport WHERE Kind=?1 AND Source=?2 AND Fingerprint=?3 AND Scope=?4 AND LocationGeneration=?5)",
            keys,
            |row| row.get(0),
        )?;
        check_cancel(cancel)?;
        if known {
            return Ok(0);
        }
        let json = load()?;
        let mut count = 0;
        match kind {
            ArchiveKind::Location => {
                let locations = crate::locations_from_json(&json).map_err(archive_data_error)?;
                for location in locations {
                    check_cancel(cancel)?;
                    if area
                        .as_ref()
                        .is_none_or(|area| in_scope(area, location.lat, location.lon))
                    {
                        count += upsert_location(&tx, &location)?;
                    }
                }
            }
            ArchiveKind::Qrv => {
                let qrvs = crate::qrvs_from_json(&json).map_err(archive_data_error)?;
                let mut location = tx.prepare_cached(
                    "SELECT EXISTS(SELECT 1 FROM Location WHERE Usable=1 AND Call=?1 AND Loc=?2
                         AND Lat BETWEEN ?3 AND ?4 AND Lon BETWEEN ?5 AND ?6)",
                )?;
                for qrv in qrvs {
                    check_cancel(cancel)?;
                    let in_area = if let Some(area) = area {
                        location.query_row(
                            params![
                                qrv.call,
                                qrv.loc,
                                area.min_lat,
                                area.max_lat,
                                area.min_lon,
                                area.max_lon
                            ],
                            |row| row.get(0),
                        )?
                    } else {
                        true
                    };
                    if in_area {
                        count += upsert_qrv(&tx, &qrv)?;
                    }
                }
            }
        }
        check_cancel(cancel)?;
        tx.execute(
            "INSERT INTO StationImport (Kind, Source, Fingerprint, Scope, LocationGeneration) VALUES (?1, ?2, ?3, ?4, ?5)",
            keys,
        )?;
        mark_timestamp(&tx, kind.table(), Utc::now())?;
        tx.commit()?;
        Ok(count)
    }

    /// Clear a station table and invalidate its completed imports in the same transaction.
    /// The internal import tables ([`INTERNAL_TABLES`]) are refused (#385).
    pub fn clear_table(&self, table: &str) -> Result<usize, StationError> {
        if INTERNAL_TABLES.contains(&table) {
            return Err(StationError::InternalTable(table.to_string()));
        }
        let conn = self.conn();
        if !matches!(table, "Location" | "QRV") {
            return Ok(as_db::clear_table(&conn, table)?);
        }
        let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
        let count = tx.execute(&format!("DELETE FROM {table}"), [])?;
        if table == "Location" {
            // A clear of an already-empty table still invalidates its import markers.
            tx.execute(
                "UPDATE StationImportState SET LocationGeneration=LocationGeneration+1",
                [],
            )?;
        }
        tx.execute("DELETE FROM StationImport WHERE Kind=?1", [table])?;
        tx.commit()?;
        Ok(count)
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
        mark_timestamp(&self.conn(), table, t)
    }
}

fn mark_timestamp(conn: &Connection, table: &str, time: DateTime<Utc>) -> Result<(), StationError> {
    conn.execute(
        "INSERT INTO Table_last_updated (TableName, LastUpdated) VALUES (?1, ?2)
         ON CONFLICT (TableName) DO UPDATE SET LastUpdated = excluded.LastUpdated",
        params![table, to_ns(time)],
    )?;
    Ok(())
}

pub(crate) fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), StationError> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(StationError::Cancelled);
    }
    Ok(())
}

fn archive_data_error(error: serde_json::Error) -> StationError {
    StationError::ArchiveData {
        line: error.line(),
        column: error.column(),
    }
}

fn in_scope(area: &GRect, lat: f64, lon: f64) -> bool {
    (area.min_lat..=area.max_lat).contains(&lat) && (area.min_lon..=area.max_lon).contains(&lon)
}

fn import_scope(area: Option<GRect>) -> String {
    let Some(area) = area else {
        return "all".into();
    };
    let min_lat = area.min_lat.max(-90.0);
    let max_lat = area.max_lat.min(90.0);
    let min_lon = area.min_lon.max(-180.0);
    let max_lon = area.max_lon.min(180.0);
    let normalized = |value: f64| if value == 0.0 { 0 } else { value.to_bits() };
    if [area.min_lat, area.max_lat, area.min_lon, area.max_lon]
        .iter()
        .any(|value| value.is_nan())
        || min_lat > max_lat
        || min_lon > max_lon
    {
        return "empty".into();
    }
    format!(
        "{:016x}:{:016x}:{:016x}:{:016x}",
        normalized(min_lat),
        normalized(max_lat),
        normalized(min_lon),
        normalized(max_lon)
    )
}

fn upsert_location(conn: &Connection, l: &Location) -> Result<usize, StationError> {
    if !check_lat_lon(l.lat, l.lon) {
        return Err(StationError::InvalidCoordinates);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_edits_keep_a_qrv_row_updated_after_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stations.db");
        let db = StationDb::open(&path).unwrap();
        let concurrent = StationDb::open(&path).unwrap();
        let loaded_at: DateTime<Utc> = "2026-11-27T12:34:56Z".parse().unwrap();
        let band = Band::B144M;
        let loaded = Qrv::new("TE1ST", "JO50AA", band, 10.0, 12.0, 50.0, loaded_at).unwrap();
        db.qrv_insert_or_update_if_newer(&loaded).unwrap();
        let newer = Qrv {
            power: 100.0,
            last_updated: loaded_at + chrono::TimeDelta::seconds(1),
            ..loaded
        };
        concurrent.qrv_insert_or_update_if_newer(&newer).unwrap();
        let counts = db
            .apply_station_edits(&[], &[], &[(" te1st ", " jo50aa ", band, loaded_at)])
            .unwrap();
        assert_eq!(counts.qrv_deleted, 0);
        assert_eq!(db.qrv_find("TE1ST", "JO50AA", band).unwrap(), Some(newer));
        assert_eq!(db.table_timestamp(QRV_USER_EDIT_MARKER).unwrap(), None);
    }

    #[test]
    fn station_edits_roll_back_location_and_qrv_when_a_delete_fails() {
        let db = StationDb::open_in_memory().unwrap();
        let now = Utc::now();
        let location = Location::new("TE1ST", "JO50AA", 50.01, 10.03, GeoSource::FromUser, 0, now);
        let band = Band::B144M;
        let qrv = Qrv::new("TE1ST", "JO50AA", band, 10.0, 12.0, 50.0, now).unwrap();
        assert!(db
            .apply_station_edits(&[location], &[qrv], &[("TE1ST", "JO50", band, now)])
            .is_err());
        assert_eq!(db.location_find("TE1ST", Some("JO50AA")).unwrap(), None);
        assert_eq!(db.qrv_count().unwrap(), 0);
        assert_eq!(db.table_timestamp(QRV_USER_EDIT_MARKER).unwrap(), None);
    }

    #[test]
    fn finding_383_legacy_invalid_keys_are_unusable_until_repaired_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stations.db3");
        let raw = rusqlite::Connection::open(&path).unwrap();
        as_db::migrate(&raw, &SCHEMA[..1]).unwrap();
        raw.execute_batch(
            "INSERT INTO Location VALUES
                ('TE1ST','JO50AA',NULL,10,2,0,2000000000),
                ('TE2ST','JJ00AA',0,0,2,0,2000000000);
             INSERT INTO QRV VALUES ('TE1ST','JO50AA',144,22,11,10,2000000000);",
        )
        .unwrap();
        drop(raw);
        let db = StationDb::open(&path).unwrap();
        assert!(db.location_find("TE1ST", Some("JO50AA")).unwrap().is_none());
        let zero = db.location_find("TE2ST", Some("JJ00AA")).unwrap().unwrap();
        assert_eq!((zero.lat, zero.lon), (0.0, 0.0));
        assert_eq!(db.location_get_all(None).unwrap().len(), 1);
        assert_eq!(db.qrv_count().unwrap(), 1);
        let repair = Location::new(
            "TE1ST",
            "JO50AA",
            50.01,
            10.01,
            GeoSource::FromUser,
            0,
            from_ns(1_000_000_000),
        );
        assert!(db.location_insert_or_update_if_newer(&repair).unwrap());
        assert!(!db.location_insert_or_update_if_newer(&repair).unwrap());
        drop(db);
        let db = StationDb::open(&path).unwrap();
        assert_eq!(
            db.location_find("TE1ST", Some("JO50AA")).unwrap(),
            Some(repair)
        );
    }
}
