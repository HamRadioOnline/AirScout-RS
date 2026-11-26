//! Port of `AirScout.Aircrafts/AircraftDatabase.cs`: aircraft, types, airlines, airports and
//! registration prefixes in SQLite (`aircrafts.db3`). Unlike the original, every query uses bound
//! parameters.

use crate::check::{find_reg_prefix, Lookup};
use crate::{AircraftError, PlaneCategory, PlaneInfo};
use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

const SCHEMA: &[&str] = &["CREATE TABLE IF NOT EXISTS Aircrafts (
        Hex TEXT NOT NULL DEFAULT '', Call TEXT, Reg TEXT, TypeCode TEXT,
        LastUpdated INTEGER NOT NULL, PRIMARY KEY (Hex));
    CREATE TABLE IF NOT EXISTS AircraftTypes (
        ICAO TEXT NOT NULL DEFAULT '', IATA TEXT NOT NULL DEFAULT '', Manufacturer TEXT, Model TEXT,
        Category INTEGER, LastUpdated INTEGER NOT NULL, PRIMARY KEY (ICAO, IATA));
    CREATE TABLE IF NOT EXISTS Airlines (
        ICAO TEXT NOT NULL DEFAULT '', IATA TEXT NOT NULL DEFAULT '', Airline TEXT, Country TEXT,
        LastUpdated INTEGER NOT NULL, PRIMARY KEY (ICAO, IATA));
    CREATE TABLE IF NOT EXISTS Airports (
        ICAO TEXT NOT NULL DEFAULT '', IATA TEXT NOT NULL DEFAULT '', Lat REAL, Lon REAL, Alt REAL,
        Airport TEXT, Country TEXT, LastUpdated INTEGER NOT NULL, PRIMARY KEY (ICAO, IATA));
    CREATE TABLE IF NOT EXISTS AircraftRegistrations (
        Prefix TEXT NOT NULL DEFAULT '', Country TEXT, Remarks TEXT,
        LastUpdated INTEGER NOT NULL, PRIMARY KEY (Prefix));
    CREATE INDEX IF NOT EXISTS idx_Aircrafts_Call ON Aircrafts (Call);
    CREATE INDEX IF NOT EXISTS idx_Aircrafts_Reg ON Aircrafts (Reg);
    CREATE TABLE IF NOT EXISTS Table_last_updated (
        TableName TEXT PRIMARY KEY NOT NULL, LastUpdated INTEGER NOT NULL DEFAULT 0)"];

fn secs(t: DateTime<Utc>) -> i64 {
    t.timestamp()
}
fn from_secs(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap_or_default()
}
fn text(r: &Row, i: usize) -> rusqlite::Result<String> {
    Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default())
}

mod category_int {
    use crate::PlaneCategory;
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PlaneCategory, D::Error> {
        Ok(PlaneCategory::from_i32(i32::deserialize(d)?).unwrap_or_default())
    }
}

/// A table row. `COLS` ends with `LastUpdated`; the first `KEY` columns form the primary key.
trait Entry: Sized {
    const TABLE: &'static str;
    const COLS: &'static [&'static str];
    const KEY: usize;
    fn values(&self) -> Vec<Value>;
    fn from_row(r: &Row) -> rusqlite::Result<Self>;
}

fn upsert_sql<E: Entry>() -> String {
    let cols = E::COLS.join(", ");
    let qs = (1..=E::COLS.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let key = E::COLS[..E::KEY].join(", ");
    let set = E::COLS[E::KEY..]
        .iter()
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {t} ({cols}) VALUES ({qs}) ON CONFLICT ({key}) DO UPDATE SET {set}
         WHERE excluded.LastUpdated > {t}.LastUpdated",
        t = E::TABLE
    )
}

/// `AircraftDesignator` (table `Aircrafts`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Aircraft {
    pub hex: String,
    #[serde(default)]
    pub call: String,
    #[serde(default)]
    pub reg: String,
    #[serde(default)]
    pub type_code: String,
    pub last_updated: DateTime<Utc>,
}

