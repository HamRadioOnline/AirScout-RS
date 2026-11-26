//! Port of `AirScout.Aircrafts/AircraftDatabaseUpdater.cs` (one update run; scheduling is the
//! caller's job).
//!
//! The original fetches a ZIP password from a key server first; the files on the server are not
//! encrypted, so that step is not ported.

use crate::db::{
    aircraft_registrations_from_json, aircraft_types_from_json, aircrafts_from_json,
    airlines_from_json, airports_from_json, AircraftDb,
};
use crate::AircraftError;
use chrono::Utc;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// `Aircrafts_UpdateURL` default.
pub const DEFAULT_UPDATE_URL: &str = "http://www.airscout.eu/downloads/AirScout/1/AircraftData/";

/// The files on the server, in the order the original reads them. Each `<name>.zip` holds
/// `<name>.json`.
pub const FILES: [&str; 5] = [
    "AircraftTypes",
    "Airports",
    "Aircrafts",
    "AircraftRegistrations",
    "Airlines",
];

/// Rows written by one update run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub aircrafts: usize,
    pub aircraft_types: usize,
    pub airlines: usize,
    pub airports: usize,
    pub registrations: usize,
}

fn read_zip_entry(zip_path: &Path, name: &str) -> Result<String, AircraftError> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut s = String::new();
    z.by_name(name)?.read_to_string(&mut s)?;
    Ok(s)
}

/// Import when the local zip is newer than the last successful import of `table`. The original
/// re-imports on every run; the result is the same because rows are only written if newer.
fn needs_import(db: &AircraftDb, table: &str, zip: &Path) -> Result<bool, AircraftError> {
    let Some(imported) = db.table_timestamp(table)? else {
        return Ok(true);
    };
    let mtime: chrono::DateTime<Utc> = std::fs::metadata(zip)?.modified()?.into();
    Ok(mtime > imported)
}

/// Download the five zips from `base_url` (ending in `/`) into `tmp_dir` and merge them into `db`.
/// Empty `Call`, `Reg` and `TypeCode` of aircraft become `"[unknown]"`, as in the original.
pub fn update_aircraft_db(
    client: &reqwest::blocking::Client,
    base_url: &str,
    tmp_dir: &Path,
    db: &AircraftDb,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(&str),
) -> Result<UpdateStats, AircraftError> {
    std::fs::create_dir_all(tmp_dir)?;
    let mut stats = UpdateStats::default();
    for name in FILES {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(AircraftError::Cancelled);
        }
        progress(&format!("Updating {name} from web database..."));
        let zip = tmp_dir.join(format!("{name}.zip"));
        as_net::download_if_newer(client, &format!("{base_url}{name}.zip"), &zip)?;
        if !needs_import(db, name, &zip)? {
            continue;
        }
        let json = read_zip_entry(&zip, &format!("{name}.json"))?;
        let n = match name {
            "AircraftTypes" => {
                let v = aircraft_types_from_json(&json)?;
                stats.aircraft_types = db.aircraft_type_bulk_insert_or_update_if_newer(&v)?;
                stats.aircraft_types
            }
            "Airports" => {
                let v = airports_from_json(&json)?;
                stats.airports = db.airport_bulk_insert_or_update_if_newer(&v)?;
                stats.airports
            }
            "Aircrafts" => {
                let mut v = aircrafts_from_json(&json)?;
                for a in &mut v {
                    for f in [&mut a.call, &mut a.reg, &mut a.type_code] {
                        if f.is_empty() {
                            *f = "[unknown]".to_string();
                        }
                    }
                }
                stats.aircrafts = db.aircraft_bulk_insert_or_update_if_newer(&v)?;
                stats.aircrafts
            }
            "AircraftRegistrations" => {
                let v = aircraft_registrations_from_json(&json)?;
                stats.registrations = db.registration_bulk_insert_or_update_if_newer(&v)?;
                stats.registrations
            }
            _ => {
                let v = airlines_from_json(&json)?;
                stats.airlines = db.airline_bulk_insert_or_update_if_newer(&v)?;
                stats.airlines
            }
        };
        db.set_table_timestamp(name, Utc::now())?;
        progress(&format!("{n} {name} entries updated."));
    }
    progress("Aircraft database update completed.");
    Ok(stats)
}
