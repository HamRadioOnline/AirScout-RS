//! `CrossingHistoryDlg` (CrossingHistoryDlg.cs): all crossings of the SINGLE path found in the
//! loaded position history, the ambiguity pass and the CSV export. Pure functions plus the two
//! job bodies; the dialog is `ui/dialogs/crossing_history.rs`.
//!
//! Stage-6 fixes:
//! - the replay scan keeps an owned per-hex sliding window, pre-rolls one TTL, evaluates at
//!   `from` and then every step through `to` (inclusive), and never mutates source reports;
//! - midpoint-range preselection is removed so configured radius filtering is applied by crossing
//!   prediction at every step, including motion into range;
//! - the history path uses the same effective-position and elevation resolver as live paths for
//!   matching stations, settings and terrain availability.
//!
//! Not reproduced (GUI):
//! - `ref AllPositions` hands the dialog the main history; the job works on a copy.
//! - `startindex == -1` (no pre-selected report is younger than `time - ttl`) throws in C# and
//!   ends the run without a chart. The port ends the loop and keeps the crossings found so far;
//! - the progress flood per plane becomes one report per step;
//! - the header of the CSV joins the names without a separator (:349-369); the port writes `;`.

use crate::history::HistoryData;
use crate::jobs::{JobCtx, JobEvent};
use crate::services::{antenna_of_loc, ground_elevation, local_obstruction, open_data, Paths};
use as_aircraft::crossing::get_nearest_planes;
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use as_elevation::ElevationModel;
use as_geo::latlon::GPoint;
use as_geo::maidenhead;
use as_propagation::{PathParams, PropagationPath};
use as_settings::Settings;
use as_signals::SignalDb;
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Step width of the dialog [s] (`MapDlg.btn_Analysis_CrossingHistory_Click`, MapDlg.cs:6835).
pub const STEP_S: i64 = 60;

/// What the run needs from the settings (:137-217).
#[derive(Debug, Clone, PartialEq)]
pub struct CrossingParams {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// Step width [s], at least 1.
    pub step_s: i64,
    /// `Planes_Position_TTL` [min].
    pub ttl_min: i64,
    /// `Planes_Filter_Max_Circumcircle` [km] (0: half the path length, < 0: no limit).
    pub max_radius_km: f64,
    /// `Path_Band_Settings[Band].MaxDistance` [km].
    pub max_distance_km: f64,
    /// `Planes_MaxAlt` [m].
    pub max_alt_m: f64,
    /// `Planes_MaxDelta` [m].
    pub max_delta_m: f64,
    /// `Analysis_CrossingHistory_WithSignalLevel`.
    pub with_signal_level: bool,
}

impl CrossingParams {
    /// The values of `s` for the range `from..=to` (as returned by [`crossing_range`]).
    pub fn from_settings(
        s: &Settings,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        step_s: i64,
    ) -> Self {
        Self {
            from,
            to,
            step_s,
            ttl_min: s.planes_position_ttl_min,
            max_radius_km: s.gui.planes_filter_max_circumcircle_km,
            max_distance_km: s.current_band_settings().max_distance,
            max_alt_m: f64::from(s.planes_max_alt_m),
            max_delta_m: s.planes_max_delta_m,
            with_signal_level: s.analysis.crossing_with_signal_level,
        }
    }
}

/// The crossings of a run, the chart series and the path they were found on.
#[derive(Debug, Clone)]
pub struct CrossingResult {
    pub crossings: Vec<PlaneInfo>,
    /// Crossings with potential 100 per second (red).
    pub count_hi: Vec<(DateTime<Utc>, u32)>,
    /// All other crossings per second (blue).
    pub count_lo: Vec<(DateTime<Utc>, u32)>,
    /// The path of the run; the export needs its end points (`PPath`).
    pub path: PropagationPath,
}

/// The constructor of the dialog (:38-57): `from` is cut to 00:00 and moved up to `oldest`, `from`
/// after `to` becomes `to`, the step is at least 1 s. Returns `(from, to, step)`.
pub fn crossing_range(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    oldest: Option<DateTime<Utc>>,
    step_s: i64,
) -> (DateTime<Utc>, DateTime<Utc>, i64) {
    let mut from = Utc.from_utc_datetime(&from.date_naive().and_hms_opt(0, 0, 0).unwrap());
    if let Some(o) = oldest.filter(|o| from < *o) {
        from = o;
    }
    if from > to {
        from = to;
    }
    (from, to, step_s.max(1))
}