impl Entry for Aircraft {
    const TABLE: &'static str = "Aircrafts";
    const COLS: &'static [&'static str] = &["Hex", "Call", "Reg", "TypeCode", "LastUpdated"];
    const KEY: usize = 1;
    fn values(&self) -> Vec<Value> {
        vec![
            self.hex.clone().into(),
            self.call.clone().into(),
            self.reg.clone().into(),
            self.type_code.clone().into(),
            secs(self.last_updated).into(),
        ]
    }
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            hex: text(r, 0)?,
            call: text(r, 1)?,
            reg: text(r, 2)?,
            type_code: text(r, 3)?,
            last_updated: from_secs(r.get(4)?),
        })
    }
}

/// `AircraftTypeDesignator` (table `AircraftTypes`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AircraftType {
    #[serde(rename = "ICAO")]
    pub icao: String,
    #[serde(rename = "IATA", default)]
    pub iata: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(with = "category_int", default)]
    pub category: PlaneCategory,
    pub last_updated: DateTime<Utc>,
}

impl Entry for AircraftType {
    const TABLE: &'static str = "AircraftTypes";
    const COLS: &'static [&'static str] = &[
        "ICAO",
        "IATA",
        "Manufacturer",
        "Model",
        "Category",
        "LastUpdated",
    ];
    const KEY: usize = 2;
    fn values(&self) -> Vec<Value> {
        vec![
            self.icao.clone().into(),
            self.iata.clone().into(),
            self.manufacturer.clone().into(),
            self.model.clone().into(),
            (self.category as i64).into(),
            secs(self.last_updated).into(),
        ]
    }
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            icao: text(r, 0)?,
            iata: text(r, 1)?,
            manufacturer: text(r, 2)?,
            model: text(r, 3)?,
            category: PlaneCategory::from_i32(r.get::<_, Option<i32>>(4)?.unwrap_or(0))
                .unwrap_or_default(),
            last_updated: from_secs(r.get(5)?),
        })
    }
}

/// `AirlineDesignator` (table `Airlines`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Airline {
    #[serde(rename = "ICAO")]
    pub icao: String,
    #[serde(rename = "IATA", default)]
    pub iata: String,
    #[serde(default)]
    pub airline: String,
    #[serde(default)]
    pub country: String,
    pub last_updated: DateTime<Utc>,
}

impl Entry for Airline {
    const TABLE: &'static str = "Airlines";
    const COLS: &'static [&'static str] = &["ICAO", "IATA", "Airline", "Country", "LastUpdated"];
    const KEY: usize = 2;
    fn values(&self) -> Vec<Value> {
        vec![
            self.icao.clone().into(),
            self.iata.clone().into(),
            self.airline.clone().into(),
            self.country.clone().into(),
            secs(self.last_updated).into(),
        ]
    }
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            icao: text(r, 0)?,
            iata: text(r, 1)?,
            airline: text(r, 2)?,
            country: text(r, 3)?,
            last_updated: from_secs(r.get(4)?),
        })
    }
}

/// `AirportDesignator` (table `Airports`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Airport {
    #[serde(rename = "ICAO")]
    pub icao: String,
    #[serde(rename = "IATA", default)]
    pub iata: String,
    #[serde(default)]
    pub lat: f64,
    #[serde(default)]
    pub lon: f64,
    #[serde(default)]
    pub alt: f64,
    #[serde(default)]
    pub airport: String,
    #[serde(default)]
    pub country: String,
    pub last_updated: DateTime<Utc>,
}

impl Entry for Airport {
    const TABLE: &'static str = "Airports";
    const COLS: &'static [&'static str] = &[
        "ICAO",
        "IATA",
        "Lat",
        "Lon",
        "Alt",
        "Airport",
        "Country",
        "LastUpdated",
    ];
    const KEY: usize = 2;
    fn values(&self) -> Vec<Value> {
        vec![
            self.icao.clone().into(),
            self.iata.clone().into(),
            self.lat.into(),
            self.lon.into(),
            self.alt.into(),
            self.airport.clone().into(),
            self.country.clone().into(),
            secs(self.last_updated).into(),
        ]
    }
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            icao: text(r, 0)?,
            iata: text(r, 1)?,
            lat: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
            lon: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
            alt: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            airport: text(r, 5)?,
            country: text(r, 6)?,
            last_updated: from_secs(r.get(7)?),
        })
    }
}

