//! Database maintenance jobs: sizes, table lists, clearing and viewing tables, deleting the
//! precalculated paths, the map tile cache and single stations (`OptionsDlg.cs:296-520`,
//! `DatabaseMaintenanceDlg.cs`, `DeleteSingleStationDlg.cs`).
//!
//! Every destructive job is only started by the app after an explicit confirmation. Table names
//! reach SQL only through `as_db` (checked against `sqlite_master`, quoted).

use crate::coverage;
use crate::jobs::{JobCtx, JobEvent};
use crate::services::Paths;
use crate::tiles;
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::PositionDb;
use as_elevation::ElevationModel;
use as_geo::band::Band;
use as_geo::latlon::GRect;
use as_signals::SignalDb;
use std::path::{Path, PathBuf};

/// Rows the read-only table view loads (the C# loads all; not reproduced).
pub const VIEW_LIMIT: usize = 10_000;

/// A database of the data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DbId {
    Stations,
    Aircraft,
    Positions,
    Signals,
    Elevation(ElevationModel),
    Propagation(ElevationModel),
}

impl DbId {
    /// All databases, in the order of the Options tab.
    pub fn all() -> Vec<DbId> {
        let mut v = vec![
            DbId::Stations,
            DbId::Aircraft,
            DbId::Positions,
            DbId::Signals,
        ];
        v.extend(ElevationModel::ALL.map(DbId::Elevation));
        v.extend(ElevationModel::ALL.map(DbId::Propagation));
        v
    }

    /// File name inside the data directory.
    pub fn file_name(self) -> String {
        match self {
            DbId::Stations => "stations.db3".into(),
            DbId::Aircraft => "aircrafts.db3".into(),
            DbId::Positions => "aircraftpositions.db3".into(),
            DbId::Signals => "signals.db3".into(),
            DbId::Elevation(m) => m.db_file_name(),
            DbId::Propagation(m) => format!("propagation_{}", m.db_file_name()),
        }
    }

    /// `Database.Name`, verbatim (the C# aircraft name is overwritten by a copy-paste, finding #140).
    pub fn title(self) -> String {
        match self {
            DbId::Stations => "ScoutBase Station Database".into(),
            DbId::Aircraft => "AirScout Aircraft Database".into(),
            DbId::Positions => "AirScout Aircraft Position Database".into(),
            DbId::Signals => "Scoutbase Signal Database".into(),
            DbId::Elevation(_) => "ScoutBase Elevation Database".into(),
            DbId::Propagation(_) => "ScoutBase Propagation Database".into(),
        }
    }

    /// [`Self::title`] with the elevation model, to tell the databases of one kind apart.
    pub fn label(self) -> String {
        match self {
            DbId::Elevation(m) | DbId::Propagation(m) => {
                format!("{} {}", self.title(), m.dir_name())
            }
            _ => self.title(),
        }
    }

    /// `Database.Description` (`StationDatabase.cs:48-52`, `AircraftDatabase.cs:50-54`,
    /// `ElevationDatabase.cs:125-130`, `PropagationDatabase.cs:90-94`).
    pub fn description(self) -> &'static str {
        match self {
            DbId::Stations => {
                "The Scoutbase Station Database is containing location and QRV infos. Each information has a unique key: \n\
                 callsign and 6-digit Maidenhead locator square. \n\
                 The database is periodically updated from a global web resource. The user can modify / add entries via user interface."
            }
            DbId::Aircraft => {
                "Holds all aircraft, airline and airport information.\n\
                 The information is updated periodically from a global web resource.\n\
                 The user cannot change any item."
            }
            DbId::Positions => "Holds aircraft position history.",
            DbId::Signals => "Holds recorded signal strength information",
            DbId::Elevation(_) => {
                "The Scoutbase Elevation Database is containing various elevation information.\n\
                 The basic elevation information is kept unique per  6digit-Maidenhead Locator and is updated periodically from a global web resource.\n\
                 The path and horizon information are unique per one oder between two geographical locations.\n\
                 These values are (pre-)calculated and stored at runtime.\n\
                 All values are based on a distinct elevation model either GLOBE, SRTM3, SRTM1 or ASTER."
            }
            DbId::Propagation(_) => {
                "The Scoutbase Propagation Database is containing propagation path and horizon information.\n\
                 The info is unique for one single or between two geographical locations, heights, frequency, F1-Clearance and calculation stepwidth.\n\
                 All calculations are based on a distinct elevation model GLOBE, SRTM3, SRTM1 or ASTER.\n\
                 All values are (pre-)calculated and stored at runtime."
            }
        }
    }
}

