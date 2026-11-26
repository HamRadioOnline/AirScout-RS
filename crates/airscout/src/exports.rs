//! CSV exports of the analysis side: the path information of the SINGLE path
//! (`btn_Analysis_Path_SaveToFile_Click`, MapDlg.cs:6694-6825) and the history of one plane
//! (`PlaneHistoryDlg`, PlaneHistoryDlg.cs:66-165).
//!
//! Numbers are written culture-independent (the C# uses the current UI culture). The CSV
//! separator is the constant `;`.

use crate::jobs::{JobCtx, JobEvent};
use crate::ui::dialogs::horizon::CSV_SEPARATOR;
use as_aircraft::check::Lookup;
use as_aircraft::convert;
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_aircraft::units::{ft_m, kts_kmh};
use as_elevation::{ElevationModel, ElevationPath, ElevationStore};
use as_geo::{latlon, propagation::epsilon_from_heights};
use as_propagation::PropagationPath;
use chrono::{DateTime, Utc};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// What the plane history is looked up by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexOrCall {
    Hex(String),
    Call(String),
}

impl HexOrCall {
    pub fn text(&self) -> &str {
        match self {
            Self::Hex(s) | Self::Call(s) => s,
        }
    }
}

/// How the C# writes `Infinity`, `-Infinity` and `NaN`; `finite` formats everything else.
fn special(v: f64, finite: impl FnOnce(f64) -> String) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else {
        finite(v)
    }
}

/// `double.ToString()`: the shortest text that reads back as the same value.
fn shortest(v: f64) -> String {
    special(v, |v| v.to_string())
}

/// `ToString("F8")`.
pub(crate) fn f8(v: f64) -> String {
    special(v, |v| format!("{v:.8}"))
}

/// `ToString("F2")`.
pub(crate) fn f2(v: f64) -> String {
    special(v, |v| format!("{v:.2}"))
}

