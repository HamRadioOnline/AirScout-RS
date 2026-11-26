//! `CrossingHistoryDlg` (CrossingHistoryDlg.cs): all crossings of the SINGLE path found in the
//! loaded position history, the ambiguity pass and the CSV export. Pure functions plus the two
//! job bodies; the dialog is `ui/dialogs/crossing_history.rs`.
//!
//! Ported 1:1, also where it is wrong (prediction-affecting):
//! - the plane cache of one step holds *references* into the pre-selected positions, and
//!   `InsertOrUpdateIfNewer` overwrites the first object of a hex with newer reports (:162-181).
//!   [`crossing_history`] keeps the cache as indices and writes into `positions`, so the later
//!   steps see the overwritten objects. This happens in the dialog's own copy only;
//! - the crossings carry the time and the position of the *step* (`GetAll` extrapolates every
//!   plane to it), duplicates per step stay;
//! - the signal level is looked up for every crossing, the option only decides whether planes
//!   without one are dropped (:198-212);
//! - the ambiguity pass of `crossing_csv::mark_ambiguous` with its quirks (:311-336).
//!
//! The path set-up ([`crossing_path`], :59-127) differs from `UpdatePaths` and the port keeps the
//! differences:
//! - the stations come from `LocationFindOrCreate(call, 6-char locator)` of the *station
//!   database*: the stored position of that locator, else the centre of its square (stored). The
//!   settings position is used for the locator only (`UpdatePaths` updates the stored location
//!   to the settings position);
//! - best case: only the selected model is asked and the position moves whenever its tile is
//!   stored, whatever the maximum is ([`calculator_shift`]); `UpdatePaths` walks all enabled
//!   models and needs a real maximum;
//! - the ground elevation is that of the selected model alone, 0 if missing (`ElevationData
//!   [lat, lon, model]`); `UpdatePaths` walks the models (`GetElevation`);
//! - the selected model is the best enabled one (`effective_model`), as everywhere in the port.
//!
//! Not reproduced (GUI):
//! - `ref AllPositions` hands the dialog the main history, and the aliasing above corrupts it.
//!   The job works on a copy;
//! - `startindex == -1` (no pre-selected report is younger than `time - ttl`) throws in C# and
//!   ends the run without a chart. The port ends the loop and keeps the crossings found so far;
//! - the progress flood per plane becomes one report per step;
//! - the header of the CSV joins the names without a separator (:349-369); the port writes `;`.

use crate::history::HistoryData;
use crate::jobs::{JobCtx, JobEvent};
use crate::services::{antenna_of_loc, local_obstruction, open_data, Paths};
use as_aircraft::crossing::get_nearest_planes;
use as_aircraft::{update_if_newer, PlaneInfo, PlaneInfoCache};
use as_elevation::ElevationModel;
use as_geo::latlon::{self, GPoint};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_propagation::{PathParams, PropagationPath};
use as_settings::Settings;
use as_signals::SignalDb;
use as_stations::{GeoSource, Location};
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