/// What the maintenance dialogs ask a worker thread to do.
#[derive(Debug, Clone, PartialEq)]
pub enum MaintenanceJob {
    /// File path and size [MB] of every database.
    Sizes,
    /// Tables of a database with description and row count.
    Tables(DbId),
    /// Delete all rows of a table; answered with the fresh table list.
    ClearTable(DbId, String),
    /// The first [`VIEW_LIMIT`] rows of a table, without BLOB columns.
    View(DbId, String),
    /// `ElevationPathDeleteAll` for all five models.
    DeleteElevationPaths,
    /// `PropagationPathDeleteAll` for all five models.
    DeletePropagationPaths,
    /// Remove the map tile cache directory.
    DeleteMapTiles,
    /// Remove the location and all QRV entries of one station.
    DeleteStation { call: String, loc: String },
    /// Stations whose call matches the LIKE pattern.
    Locations(String),
    /// Which tiles of `model`'s catalogue on `base_url` (restricted to `area`) the database has
    /// (`OptionsDlg.cs:1356-1461`). Downloads the small `files.zip`; read-only otherwise. The
    /// app starts it only from the "Show coverage" button.
    Coverage {
        model: ElevationModel,
        base_url: String,
        area: GRect,
    },
    /// The elevation of the grid cells over `rect` for `model` (`MapStationDlg.cs:288-336`).
    ElevationGrid { rect: GRect, model: ElevationModel },
}

impl MaintenanceJob {
    /// True if the job removes data: it only runs after a confirmation.
    pub fn destructive(&self) -> bool {
        matches!(
            self,
            MaintenanceJob::ClearTable(..)
                | MaintenanceJob::DeleteElevationPaths
                | MaintenanceJob::DeletePropagationPaths
                | MaintenanceJob::DeleteMapTiles
                | MaintenanceJob::DeleteStation { .. }
        )
    }

    /// The question of the confirmation (`OptionsDlg.cs:376-401`, `DatabaseMaintenanceDlg.cs:53`,
    /// `:68`). The C# asks nothing before deleting map tiles; the port does. `View` only reads,
    /// but asks as the C# does (it may be slow).
    pub fn confirmation(&self) -> Option<(&'static str, String)> {
        Some(match self {
            MaintenanceJob::ClearTable(_, t) => (
                "Delete All Entries",
                format!("Do you really want to delete all entries of table {t}?"),
            ),
            MaintenanceJob::View(..) => (
                "Database Edit",
                "CAUTION! Pressing \"View\" on large databases may take a long time to open the edit dialog.\n\nDo you really want to continue?".into(),
            ),
            MaintenanceJob::DeleteElevationPaths => (
                "Delete all elevation paths",
                "Are you sure to delete all precalculated elevation paths from database?".into(),
            ),
            MaintenanceJob::DeletePropagationPaths => (
                "Delete all propagation paths",
                "Are you sure to delete all precalculated propagation paths from database?".into(),
            ),
            MaintenanceJob::DeleteMapTiles => (
                "Delete all map tiles",
                "Are you sure to delete all cached map tiles? They are downloaded again when needed.".into(),
            ),
            MaintenanceJob::DeleteStation { call, loc } => (
                "Delete Single Station",
                format!("Do you really want to delete {call} {loc} from the station database?"),
            ),
            _ => return None,
        })
    }
}

/// `TableDescriptions` of the databases (`ElevationDatabase.cs:133-136`,
/// `StationDatabase.cs:53-55`, `PropagationDatabase.cs:95-96`, `AircraftDatabase.cs:58-62`).
/// Other tables give `"[not found]"`.
pub fn table_description(table: &str) -> &'static str {
    match table {
        "Elevation" => "Holds elevation information per 6digit Maidenhead Locator.",
        "ElevationPath" => "Holds elevation path information between two locations.",
        "ElevationHorizon" => "Holds elevation horizon information for one location.",
        "LocalObstruction" => "Holds local obstruction information for one location.",
        "Location" => "Holds location information.",
        "QRV" => "Holds QRV information.",
        "Table_last_updated" => "Holds Timestamps of last update.",
        "PropagationPath" => "Holds propagation path information.",
        "PropagationHorizon" => "Holds propagation horizon information.",
        "AircraftRegistrations" => "Holds aircraft registration information.",
        "AircraftTypes" => "Holds aircraft type information.",
        "Aircrafts" => "Holds basic aircraft information.",
        "Airlines" => "Holds airline information.",
        "Airports" => "Holds airport information.",
        _ => "[not found]",
    }
}

