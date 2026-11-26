//! Port of `AirScout.Aircrafts/PlaneInfoCache.cs`: the latest report per aircraft, with
//! extrapolation to a given time.
//!
//! The original stores the caller's objects and mutates them in place from several threads
//! (logic-findings #22). This port stores and returns copies behind a mutex.

use crate::units::kts_kmh;
use crate::PlaneInfo;
use as_geo::latlon;
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Seconds between two times (C# `TimeSpan.TotalSeconds`).
fn secs(d: Duration) -> f64 {
    d.num_nanoseconds()
        .map(|n| n as f64 / 1e9)
        .unwrap_or(d.num_milliseconds() as f64 / 1e3)
}

/// Linear extrapolation of alt, track and speed from the previous report to `at`, or `None` if
/// there is no previous report or a time difference is not positive. Track has no wraparound
/// handling (logic-findings #23).
fn extrapolate(p: &PlaneInfo, at: DateTime<Utc>) -> Option<(f64, f64, f64)> {
    let old = p.old_time?;
    let dt_old = secs(p.time - old);
    let dt_new = secs(at - p.time);
    if !(dt_old > 0.0 && dt_new > 0.0) {
        return None;
    }
    let f = |now: f64, before: f64| (now - before) / dt_old * dt_new + now;
    Some((
        f(p.alt_ft, p.old_alt_ft),
        f(p.track, p.old_track),
        f(p.speed_kts, p.old_speed_kts),
    ))
}

/// Dead-reckon `info` to `at` with `speed_kmh` and `track`.
fn dead_reckon(info: &mut PlaneInfo, at: DateTime<Utc>, track: f64, speed_kmh: f64) {
    let dist = speed_kmh * secs(at - info.time) / 3600.0;
    let p = latlon::destination_point(info.lat, info.lon, track, dist);
    info.lat = p.lat;
    info.lon = p.lon;
    info.time = at;
}

/// Plane cache keyed by hex (sorted like the C# `SortedDictionary`).
#[derive(Debug, Default)]
pub struct PlaneInfoCache {
    planes: Mutex<BTreeMap<String, PlaneInfo>>,
}

impl PlaneInfoCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, PlaneInfo>> {
        self.planes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn insert_locked(map: &mut BTreeMap<String, PlaneInfo>, plane: &PlaneInfo) -> bool {
        let Some(old) = map.get_mut(&plane.hex) else {
            map.insert(plane.hex.clone(), plane.clone());
            return true;
        };
        update_if_newer(old, plane)
    }

    /// `InsertOrUpdateIfNewer`: add the plane, or update it if the report is newer. The current
    /// position, alt, speed and track move into the `old_*` fields. `true` if something changed.
    pub fn insert_or_update_if_newer(&self, plane: &PlaneInfo) -> bool {
        Self::insert_locked(&mut self.lock(), plane)
    }

    /// `BulkInsertOrUpdateIfNewer`; number of planes added or updated.
    pub fn bulk_insert_or_update_if_newer(&self, planes: &[PlaneInfo]) -> usize {
        let mut map = self.lock();
        planes
            .iter()
            .filter(|p| Self::insert_locked(&mut map, p))
            .count()
    }

    /// The stored report for `hex` as it is (C# `TryGetValue` / indexer), without
    /// extrapolation.
    pub fn latest(&self, hex: &str) -> Option<PlaneInfo> {
        self.lock().get(hex).cloned()
    }

    /// The C# indexer `Planes[hex] = info` (`MapDlg.cs:5390`): store a copy, replacing any
    /// entry for the hex whatever its time. The `old_*` fields are taken as given.
    pub fn set(&self, plane: PlaneInfo) {
        self.lock().insert(plane.hex.clone(), plane);
    }

    /// `Planes.Clear()` (`MapDlg.cs:5386`).
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// `Delete`; `true` if the plane was in the cache.
    pub fn delete(&self, hex: &str) -> bool {
        self.lock().remove(hex).is_some()
    }

    /// Number of planes.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// `Get` (`PlaneInfoCache.cs:122-211`): the plane extrapolated to `at`, or `None` if unknown
    /// or its last report is not within `[at - ttl_min, at]`.
    ///
    /// With a previous report, alt/track/speed are extrapolated linearly; they are used if
    /// `0 < alt <= 50000`, `0 < track <= 360`, `0 < speed <= 700` and the resulting motion is
    /// within 45° of the reported track (no wraparound). Otherwise the position is dead-reckoned
    /// with the reported speed and track.
    pub fn get(&self, hex: &str, at: DateTime<Utc>, ttl_min: i64) -> Option<PlaneInfo> {
        let plane = self.lock().get(hex)?.clone();
        let from = ttl_start(at, ttl_min);
        if plane.time < from || plane.time > at {
            return None;
        }
        let mut info = plane.copied();
        let (alt, track, speed) = extrapolate(&plane, at).unwrap_or((0.0, 0.0, 0.0));
        if alt > 0.0
            && alt <= 50000.0
            && track > 0.0
            && track <= 360.0
            && speed > 0.0
            && speed <= 700.0
        {
            let dist = kts_kmh(speed) * secs(at - info.time) / 3600.0;
            let p = latlon::destination_point(info.lat, info.lon, track, dist);
            let calc = latlon::bearing(info.lat, info.lon, p.lat, p.lon);
            if (info.track - calc).abs() < 45.0 {
                info.lat = p.lat;
                info.lon = p.lon;
                info.alt_ft = alt;
                info.track = track;
                info.speed_kts = speed;
                info.time = at;
                return Some(info);
            }
        }
        let (t, v) = (info.track, info.speed_kmh());
        dead_reckon(&mut info, at, t, v);
        Some(info)
    }

    /// `GetAll` (`PlaneInfoCache.cs:213-330`): every plane whose last report is within
    /// `[at - ttl_min, at]`, extrapolated to `at`. Extrapolated alt/track/speed are used if
    /// `0 < alt < 50000`, `0 < track < 360`, `0 < speed < 700` (strict, unlike `get`); the
    /// position is always dead-reckoned with the resulting speed and track.
    pub fn get_all(&self, at: DateTime<Utc>, ttl_min: i64) -> Vec<PlaneInfo> {
        let from = ttl_start(at, ttl_min);
        let map = self.lock();
        map.values()
            .filter(|p| p.time >= from && p.time <= at)
            .map(|plane| {
                let mut info = plane.copied();
                if let Some((alt, track, speed)) = extrapolate(plane, at) {
                    if alt > 0.0
                        && alt < 50000.0
                        && track > 0.0
                        && track < 360.0
                        && speed > 0.0
                        && speed < 700.0
                    {
                        info.alt_ft = alt;
                        info.track = track;
                        info.speed_kts = speed;
                    }
                }
                let (t, v) = (info.track, info.speed_kmh());
                dead_reckon(&mut info, at, t, v);
                info
            })
            .collect()
    }
}

/// The update branch of `InsertOrUpdateIfNewer` (`PlaneInfoCache.cs:33-72`) on the stored
/// object `old`: if `plane` is newer, the current position, alt, speed and track move into the
/// `old_*` fields and every field except Hex, From, To, VSpeed and Ambiguous is taken from
/// `plane`. `true` if `old` changed.
pub fn update_if_newer(old: &mut PlaneInfo, plane: &PlaneInfo) -> bool {
    if plane.time <= old.time {
        return false;
    }
    old.old_time = Some(old.time);
    old.old_lat = old.lat;
    old.old_lon = old.lon;
    old.old_alt_ft = old.alt_ft;
    old.old_speed_kts = old.speed_kts;
    old.old_track = old.track;
    // every field except Hex, From, To, VSpeed and Ambiguous (as in the original)
    old.alt_ft = plane.alt_ft;
    old.alt_diff = plane.alt_diff;
    old.angle = plane.angle;
    old.call = plane.call.clone();
    old.category = plane.category;
    old.comment = plane.comment.clone();
    old.eps1 = plane.eps1;
    old.eps2 = plane.eps2;
    old.theta1 = plane.theta1;
    old.theta2 = plane.theta2;
    old.int_point = plane.int_point;
    old.int_qrb = plane.int_qrb;
    old.lat = plane.lat;
    old.lon = plane.lon;
    old.manufacturer = plane.manufacturer.clone();
    old.model = plane.model.clone();
    old.potential = plane.potential;
    old.reg = plane.reg.clone();
    old.signal_strength = plane.signal_strength;
    old.speed_kts = plane.speed_kts;
    old.squint = plane.squint;
    old.time = plane.time;
    old.track = plane.track;
    old.type_code = plane.type_code.clone();
    true
}

/// `at - ttl_min` minutes; on overflow the earliest representable time (feed times are not
/// trusted, and C# would throw on the whole cycle).
fn ttl_start(at: DateTime<Utc>, ttl_min: i64) -> DateTime<Utc> {
    Duration::try_minutes(ttl_min)
        .and_then(|d| at.checked_sub_signed(d))
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}
