//! Job bodies of the analysis side: the history data getter (`bw_Analysis_DataGetter`,
//! MapDlg.cs:9340-9418) and the position file jobs.

use crate::app::PathEntry;
use crate::crossing_csv;
use crate::crossing_history::{self, CrossingParams, CrossingResult};
use crate::exports::{self, HexOrCall};
use crate::history::HistoryData;
use crate::history_download::{self, DownloadRequest, Fs4Disk};
use crate::jobs::{JobCtx, JobEvent};
use crate::traffic;
use as_aircraft::db::AircraftDb;
use as_aircraft::position_files::{self, ImportStats};
use as_aircraft::positions::PositionDb;
use as_aircraft::{AircraftError, PlaneCategory, PlaneInfo};
use as_elevation::ElevationModel;
use as_settings::Settings;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A job of the analysis side. `Services::run_analysis` runs one at a time.
#[derive(Debug, Clone)]
pub enum AnalysisJob {
    /// Load the whole position history; `ttl_min` is `Planes_Position_TTL`.
    LoadHistory { ttl_min: i64 },
    /// Write the positions in `from..=to` as JSON or CSV to `path`.
    SavePositions {
        path: PathBuf,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        csv: bool,
    },
    /// Import position files (`.json`, `.csv`, `.hist`) into the database.
    LoadFiles(Vec<PathBuf>),
    /// Delete every stored position.
    ClearPositions,
    /// Download a day archive from a user URL and import it (`history_download`).
    Download(Box<DownloadRequest>),
    /// Write the table of the SINGLE path to `<data>/Tmp/Path Information <my> to <dx>.csv`.
    ExportPath {
        entry: Box<PathEntry>,
        model: ElevationModel,
        max_alt_m: f64,
        my_call: String,
        dx_call: String,
    },
    /// Hexes and calls with a position in the range, for the Plane History dialog.
    PlaneLists {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    /// Write the history of one plane to `<data>/Tmp/`.
    ExportPlane {
        key: HexOrCall,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    /// All crossings of the SINGLE path in the history (`CrossingHistoryDlg`). The job works on
    /// a copy of `data`: the app's history stays as it is.
    Crossings {
        data: Arc<HistoryData>,
        params: Box<CrossingParams>,
        settings: Box<Settings>,
    },
    /// The routes of all planes in the range (`TrafficDlg`).
    Traffic {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    /// Write the crossings of a run to `<data>/Tmp/Path Crossing History_...csv`.
    ExportCrossings {
        result: Arc<CrossingResult>,
        gap_s: f64,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        my_call: String,
        dx_call: String,
    },
}

/// Run `job` in the calling thread, reporting through `ctx`. Errors become `JobEvent::Failed`;
/// a cancelled job reports nothing more.
pub fn run(ctx: &JobCtx, data_dir: &Path, job: AnalysisJob) {
    let r = match job {
        AnalysisJob::LoadHistory { ttl_min } => load_history(ctx, data_dir, ttl_min),
        AnalysisJob::SavePositions {
            path,
            from,
            to,
            csv,
        } => save_positions(ctx, data_dir, &path, from, to, csv),
        AnalysisJob::LoadFiles(files) => load_files(ctx, data_dir, &files),
        AnalysisJob::ClearPositions => clear_positions(ctx, data_dir),
        AnalysisJob::Download(req) => {
            history_download::run(ctx, data_dir, &req, &Fs4Disk);
            Ok(())
        }
        AnalysisJob::ExportPath {
            entry,
            model,
            max_alt_m,
            my_call,
            dx_call,
        } => exports::export_path(ctx, data_dir, &entry, model, max_alt_m, &my_call, &dx_call),
        AnalysisJob::PlaneLists { from, to } => exports::plane_lists(ctx, data_dir, from, to),
        AnalysisJob::ExportPlane { key, from, to } => {
            exports::export_plane(ctx, data_dir, &key, from, to)
        }
        AnalysisJob::Traffic { from, to } => traffic::run_traffic(ctx, data_dir, from, to),
        AnalysisJob::Crossings {
            data,
            params,
            settings,
        } => crossing_history::run_crossings(ctx, data_dir, &data, &params, &settings),
        AnalysisJob::ExportCrossings {
            result,
            gap_s,
            from,
            to,
            my_call,
            dx_call,
        } => crossing_csv::export_crossings(
            ctx,
            data_dir,
            &result,
            gap_s,
            (from, to),
            (&my_call, &dx_call),
        ),
    };
    if let Err(e) = r {
        ctx.send(JobEvent::Failed(e));
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn load_history(ctx: &JobCtx, dir: &Path, ttl_min: i64) -> Result<(), String> {
    let positions = PositionDb::open(dir).map_err(err)?;
    let aircraft = AircraftDb::open(dir).map_err(err)?;
    ctx.progress("Getting timespan of all positions in database...");
    let oldest = positions.oldest().map_err(err)?;
    if ctx.cancelled() {
        return Ok(());
    }
    let youngest = positions
        .youngest()
        .map_err(err)?
        .map(|t| t + Duration::minutes(ttl_min));
    if ctx.cancelled() {
        return Ok(());
    }
    ctx.progress("Getting positions...");
    let count = positions.count().map_err(err)?;
    // the callback runs while the DB lock is held: it only sends events
    let aps = positions
        .get_all_with(&ctx.cancel, &mut |n| {
            ctx.progress(format!("Getting position {n}"))
        })
        .map_err(err)?;
    if ctx.cancelled() {
        return Ok(());
    }
    ctx.progress("Create additional info...");
    let mut infos: HashMap<String, Info> = HashMap::new();
    let mut out = Vec::with_capacity(aps.len());
    for (i, ap) in aps.into_iter().enumerate() {
        if i % 1000 == 0 && ctx.cancelled() {
            return Ok(());
        }
        if !infos.contains_key(&ap.hex) {
            let info = Info::find(&aircraft, &ap.hex, &ap.call).map_err(err)?;
            infos.insert(ap.hex.clone(), info);
        }
        let info = &infos[&ap.hex];
        out.push(PlaneInfo {
            time: ap.last_updated,
            call: info.call.clone(),
            reg: info.reg.clone(),
            hex: ap.hex,
            lat: ap.lat,
            lon: ap.lon,
            alt_ft: ap.alt_ft,
            track: ap.track,
            speed_kts: ap.speed_kts,
            type_code: info.type_code.clone(),
            manufacturer: info.manufacturer.clone(),
            model: info.model.clone(),
            category: info.category,
            ..PlaneInfo::default()
        });
    }
    out.sort_by_key(|p| p.time);
    ctx.send(JobEvent::HistoryLoaded(Box::new(HistoryData {
        oldest,
        youngest,
        count,
        positions: out,
    })));
    Ok(())
}

/// Aircraft data shared by every position of a hex.
struct Info {
    /// Call of the hex's first row; C# keeps it for all rows (MapDlg.cs:9408-9415).
    call: String,
    reg: String,
    type_code: String,
    manufacturer: String,
    model: String,
    category: PlaneCategory,
}

impl Info {
    fn find(db: &AircraftDb, hex: &str, call: &str) -> Result<Self, AircraftError> {
        const UNKNOWN: &str = "[unknown]";
        let ac = db.aircraft_find_by_hex(hex)?;
        let at = match &ac {
            Some(a) => db.aircraft_type_find_by_icao(&a.type_code)?,
            None => None,
        };
        Ok(Self {
            call: call.to_string(),
            reg: ac.as_ref().map_or(UNKNOWN.into(), |a| a.reg.clone()),
            type_code: ac.as_ref().map_or(UNKNOWN.into(), |a| a.type_code.clone()),
            manufacturer: at
                .as_ref()
                .map_or(UNKNOWN.into(), |t| t.manufacturer.clone()),
            model: at.as_ref().map_or(UNKNOWN.into(), |t| t.model.clone()),
            category: at.map_or(PlaneCategory::None, |t| t.category),
        })
    }
}

fn save_positions(
    ctx: &JobCtx,
    dir: &Path,
    path: &Path,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    csv: bool,
) -> Result<(), String> {
    let db = PositionDb::open(dir).map_err(err)?;
    ctx.progress("Getting positions...");
    let rows = db.get_all(from, to).map_err(err)?;
    if ctx.cancelled() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    let mut w = BufWriter::new(std::fs::File::create(path).map_err(err)?);
    if csv {
        position_files::write_csv(&mut w, &rows)
    } else {
        position_files::write_json(&mut w, &rows)
    }
    .and_then(|()| w.flush())
    .map_err(err)?;
    ctx.send(JobEvent::Done(format!(
        "{} positions saved to {}",
        rows.len(),
        path.display()
    )));
    Ok(())
}

const CHUNK: usize = 1000;

fn load_files(ctx: &JobCtx, dir: &Path, files: &[PathBuf]) -> Result<(), String> {
    let db = PositionDb::open(dir).map_err(err)?;
    let aircraft = AircraftDb::open(dir).map_err(err)?;
    let (mut imported, mut skipped) = (0usize, 0usize);
    for f in files {
        if ctx.cancelled() {
            return Ok(());
        }
        let name = f
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());
        let ext = f
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "json" | "csv" | "hist") {
            ctx.progress(format!("{name}: unknown file type, skipped"));
            continue;
        }
        ctx.progress(format!("Loading {name}..."));
        let parsed = std::fs::read_to_string(f)
            .map_err(err)
            .and_then(|text| match ext.as_str() {
                "json" => position_files::read_json(&text),
                "csv" => position_files::read_csv(&text),
                _ => position_files::read_hist(&text, &|call| {
                    aircraft
                        .aircraft_find_by_call(call)
                        .ok()
                        .flatten()
                        .map(|a| a.hex)
                }),
            });
        let ImportStats {
            imported: rows,
            skipped: bad,
        } = match parsed {
            Ok(s) => s,
            Err(e) => {
                ctx.progress(format!("{name}: {e}"));
                continue;
            }
        };
        skipped += bad;
        for chunk in rows.chunks(CHUNK) {
            if ctx.cancelled() {
                return Ok(());
            }
            imported += db.bulk_insert(chunk).map_err(err)?;
        }
    }
    ctx.send(JobEvent::Done(format!(
        "{imported} imported, {skipped} skipped"
    )));
    Ok(())
}

fn clear_positions(ctx: &JobCtx, dir: &Path) -> Result<(), String> {
    let n = PositionDb::open(dir)
        .map_err(err)?
        .delete_all()
        .map_err(err)?;
    ctx.send(JobEvent::Done(format!("{n} positions deleted")));
    Ok(())
}