/// A file name part without path separators and without characters Windows forbids in file
/// names (`MyCall.Replace("/", "_")` and the like).
pub(crate) fn flat(s: &str) -> String {
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

/// `Path Information <my> to <dx>.csv`.
pub fn path_file_name(my_call: &str, dx_call: &str) -> String {
    format!(
        "Path Information {} to {}.csv",
        flat(my_call),
        flat(dx_call)
    )
}

/// `Plane History_<key>_<from>_to_<to>.csv`.
pub fn plane_file_name(key: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    let f = "%Y%m%d_%H%M%S";
    format!(
        "Plane History_{}_{}_to_{}.csv",
        flat(key),
        from.format(f),
        to.format(f)
    )
}

const PATH_COLUMNS: [&str; 14] = [
    "Distance[km]",
    "Lat[deg]",
    "Lon[deg]",
    "Elevation[m]",
    "Min_h1[m]",
    "Min_h2[m]",
    "Min_h[m]",
    "Max_h[m]",
    "F1[m]",
    "eps1[deg]",
    "eps2[deg]",
    "eps1_min[deg]",
    "eps2_min[deg]",
    "ElevationModel",
];

/// Write the path table: one row per elevation point. `Ok(false)` if `cancel` was set (the C#
/// loop ignores it, finding). The C# header has 14 columns but its rows only 13 (the model name
/// is missing): the rows here have all 14.
pub fn write_path_csv(
    w: &mut dyn Write,
    path: &PropagationPath,
    elv: &ElevationPath,
    model: ElevationModel,
    max_alt_m: f64,
    cancel: &AtomicBool,
) -> io::Result<bool> {
    let sep = CSV_SEPARATOR;
    writeln!(w, "{}", PATH_COLUMNS.join(sep))?;
    let p = &path.params;
    let total = path.distance();
    let deg = |rad: f64| rad / std::f64::consts::PI * 180.0;
    for i in 0..elv.count {
        if i.is_multiple_of(256) && cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let dist = i as f64 * elv.step_width_m / 1000.0;
        let pt = path
            .info_point(dist)
            .ok_or_else(|| io::Error::other("the path has no step width"))?;
        let e = f64::from(elv.path[i]);
        let cols = [
            dist,
            pt.lat,
            pt.lon,
            e,
            pt.h1,
            pt.h2,
            pt.h1.max(pt.h2),
            max_alt_m,
            pt.f1,
            deg(epsilon_from_heights(p.h1, dist, e, p.radius_km)),
            deg(epsilon_from_heights(p.h2, total - dist, e, p.radius_km)),
            deg(path.eps1_min),
            deg(path.eps2_min),
        ];
        for c in cols {
            write!(w, "{}{sep}", shortest(c))?;
        }
        writeln!(w, "{}", model.dir_name())?;
    }
    Ok(true)
}

/// Write the history of one plane. `dist` and `av_speed` are 0 in the first row; a zero time
/// difference writes `Infinity` or `NaN`, as the C# does (finding). `Ok(false)` if cancelled.
pub fn write_plane_csv(
    w: &mut dyn Write,
    rows: &[AircraftPosition],
    cancel: &AtomicBool,
) -> io::Result<bool> {
    writeln!(
        w,
        "time;hex;call;lat[deg];lon[deg];alt[m];track[deg];speed[kmh];dist[km];av_speed[km/h]"
    )?;
    let clean = |s: &str| s.replace([';', '\r', '\n'], " ");
    for (i, r) in rows.iter().enumerate() {
        if i.is_multiple_of(1024) && cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let (mut dist, mut av_speed) = (0.0, 0.0);
        if i > 0 {
            let prev = &rows[i - 1];
            dist = latlon::distance(prev.lat, prev.lon, r.lat, r.lon);
            let hours = (r.last_updated - prev.last_updated).num_seconds() as f64 / 3600.0;
            av_speed = dist / hours;
        }
        writeln!(
            w,
            "{};{};{};{};{};{};{};{};{};{}",
            r.last_updated.format("%Y-%m-%d %H:%M:%S"),
            clean(&r.hex),
            clean(&r.call),
            f8(r.lat),
            f8(r.lon),
            f8(ft_m(r.alt_ft)),
            f8(r.track),
            f8(kts_kmh(r.speed_kts)),
            f8(dist),
            f8(av_speed)
        )?;
    }
    Ok(true)
}

/// The two combo lists of the dialog (`bw_GetHexAndCalls_ProgressChanged`): hexes by `To_Hex`,
/// calls by `To_Call` with the airline check; sorted, without duplicates. `None` if cancelled.
pub fn filter_plane_lists(
    hexes: Vec<String>,
    calls: Vec<String>,
    lookup: &dyn Lookup,
    cancel: &AtomicBool,
) -> Option<(Vec<String>, Vec<String>)> {
    let cancelled = |i: usize| i.is_multiple_of(1000) && cancel.load(Ordering::Relaxed);
    let mut h = Vec::new();
    for (i, s) in hexes.iter().enumerate() {
        if cancelled(i) {
            return None;
        }
        h.extend(convert::to_hex(s));
    }
    let mut c = Vec::new();
    for (i, s) in calls.iter().enumerate() {
        if cancelled(i) {
            return None;
        }
        c.extend(convert::to_call(s, true, lookup));
    }
    for v in [&mut h, &mut c] {
        v.sort();
        v.dedup();
    }
    Some((h, c))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Create `<data dir>/Tmp/<name>` and fill it with `body`; `Ok(None)` if `body` was cancelled
/// (the partial file is removed).
pub(crate) fn write_tmp(
    dir: &Path,
    name: &str,
    body: impl FnOnce(&mut dyn Write) -> io::Result<bool>,
) -> Result<Option<PathBuf>, String> {
    let tmp = dir.join("Tmp");
    std::fs::create_dir_all(&tmp).map_err(err)?;
    let path = tmp.join(name);
    let mut w = BufWriter::new(std::fs::File::create(&path).map_err(err)?);
    let done = body(&mut w)
        .and_then(|d| w.flush().map(|()| d))
        .map_err(err)?;
    drop(w);
    if !done {
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }
    Ok(Some(path))
}

/// Job `ExportPath`: the elevation path of the SINGLE path is found or created in the store,
/// then the table is written to `<data>/Tmp/Path Information <my> to <dx>.csv`.
pub fn export_path(
    ctx: &JobCtx,
    dir: &Path,
    entry: &crate::app::PathEntry,
    model: ElevationModel,
    max_alt_m: f64,
    my_call: &str,
    dx_call: &str,
) -> Result<(), String> {
    ctx.progress("Getting elevation path...");
    let store = ElevationStore::open(dir).map_err(err)?;
    let p = &entry.path.params;
    let elv = store
        .find_or_create_path(
            model,
            p.lat1,
            p.lon1,
            p.lat2,
            p.lon2,
            model.default_step_width_m(),
            true,
            Some(&ctx.cancel),
        )
        .map_err(err)?;
    let Some(elv) = elv else {
        // cancelled
        return Ok(());
    };
    ctx.progress("Writing path information...");
    let name = path_file_name(my_call, dx_call);
    let written = write_tmp(dir, &name, |w| {
        write_path_csv(w, &entry.path, &elv, model, max_alt_m, &ctx.cancel)
    })?;
    if let Some(path) = written {
        ctx.send(JobEvent::Done(format!(
            "{} points written to {}",
            elv.count,
            path.display()
        )));
    }
    Ok(())
}

/// Job `PlaneLists`: the hexes and calls of the range for the dialog's combos.
pub fn plane_lists(
    ctx: &JobCtx,
    dir: &Path,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(), String> {
    ctx.progress("Collecting data, please wait...");
    let db = PositionDb::open(dir).map_err(err)?;
    let hexes = db.get_all_hex(from, to).map_err(err)?;
    let calls = db.get_all_calls(from, to).map_err(err)?;
    let aircraft = AircraftDb::open(dir).map_err(err)?;
    let Some((hexes, calls)) = filter_plane_lists(hexes, calls, &aircraft, &ctx.cancel) else {
        return Ok(());
    };
    ctx.send(JobEvent::PlaneLists { hexes, calls });
    ctx.send(JobEvent::Done("Ready.".into()));
    Ok(())
}

/// Job `ExportPlane`: `<data>/Tmp/Plane History_<key>_<from>_to_<to>.csv`. Exporting by call
/// mixes the hexes that used the call, as the C# does.
pub fn export_plane(
    ctx: &JobCtx,
    dir: &Path,
    key: &HexOrCall,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(), String> {
    ctx.progress("Getting positions...");
    let db = PositionDb::open(dir).map_err(err)?;
    let rows = match key {
        HexOrCall::Hex(h) => db.get_by_hex_in(h, from, to),
        HexOrCall::Call(c) => db.get_by_call_in(c, from, to),
    }
    .map_err(err)?;
    if ctx.cancelled() {
        return Ok(());
    }
    let name = plane_file_name(key.text(), from, to);
    let written = write_tmp(dir, &name, |w| write_plane_csv(w, &rows, &ctx.cancel))?;
    if let Some(path) = written {
        ctx.send(JobEvent::Done(format!(
            "{} positions written to {}",
            rows.len(),
            path.display()
        )));
    }
    Ok(())
}