/// `AircraftRegistrationDesignator` (table `AircraftRegistrations`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AircraftRegistration {
    pub prefix: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub remarks: String,
    pub last_updated: DateTime<Utc>,
}

impl Entry for AircraftRegistration {
    const TABLE: &'static str = "AircraftRegistrations";
    const COLS: &'static [&'static str] = &["Prefix", "Country", "Remarks", "LastUpdated"];
    const KEY: usize = 1;
    fn values(&self) -> Vec<Value> {
        vec![
            self.prefix.clone().into(),
            self.country.clone().into(),
            self.remarks.clone().into(),
            secs(self.last_updated).into(),
        ]
    }
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            prefix: text(r, 0)?,
            country: text(r, 1)?,
            remarks: text(r, 2)?,
            last_updated: from_secs(r.get(3)?),
        })
    }
}

fn from_json<T: for<'de> Deserialize<'de>>(json: &str) -> Result<Vec<T>, AircraftError> {
    Ok(serde_json::from_str(json.trim_start_matches('\u{feff}'))?)
}

/// `AircraftFromJSON`.
pub fn aircrafts_from_json(json: &str) -> Result<Vec<Aircraft>, AircraftError> {
    from_json(json)
}
/// `AircraftTypeFromJSON`.
pub fn aircraft_types_from_json(json: &str) -> Result<Vec<AircraftType>, AircraftError> {
    from_json(json)
}
/// `AirlineFromJSON`.
pub fn airlines_from_json(json: &str) -> Result<Vec<Airline>, AircraftError> {
    from_json(json)
}
/// `AirportFromJSON`.
pub fn airports_from_json(json: &str) -> Result<Vec<Airport>, AircraftError> {
    from_json(json)
}
/// `AircraftRegistrationFromJSON`.
pub fn aircraft_registrations_from_json(
    json: &str,
) -> Result<Vec<AircraftRegistration>, AircraftError> {
    from_json(json)
}

/// The aircraft database (one SQLite file, `aircrafts.db3`).
pub struct AircraftDb {
    conn: Mutex<Connection>,
    reg_min_len: AtomicUsize,
    type_icao_min_len: AtomicUsize,
}

impl AircraftDb {
    /// Open (and migrate) `<dir>/aircrafts.db3`.
    pub fn open(dir: &Path) -> Result<Self, AircraftError> {
        Self::init(as_db::open(&dir.join("aircrafts.db3"))?)
    }