fn station(call: &str, lat: f64, lon: f64) -> Result<(GPoint, String), String> {
    if call.trim().is_empty() {
        return Err("no call sign".into());
    }
    let loc = maidenhead::loc_from_lat_lon(lat, lon, false, 3, true)
        .ok()
        .flatten()
        .ok_or_else(|| format!("invalid position {lat}, {lon}"))?;
    Ok((GPoint::new(lat, lon), loc))
}

/// The path of the dialog (:59-127), found in the propagation database or created and stored.
/// The near-field suppression of `paths.engine` is the caller's business. `Ok(None)` if `cancel`
/// stopped the build (`bw_History` is the worker of the path calculation in C#, :114-115).
pub fn crossing_path(
    paths: &Paths,
    s: &Settings,
    cancel: &AtomicBool,
) -> Result<Option<PropagationPath>, String> {
    let model = s.elevation_enabled.effective_model();
    if model == ElevationModel::None {
        return Err("no elevation model enabled".into());
    }
    let (my_raw, my_loc) = station(&s.my_call, s.my_lat, s.my_lon)?;
    let (dx_raw, dx_loc) = station(&s.dx_call, s.dx_lat, s.dx_lon)?;
    let (h_my, _) = antenna_of_loc(&paths.stations, &s.my_call, &my_loc, s.band, Some(my_raw))?;
    let (h_dx, _) = antenna_of_loc(&paths.stations, &s.dx_call, &dx_loc, s.band, Some(dx_raw))?;
    let my = crate::best_case::effective_position(&paths.store, s, my_raw)?;
    let dx = crate::best_case::effective_position(&paths.store, s, dx_raw)?;
    // the obstruction is stored at the settings position, not at the shifted one (#105)
    let obstruction = local_obstruction(&paths.store, model, my_raw, dx_raw, h_my)?;
    let bs = s.current_band_settings();
    let p = PathParams {
        lat1: my.lat,
        lon1: my.lon,
        h1: ground_elevation(&paths.store, &s.elevation_enabled, my.lat, my.lon) + h_my,
        lat2: dx.lat,
        lon2: dx.lon,
        h2: ground_elevation(&paths.store, &s.elevation_enabled, dx.lat, dx.lon) + h_dx,
        qrg_ghz: s.band.to_ghz(),
        radius_km: s.radius_km(),
        f1_clearance: bs.f1_clearance,
        step_width_m: model.default_step_width_m(),
        local_obstruction: obstruction,
    };
    paths
        .engine
        .find_or_create_path(model, &p, true, Some(cancel))
        .map_err(|e| format!("path calculation: {e}"))
}

