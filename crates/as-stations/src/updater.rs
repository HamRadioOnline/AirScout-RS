//! Port of `StationDatabaseUpdater.cs` (one update run; scheduling is the caller's job).

use crate::{locations_from_json, qrvs_from_json, StationDb, StationError};
use as_geo::latlon::GRect;
use chrono::Utc;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// `Stations_UpdateURL` default.
pub const DEFAULT_UPDATE_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/StationData/";

/// Rows written by one update run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub locations: usize,
    pub qrvs: usize,
}

fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

fn in_area(area: &GRect, lat: f64, lon: f64) -> bool {
    !(lat < area.min_lat || lat > area.max_lat || lon < area.min_lon || lon > area.max_lon)
}

/// Import when the local zip is newer than the last successful import of `table` (or there was
/// none). Unlike "downloaded in this run", this retries a file whose import failed or was cancelled.
fn needs_import(db: &StationDb, table: &str, zip: &Path) -> Result<bool, StationError> {
    let Some(imported) = db.table_timestamp(table)? else {
        return Ok(true);
    };
    let mtime: chrono::DateTime<Utc> = std::fs::metadata(zip)?.modified()?.into();
    Ok(mtime > imported)
}

fn read_zip_entry(zip_path: &Path, name: &str) -> Result<String, StationError> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut s = String::new();
    z.by_name(name)?.read_to_string(&mut s)?;
    Ok(s)
}

/// Download `locations.zip` and `qrv.zip` from `base_url` (ending in `/`) into `cache_dir` and
/// merge them into `db` (insert or update if newer). A file is parsed when the local copy is newer
/// than the last successful import of its table, so a failed or cancelled import is retried. With `area`, locations outside it are skipped, and QRV
/// entries whose location is unknown or outside it.
///
/// The original fetches a ZIP password from a key server first; the files on the server are
/// not encrypted, so that step is not ported.
pub fn update_stations(
    client: &reqwest::blocking::Client,
    base_url: &str,
    cache_dir: &Path,
    db: &StationDb,
    area: Option<GRect>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(&str),
) -> Result<UpdateStats, StationError> {
    std::fs::create_dir_all(cache_dir)?;
    let mut stats = UpdateStats::default();

    progress("Updating locations from web database...");
    let zip = cache_dir.join("locations.zip");
    as_net::download_if_newer(client, &format!("{base_url}locations.zip"), &zip)?;
    if needs_import(db, "Location", &zip)? {
        let mut ls = locations_from_json(&read_zip_entry(&zip, "locations.json")?)?;
        if let Some(a) = &area {
            ls.retain(|l| in_area(a, l.lat, l.lon));
        }
        if cancelled(cancel) {
            return Err(StationError::Cancelled);
        }
        stats.locations = db.location_bulk_insert_or_update_if_newer(&ls)?;
        db.set_table_timestamp("Location", Utc::now())?;
        progress(&format!("{} locations updated.", stats.locations));
    }
    if cancelled(cancel) {
        return Err(StationError::Cancelled);
    }

    progress("Updating qrv info from web database...");
    let zip = cache_dir.join("qrv.zip");
    as_net::download_if_newer(client, &format!("{base_url}qrv.zip"), &zip)?;
    if needs_import(db, "QRV", &zip)? {
        let mut qs = qrvs_from_json(&read_zip_entry(&zip, "qrv.json")?)?;
        if let Some(a) = &area {
            let mut keep = Vec::with_capacity(qs.len());
            for q in qs {
                if let Some(l) = db.location_find(&q.call, Some(&q.loc))? {
                    if in_area(a, l.lat, l.lon) {
                        keep.push(q);
                    }
                }
                if cancelled(cancel) {
                    return Err(StationError::Cancelled);
                }
            }
            qs = keep;
        }
        stats.qrvs = db.qrv_bulk_insert_or_update_if_newer(&qs)?;
        db.set_table_timestamp("QRV", Utc::now())?;
        progress(&format!("{} QRV entries updated.", stats.qrvs));
    }
    progress("Station database update completed.");
    Ok(stats)
}