    /// In-memory database (tests, temporary use).
    pub fn open_in_memory() -> Result<Self, AircraftError> {
        Self::init(as_db::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, AircraftError> {
        as_db::migrate(&conn, SCHEMA)?;
        let db = Self {
            conn: Mutex::new(conn),
            reg_min_len: AtomicUsize::new(0),
            type_icao_min_len: AtomicUsize::new(0),
        };
        db.refresh_lengths()?;
        Ok(db)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` with the connection (database maintenance: table list, row counts, clear, view).
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.conn())
    }

    /// Re-read the cached minimum lengths (C# keeps them as properties set at startup and on insert).
    fn refresh_lengths(&self) -> Result<(), AircraftError> {
        let conn = self.conn();
        let min = |sql: &str| -> rusqlite::Result<usize> {
            Ok(conn
                .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))?
                .unwrap_or(0)
                .max(0) as usize)
        };
        self.reg_min_len.store(
            min("SELECT min(length(Prefix)) FROM AircraftRegistrations")?,
            Ordering::Relaxed,
        );
        self.type_icao_min_len.store(
            min("SELECT min(length(ICAO)) FROM AircraftTypes")?,
            Ordering::Relaxed,
        );
        Ok(())
    }

    /// `AircraftRegistrationMinLength` (0 if the table is empty).
    pub fn registration_min_len(&self) -> usize {
        self.reg_min_len.load(Ordering::Relaxed)
    }

    /// `AircraftTypeICAOMinLength` (0 if the table is empty).
    pub fn type_icao_min_len(&self) -> usize {
        self.type_icao_min_len.load(Ordering::Relaxed)
    }

    fn count<E: Entry>(&self) -> Result<u64, AircraftError> {
        Ok(self
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {}", E::TABLE), [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }

    fn upsert<E: Entry>(&self, e: &E) -> Result<bool, AircraftError> {
        let n = self
            .conn()
            .prepare_cached(&upsert_sql::<E>())?
            .execute(params_from_iter(e.values()))?;
        self.refresh_lengths()?;
        Ok(n > 0)
    }

    fn bulk<E: Entry>(&self, es: &[E]) -> Result<usize, AircraftError> {
        let mut n = 0;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            {
                let mut st = tx.prepare_cached(&upsert_sql::<E>())?;
                for e in es {
                    n += st.execute(params_from_iter(e.values()))?;
                }
            }
            tx.commit()?;
        }
        self.refresh_lengths()?;
        Ok(n)
    }

    /// First row of `SELECT cols FROM table WHERE <where_> ORDER BY LastUpdated DESC LIMIT 1`.
    fn find<E: Entry>(&self, where_: &str, args: &[&str]) -> Result<Option<E>, AircraftError> {
        let sql = format!(
            "SELECT {} FROM {} WHERE {where_} ORDER BY LastUpdated DESC LIMIT 1",
            E::COLS.join(", "),
            E::TABLE
        );
        Ok(self
            .conn()
            .prepare_cached(&sql)?
            .query_row(params_from_iter(args.iter()), E::from_row)
            .optional()?)
    }

    /// Every row of the table, in table order (`SELECT *` without `ORDER BY`).
    fn all<E: Entry>(&self) -> Result<Vec<E>, AircraftError> {
        let sql = format!("SELECT {} FROM {}", E::COLS.join(", "), E::TABLE);
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map([], E::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<E>>>()?)
    }

    /// `AircraftCount`.
    pub fn aircraft_count(&self) -> Result<u64, AircraftError> {
        self.count::<Aircraft>()
    }
    /// `AircraftInsertOrUpdateIfNewer`; `true` if a row was written.
    pub fn aircraft_insert_or_update_if_newer(&self, a: &Aircraft) -> Result<bool, AircraftError> {
        self.upsert(a)
    }
    /// `AircraftBulkInsertOrUpdateIfNewer` (one transaction); number of rows written.
    pub fn aircraft_bulk_insert_or_update_if_newer(
        &self,
        a: &[Aircraft],
    ) -> Result<usize, AircraftError> {
        self.bulk(a)
    }
    /// `AircraftFindByHex`.
    pub fn aircraft_find_by_hex(&self, hex: &str) -> Result<Option<Aircraft>, AircraftError> {
        self.find("Hex = ?1", &[hex])
    }
    /// `AircraftFindByReg` (newest row).
    pub fn aircraft_find_by_reg(&self, reg: &str) -> Result<Option<Aircraft>, AircraftError> {
        self.find("Reg = ?1", &[reg])
    }
    /// `AircraftFindByCall` (newest row).
    pub fn aircraft_find_by_call(&self, call: &str) -> Result<Option<Aircraft>, AircraftError> {
        self.find("Call = ?1", &[call])
    }

    /// `AircraftTypeCount`.
    pub fn aircraft_type_count(&self) -> Result<u64, AircraftError> {
        self.count::<AircraftType>()
    }
    /// `AircraftTypeInsertOrUpdateIfNewer`.
    pub fn aircraft_type_insert_or_update_if_newer(
        &self,
        t: &AircraftType,
    ) -> Result<bool, AircraftError> {
        self.upsert(t)
    }
    /// `AircraftTypeBulkInsertOrUpdateIfNewer`.
    pub fn aircraft_type_bulk_insert_or_update_if_newer(
        &self,
        t: &[AircraftType],
    ) -> Result<usize, AircraftError> {
        self.bulk(t)
    }
    /// `AircraftTypeFindByICAO` (newest row).
    pub fn aircraft_type_find_by_icao(
        &self,
        icao: &str,
    ) -> Result<Option<AircraftType>, AircraftError> {
        self.find("ICAO = ?1", &[icao])
    }
    /// `AircraftTypeFindByIATA` (newest row).
    pub fn aircraft_type_find_by_iata(
        &self,
        iata: &str,
    ) -> Result<Option<AircraftType>, AircraftError> {
        self.find("IATA = ?1", &[iata])
    }

    /// `AirlineCount`.
    pub fn airline_count(&self) -> Result<u64, AircraftError> {
        self.count::<Airline>()
    }
    /// `AirlineInsertOrUpdateIfNewer`.
    pub fn airline_insert_or_update_if_newer(&self, a: &Airline) -> Result<bool, AircraftError> {
        self.upsert(a)
    }
    /// `AirlineBulkInsertOrUpdateIfNewer`.
    pub fn airline_bulk_insert_or_update_if_newer(
        &self,
        a: &[Airline],
    ) -> Result<usize, AircraftError> {
        self.bulk(a)
    }
    /// `AirlineFindByICAO` (newest row).
    pub fn airline_find_by_icao(&self, icao: &str) -> Result<Option<Airline>, AircraftError> {
        self.find("ICAO = ?1", &[icao])
    }
    /// `AirlineFindByIATA` (newest row).
    pub fn airline_find_by_iata(&self, iata: &str) -> Result<Option<Airline>, AircraftError> {
        self.find("IATA = ?1", &[iata])
    }

    /// `AirportCount`.
    pub fn airport_count(&self) -> Result<u64, AircraftError> {
        self.count::<Airport>()
    }
    /// `AirportInsertOrUpdateIfNewer`.
    pub fn airport_insert_or_update_if_newer(&self, a: &Airport) -> Result<bool, AircraftError> {
        self.upsert(a)
    }
    /// `AirportBulkInsertOrUpdateIfNewer`.
    pub fn airport_bulk_insert_or_update_if_newer(
        &self,
        a: &[Airport],
    ) -> Result<usize, AircraftError> {
        self.bulk(a)
    }
    /// `AirportGetAll` (`AirScout.Aircrafts/AircraftDatabase.cs:1837-1847`): all rows in
    /// table order.
    pub fn airport_get_all(&self) -> Result<Vec<Airport>, AircraftError> {
        self.all()
    }
    /// `AirportFindByICAO` (newest row).
    pub fn airport_find_by_icao(&self, icao: &str) -> Result<Option<Airport>, AircraftError> {
        self.find("ICAO = ?1", &[icao])
    }
    /// `AirportFindByIATA` (newest row).
    pub fn airport_find_by_iata(&self, iata: &str) -> Result<Option<Airport>, AircraftError> {
        self.find("IATA = ?1", &[iata])
    }

    /// `AircraftRegistrationCount`.
    pub fn registration_count(&self) -> Result<u64, AircraftError> {
        self.count::<AircraftRegistration>()
    }
    /// `AircraftRegistrationInsertOrUpdateIfNewer`.
    pub fn registration_insert_or_update_if_newer(
        &self,
        r: &AircraftRegistration,
    ) -> Result<bool, AircraftError> {
        self.upsert(r)
    }
    /// `AircraftRegistrationBulkInsertOrUpdateIfNewer`.
    pub fn registration_bulk_insert_or_update_if_newer(
        &self,
        r: &[AircraftRegistration],
    ) -> Result<usize, AircraftError> {
        self.bulk(r)
    }
    /// `AircraftRegistrationFind`: exact prefix.
    pub fn registration_find(
        &self,
        prefix: &str,
    ) -> Result<Option<AircraftRegistration>, AircraftError> {
        self.find("Prefix = ?1", &[prefix])
    }

    /// `AircraftRegistrationFindByReg`, see [`crate::check::find_reg_prefix`].
    pub fn registration_find_by_reg(
        &self,
        reg: &str,
    ) -> Result<Option<AircraftRegistration>, AircraftError> {
        match find_reg_prefix(reg, |p| self.registration_find(p).map(|r| r.is_some()))? {
            Some(p) => self.registration_find(&p),
            None => Ok(None),
        }
    }

    /// `GetTableLastUpdatedTimeStamp` for the last successful import; `None` if never.
    pub fn table_timestamp(&self, table: &str) -> Result<Option<DateTime<Utc>>, AircraftError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT LastUpdated FROM Table_last_updated WHERE TableName = ?1",
                params![table],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(from_secs))
    }

    /// `SetTableLastUpdatedTimeStamp`.
    pub fn set_table_timestamp(&self, table: &str, t: DateTime<Utc>) -> Result<(), AircraftError> {
        self.conn().execute(
            "INSERT INTO Table_last_updated (TableName, LastUpdated) VALUES (?1, ?2)
             ON CONFLICT (TableName) DO UPDATE SET LastUpdated = excluded.LastUpdated",
            params![table, secs(t)],
        )?;
        Ok(())
    }
}

impl AircraftDb {
    /// `PlaneInfoBulkInsertOrUpdateIfNewer(caller, planes)` (`AircraftDatabase.cs:2368-2410`, the
    /// overload `PlaneFeed` uses): for planes with hex, reg and type, write
    /// `Aircrafts(hex, call, reg, type, time)` if newer; for every plane with a type, write
    /// `AircraftTypes(ICAO = type, IATA = "", manufacturer, model, category, now)`
    /// (logic-findings #18). One transaction; rolled back on error. Returns rows written
    /// (C# always returns 0).
    pub fn plane_info_bulk_upsert(&self, planes: &[PlaneInfo]) -> Result<usize, AircraftError> {
        let now = Utc::now();
        let mut n = 0;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            {
                let mut ac = tx.prepare_cached(&upsert_sql::<Aircraft>())?;
                let mut ty = tx.prepare_cached(&upsert_sql::<AircraftType>())?;
                for p in planes {
                    if !p.hex.is_empty() && !p.reg.is_empty() && !p.type_code.is_empty() {
                        let a = Aircraft {
                            hex: p.hex.clone(),
                            call: p.call.clone(),
                            reg: p.reg.clone(),
                            type_code: p.type_code.clone(),
                            last_updated: p.time,
                        };
                        n += ac.execute(params_from_iter(a.values()))?;
                    }
                    if !p.type_code.is_empty() {
                        let t = AircraftType {
                            icao: p.type_code.clone(),
                            iata: String::new(),
                            manufacturer: p.manufacturer.clone(),
                            model: p.model.clone(),
                            category: p.category,
                            last_updated: now,
                        };
                        n += ty.execute(params_from_iter(t.values()))?;
                    }
                }
            }
            tx.commit()?;
        }
        self.refresh_lengths()?;
        Ok(n)
    }
}

impl Lookup for AircraftDb {
    fn airline_by_icao(&self, icao: &str) -> bool {
        matches!(self.airline_find_by_icao(icao), Ok(Some(_)))
    }
    fn airline_by_iata(&self, iata: &str) -> bool {
        matches!(self.airline_find_by_iata(iata), Ok(Some(_)))
    }
    fn registration_by_reg(&self, reg: &str) -> bool {
        matches!(self.registration_find_by_reg(reg), Ok(Some(_)))
    }
    fn airport_by_icao(&self, icao: &str) -> bool {
        matches!(self.airport_find_by_icao(icao), Ok(Some(_)))
    }
    fn type_by_icao(&self, icao: &str) -> bool {
        matches!(self.aircraft_type_find_by_icao(icao), Ok(Some(_)))
    }
    fn registration_min_len(&self) -> usize {
        AircraftDb::registration_min_len(self)
    }
    fn type_icao_min_len(&self) -> usize {
        AircraftDb::type_icao_min_len(self)
    }
}