/// The worker of the dialog (:137-217). `positions` is the dialog's copy of the history, sorted
/// by time. `progress` gets the number of crossings so far once per step. Cancel returns what was
/// found.
pub fn crossing_history(
    positions: &mut [PlaneInfo],
    path: &PropagationPath,
    p: &CrossingParams,
    signal: &dyn Fn(DateTime<Utc>) -> Option<f64>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize),
) -> Vec<PlaneInfo> {
    let mut crossings = Vec::new();
    let cancelled = || cancel.load(Ordering::Relaxed);
    let step = Duration::try_seconds(p.step_s.max(1)).unwrap_or(Duration::seconds(1));
    let ttl = Duration::try_minutes(p.ttl_min.max(0)).unwrap_or(Duration::zero());
    let pre_roll_from = p
        .from
        .checked_sub_signed(ttl)
        .unwrap_or(DateTime::<Utc>::MIN_UTC);
    let mut nearest = Vec::new();
    for (i, info) in positions.iter().enumerate() {
        if info.time >= pre_roll_from && info.time <= p.to {
            nearest.push(i);
        }
        if i % 4096 == 0 && cancelled() {
            return crossings;
        }
    }
    let mut next = 0usize;
    let mut cache: BTreeMap<String, PlaneInfo> = BTreeMap::new();
    let mut time = p.from;
    while !cancelled() && time <= p.to {
        progress(crossings.len());
        while !cancelled() && next < nearest.len() {
            let idx = nearest[next];
            let report = &positions[idx];
            if report.time > time {
                break;
            }
            let replace = cache
                .get(&report.hex)
                .is_none_or(|current| report.time >= current.time);
            if replace {
                cache.insert(report.hex.clone(), report.clone());
            }
            next += 1;
        }
        let from = time
            .checked_sub_signed(ttl)
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        cache.retain(|_, report| report.time >= from);
        let planes = PlaneInfoCache::new();
        for report in cache.values() {
            planes.set(report.clone());
        }
        let all = planes.get_all(time, p.ttl_min);
        let near = get_nearest_planes(
            time,
            path,
            &all,
            p.max_radius_km,
            p.max_distance_km,
            p.max_alt_m,
            p.max_delta_m,
        );
        for mut plane in near {
            if plane.int_qrb <= p.max_distance_km {
                let level = signal(plane.time);
                plane.signal_strength = level.unwrap_or(f64::MIN);
                if !p.with_signal_level || level.is_some() {
                    crossings.push(plane);
                }
            }
        }
        let Some(next) = time.checked_add_signed(step) else {
            break;
        };
        time = next;
    }
    crossings
}

/// The chart series (:233-299): crossings per second, potential 100 (hi) apart from the rest
/// (lo). Both series have every second that occurs in one of them, 0 where it is missing; the
/// lists are ordered by time.
#[allow(clippy::type_complexity)]
pub fn chart_buckets(
    crossings: &[PlaneInfo],
) -> (Vec<(DateTime<Utc>, u32)>, Vec<(DateTime<Utc>, u32)>) {
    let mut buckets: BTreeMap<i64, (u32, u32)> = BTreeMap::new();
    for c in crossings {
        let b = buckets.entry(c.time.timestamp()).or_default();
        if c.potential == 100 {
            b.0 += 1;
        } else {
            b.1 += 1;
        }
    }
    let at = |secs: i64| DateTime::from_timestamp(secs, 0).unwrap_or_default();
    let hi = buckets.iter().map(|(&t, b)| (at(t), b.0)).collect();
    let lo = buckets.iter().map(|(&t, b)| (at(t), b.1)).collect();
    (hi, lo)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Job `Crossings`: path, algorithm on a copy of the history, chart series. Sends `Crossings`
/// and `Done`; a cancelled job sends nothing more.
pub fn run_crossings(
    ctx: &JobCtx,
    dir: &Path,
    data: &HistoryData,
    params: &CrossingParams,
    settings: &Settings,
) -> Result<(), String> {
    ctx.progress("Calculating Path....");
    let paths = open_data(dir).map_err(err)?;
    // not part of the stored path's key, but of a newly computed one
    paths
        .engine
        .set_near_field_suppression_m(settings.path_near_field_suppression_m);
    let Some(path) = crossing_path(&paths, settings, &ctx.cancel)? else {
        return Ok(());
    };
    if ctx.cancelled() {
        return Ok(());
    }
    let signals = SignalDb::open(dir).map_err(err)?;
    ctx.progress("Pre-selecting nearest positions...");
    let mut positions = data.positions.clone();
    let signal = |t: DateTime<Utc>| signals.find(t).ok().flatten();
    let mut steps = 0u32;
    let crossings = crossing_history(
        &mut positions,
        &path,
        params,
        &signal,
        &ctx.cancel,
        &mut |n| {
            if steps.is_multiple_of(100) {
                ctx.progress(format!("Calculating, {n} crossings so far."));
            }
            steps += 1;
        },
    );
    if ctx.cancelled() {
        return Ok(());
    }
    let (count_hi, count_lo) = chart_buckets(&crossings);
    let n = crossings.len();
    ctx.send(JobEvent::Crossings(Box::new(CrossingResult {
        crossings,
        count_hi,
        count_lo,
        path,
    })));
    ctx.send(JobEvent::Done(format!("Calculation done, {n} crossings.")));
    Ok(())
}