/// The LIKE pattern of the Delete Single Station call field: the text upper-cased and trimmed,
/// at least two characters (`DeleteSingleStationDlg.cs:34-40, :58`).
pub fn locations_pattern(text: &str) -> Option<String> {
    let t = text.trim().to_uppercase();
    (t.chars().count() >= 2).then(|| format!("%{t}%"))
}

/// A path shortened to its last two components for the Options tab (`GetDatabaseDir`).
pub fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.len() <= 2 {
        path.to_string()
    } else {
        format!(".../{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run `job` in the calling thread, reporting through `ctx`. Errors become `JobEvent::Failed`.
pub fn run(ctx: &JobCtx, paths: &Paths, job: MaintenanceJob) {
    let r = match job {
        MaintenanceJob::Sizes => sizes(ctx, &paths.data_dir),
        MaintenanceJob::Tables(db) => tables(db, paths).map(|t| ctx.send(JobEvent::Tables(db, t))),
        MaintenanceJob::ClearTable(db, table) => clear_table(ctx, paths, db, &table),
        MaintenanceJob::View(db, table) => view(ctx, paths, db, &table),
        MaintenanceJob::DeleteElevationPaths => delete_paths(ctx, paths, true),
        MaintenanceJob::DeletePropagationPaths => delete_paths(ctx, paths, false),
        MaintenanceJob::DeleteMapTiles => delete_map_tiles(ctx, &paths.data_dir),
        MaintenanceJob::DeleteStation { call, loc } => delete_station(ctx, paths, &call, &loc),
        MaintenanceJob::Locations(pattern) => paths
            .stations
            .locations_like(&pattern)
            .map(|v| {
                let v = v.into_iter().map(|l| (l.call, l.loc)).collect();
                ctx.send(JobEvent::Locations(v));
            })
            .map_err(err),
        MaintenanceJob::Coverage {
            model,
            base_url,
            area,
        } => coverage::run_coverage(ctx, paths, model, &base_url, area),
        MaintenanceJob::ElevationGrid { rect, model } => {
            coverage::run_grid(ctx, paths, rect, model)
        }
    };
    if let Err(e) = r {
        ctx.send(JobEvent::Failed(e));
    }
}

/// Run `f` with the connection of `db`. A database file that does not exist is not created.
fn with_db<R>(
    paths: &Paths,
    db: DbId,
    f: impl FnOnce(&as_db::rusqlite::Connection) -> as_db::rusqlite::Result<R>,
) -> Result<R, String> {
    let dir = &paths.data_dir;
    match db {
        DbId::Stations => paths.stations.with_conn(f).map_err(err),
        DbId::Elevation(m) => paths
            .store
            .with_conn(m, |c| f(c).map_err(as_elevation::ElevationError::from))
            .map_err(err),
        DbId::Propagation(m) => paths.engine.with_conn(m, f).map_err(err)?.map_err(err),
        DbId::Aircraft => {
            existing(dir, db)?;
            AircraftDb::open(dir)
                .map_err(err)?
                .with_conn(f)
                .map_err(err)
        }
        DbId::Positions => {
            existing(dir, db)?;
            PositionDb::open(dir)
                .map_err(err)?
                .with_conn(f)
                .map_err(err)
        }
        DbId::Signals => {
            existing(dir, db)?;
            SignalDb::open(dir).map_err(err)?.with_conn(f).map_err(err)
        }
    }
}

fn existing(dir: &Path, db: DbId) -> Result<(), String> {
    if dir.join(db.file_name()).is_file() {
        Ok(())
    } else {
        Err(format!("{} does not exist yet", db.file_name()))
    }
}

fn sizes(ctx: &JobCtx, dir: &Path) -> Result<(), String> {
    let v = DbId::all()
        .into_iter()
        .map(|db| {
            let p = dir.join(db.file_name());
            let mb = std::fs::metadata(&p).map_or(0.0, |m| m.len() as f64 / 1024.0 / 1024.0);
            (db, p.display().to_string(), mb)
        })
        .collect();
    ctx.send(JobEvent::Sizes(v));
    Ok(())
}

fn tables(db: DbId, paths: &Paths) -> Result<Vec<(String, String, u64)>, String> {
    with_db(paths, db, |c| {
        as_db::table_list(c)?
            .into_iter()
            .map(|t| {
                let n = as_db::row_count(c, &t)?;
                Ok((t.clone(), table_description(&t).to_string(), n))
            })
            .collect()
    })
}

fn clear_table(ctx: &JobCtx, paths: &Paths, db: DbId, table: &str) -> Result<(), String> {
    if ctx.cancelled() {
        return Ok(());
    }
    let n = with_db(paths, db, |c| as_db::clear_table(c, table))?;
    ctx.progress(format!("{n} entries deleted from table {table}"));
    ctx.send(JobEvent::Tables(db, tables(db, paths)?));
    Ok(())
}

fn view(ctx: &JobCtx, paths: &Paths, db: DbId, table: &str) -> Result<(), String> {
    let (columns, rows) = with_db(paths, db, |c| as_db::select_non_blob(c, table, VIEW_LIMIT))?;
    ctx.send(JobEvent::View { columns, rows });
    Ok(())
}

/// All five models; the C# does GLOBE, SRTM3 and SRTM1 only (`OptionsDlg.cs:385-403`; finding).
fn delete_paths(ctx: &JobCtx, paths: &Paths, elevation: bool) -> Result<(), String> {
    let mut total = 0;
    for m in ElevationModel::ALL {
        if ctx.cancelled() {
            return Ok(());
        }
        total += if elevation {
            paths.store.elevation_path_delete_all(m).map_err(err)?
        } else {
            paths.engine.path_delete_all(m).map_err(err)?
        };
    }
    let what = if elevation {
        "elevation"
    } else {
        "propagation"
    };
    ctx.send(JobEvent::Done(format!("{total} {what} paths deleted")));
    Ok(())
}

fn delete_map_tiles(ctx: &JobCtx, data_dir: &Path) -> Result<(), String> {
    if ctx.cancelled() {
        return Ok(());
    }
    clear_tile_cache(data_dir)?;
    ctx.send(JobEvent::Done("map tile cache deleted".into()));
    Ok(())
}

/// The directory `data_dir/tiles` if it is safe to remove: `data_dir` is a non-empty, non-root,
/// non-home path and the cache is exactly `tiles_cache_dir(data_dir)`.
fn safe_tile_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let dir = tiles::tiles_cache_dir(data_dir);
    let bad = data_dir_unsafe(data_dir)
        || dir.file_name().and_then(|n| n.to_str()) != Some("tiles")
        || dir.parent() != Some(data_dir);
    if bad {
        return Err(format!(
            "refusing to delete tiles in {}",
            data_dir.display()
        ));
    }
    Ok(dir)
}

/// A data directory nothing may be deleted in: empty, relative, a root or the home directory.
pub(crate) fn data_dir_unsafe(data_dir: &Path) -> bool {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    data_dir.as_os_str().is_empty()
        || !data_dir.is_absolute()
        || data_dir.parent().is_none()
        || home.is_some_and(|h| Path::new(&h) == data_dir)
}

/// Remove the walkers tile cache `data_dir/tiles` and nothing else. A symlink in its place is
/// refused, so it cannot lead out of the data directory. `remove_dir_all` does not follow links
/// inside the cache either: it removes the links.
pub fn clear_tile_cache(data_dir: &Path) -> Result<(), String> {
    let dir = safe_tile_dir(data_dir)?;
    // a file where the data directory should be: Unix reports "not a directory" for the path
    // below it, Windows reports "not found"; refuse it the same way on every OS
    if std::fs::metadata(data_dir).is_ok_and(|m| !m.is_dir()) {
        return Err(format!("{} is not a directory", data_dir.display()));
    }
    match std::fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(err(e)),
        Ok(m) if m.file_type().is_symlink() => {
            Err(format!("{} is a symbolic link: not deleted", dir.display()))
        }
        Ok(m) if !m.is_dir() => Err(format!("{} is not a directory", dir.display())),
        Ok(_) => std::fs::remove_dir_all(&dir).map_err(err),
    }
}

fn delete_station(ctx: &JobCtx, paths: &Paths, call: &str, loc: &str) -> Result<(), String> {
    if call.trim().is_empty() || loc.trim().is_empty() {
        return Err("call and locator are required".into());
    }
    if ctx.cancelled() {
        return Ok(());
    }
    let st = &paths.stations;
    let l = st.location_delete(call, loc).map_err(err)?;
    let q = st.qrv_delete(call, loc, Band::All).map_err(err)?;
    ctx.send(JobEvent::Done(if l || q {
        format!("{call} {loc} deleted")
    } else {
        format!("{call} {loc} not found")
    }));
    Ok(())
}