/// One station of the path: its stored location of the 6-character locator (created at the centre
/// of the square if there is none), the best-case position and the antenna height [m].
fn station(
    paths: &Paths,
    s: &Settings,
    model: ElevationModel,
    call: &str,
    lat: f64,
    lon: f64,
) -> Result<(GPoint, f64), String> {
    if call.trim().is_empty() {
        return Err("no call sign".into());
    }
    let loc = maidenhead::loc_from_lat_lon(lat, lon, false, 3, true)
        .ok()
        .flatten()
        .ok_or_else(|| format!("invalid position {lat}, {lon}"))?;
    let db_err = |e: as_stations::StationError| format!("station database: {e}");
    let found = paths
        .stations
        .location_find(call, Some(&loc))
        .map_err(db_err)?;
    let ld = match found {
        Some(l) => l,
        None => {
            let c = maidenhead::gpoint_from_loc(&loc, PositionInRectangle::MiddleMiddle)
                .map_err(|e| format!("locator {loc}: {e}"))?;
            let l = Location::new(call, &loc, c.lat, c.lon, GeoSource::FromLoc, 0, Utc::now());
            paths
                .stations
                .location_insert_or_update_if_newer(&l)
                .map_err(db_err)?;
            l
        }
    };
    let (height, _) = antenna_of_loc(&paths.stations, &ld.call, &ld.loc, s.band)?;
    let mut pos = GPoint::new(ld.lat, ld.lon);
    if s.path_best_case_elevation {
        let shift = crate::best_case::calculator_shift(model, pos, |m, loc| {
            paths.store.max_elv_loc(m, loc).ok().flatten()
        })?;
        if let Some(info) = shift {
            pos = GPoint::new(info.max_lat, info.max_lon);
        }
    }
    Ok((pos, height))
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
    let (my, h_my) = station(paths, s, model, &s.my_call, s.my_lat, s.my_lon)?;
    let (dx, h_dx) = station(paths, s, model, &s.dx_call, s.dx_lat, s.dx_lon)?;
    let ground = |p: GPoint| {
        paths
            .store
            .elevation(model, p.lat, p.lon, true)
            .map(f64::from)
            .map_err(|e| format!("elevation: {e}"))
    };
    let obstruction = local_obstruction(&paths.store, model, my, dx, h_my)?;
    let bs = s.current_band_settings();
    let p = PathParams {
        lat1: my.lat,
        lon1: my.lon,
        h1: ground(my)? + h_my,
        lat2: dx.lat,
        lon2: dx.lon,
        h2: ground(dx)? + h_dx,
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

/// Borrow `positions[a]` mutably and `positions[b]` shared (`a != b`).
fn pair(positions: &mut [PlaneInfo], a: usize, b: usize) -> (&mut PlaneInfo, &PlaneInfo) {
    if a < b {
        let (lo, hi) = positions.split_at_mut(b);
        (&mut lo[a], &hi[0])
    } else {
        let (lo, hi) = positions.split_at_mut(a);
        (&mut hi[0], &lo[b])
    }
}

/// `PlaneInfoCache.InsertOrUpdateIfNewer` on references: a hex seen before keeps its first
/// object, which a newer report overwrites in place.
fn cache_insert(cache: &mut BTreeMap<String, usize>, positions: &mut [PlaneInfo], idx: usize) {
    match cache.get(&positions[idx].hex).copied() {
        None => {
            cache.insert(positions[idx].hex.clone(), idx);
        }
        Some(first) if first != idx => {
            let (old, new) = pair(positions, first, idx);
            update_if_newer(old, new);
        }
        Some(_) => {}
    }
}

/// The worker of the dialog (:137-217). `positions` is the dialog's copy of the history, sorted
/// by time; the aliasing of the cache changes it (see the module doc). `progress` gets the number
/// of crossings so far once per step. Cancel returns what was found.
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
    // pre-select the positions near the path (:111-130)
    let mid = path.mid_point();
    let max_dist = path.distance() / 2.0;
    let mut nearest = Vec::new();
    for (i, info) in positions.iter().enumerate() {
        if info.time >= p.from
            && info.time <= p.to
            && latlon::distance(info.lat, info.lon, mid.lat, mid.lon) <= max_dist
        {
            nearest.push(i);
        }
        if i % 4096 == 0 && cancelled() {
            return crossings;
        }
    }
    if nearest.is_empty() {
        return crossings;
    }
    let step = Duration::try_seconds(p.step_s.max(1)).unwrap_or(Duration::seconds(1));
    let ttl = Duration::try_minutes(p.ttl_min);
    let mut start = Some(0usize);
    let mut time = positions[nearest[0]].time;
    while !cancelled() && time <= p.to {
        progress(crossings.len());
        // C# starts at index -1 and throws (:170-172): the loop ends
        let Some(first) = start else { break };
        let from = ttl
            .and_then(|d| time.checked_sub_signed(d))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let mut cache: BTreeMap<String, usize> = BTreeMap::new();
        start = None;
        let mut i = first;
        while !cancelled() && i < nearest.len() {
            let idx = nearest[i];
            if positions[idx].time >= from {
                // the first index of the cache fill is where the next step starts
                if start.is_none() {
                    start = Some(i);
                }
                cache_insert(&mut cache, positions, idx);
            }
            if positions[idx].time > time {
                break;
            }
            i += 1;
        }
        let planes = PlaneInfoCache::new();
        for &idx in cache.values() {
            planes.set(positions[idx].clone());
        }
        let mut all = planes.get_all(time, p.ttl_min);
        let near = get_nearest_planes(
            time,
            path,
            &mut all,
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
