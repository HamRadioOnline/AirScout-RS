//! Port of `AirScout.Aircrafts/PlaneInfoCache.cs`: the latest report per aircraft, with
//! extrapolation to a given time.
//!
//! The original stores the caller's objects and mutates them in place from several threads
//! (logic-findings #22). This port stores and returns copies behind a mutex.

use crate::motion::{
    advance_with_tangent, extrapolated_track, heading_separation, ramp_distance_km, secs,
};
use crate::{AltitudeProvenance, PlaneInfo};
use as_geo::latlon;
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Linear extrapolation of alt, track and speed from the previous report to `at`, or `None` if
/// there is no previous report or a time difference is not positive.
fn extrapolate(p: &PlaneInfo, at: DateTime<Utc>) -> Option<(f64, f64, f64)> {
    let old = p.old_time?;
    let dt_old = secs(p.time - old);
    let dt_new = secs(at - p.time);
    if !(dt_old > 0.0 && dt_new > 0.0) {
        return None;
    }
    let linear = |now: f64, before: f64| (now - before) / dt_old * dt_new + now;
    Some((
        linear(p.alt_ft, p.old_alt_ft),
        extrapolated_track(p.track, p.old_track, dt_old, dt_new),
        linear(p.speed_kts, p.old_speed_kts),
    ))
}

fn advance(info: &mut PlaneInfo, at: DateTime<Utc>, track: f64, distance_km: f64) {
    let (lat, lon, track) = advance_with_tangent(info.lat, info.lon, track, distance_km);
    info.lat = lat;
    info.lon = lon;
    info.track = track;
    info.time = at;
}

/// Dead-reckon `info` to `at` with constant `speed_kmh` and `track`.
fn dead_reckon(info: &mut PlaneInfo, at: DateTime<Utc>, track: f64, speed_kmh: f64) {
    let distance = speed_kmh * secs(at - info.time) / 3600.0;
    advance(info, at, track, distance);
}

/// Dead-reckon `info` to `at` with a linear speed ramp from `start_kts` to `end_kts`.
fn dead_reckon_speed_ramp(
    info: &mut PlaneInfo,
    at: DateTime<Utc>,
    track: f64,
    start_kts: f64,
    end_kts: f64,
) {
    let distance = ramp_distance_km(start_kts, end_kts, secs(at - info.time));
    advance(info, at, track, distance);
}

/// Plane cache keyed by hex (sorted like the C# `SortedDictionary`).
#[derive(Debug, Default)]
pub struct PlaneInfoCache {
    inner: Mutex<CacheEntries>,
}

#[derive(Debug, Default)]
struct CacheEntries {
    planes: BTreeMap<String, PlaneInfo>,
    component_feed_geometric_only: bool,
}

fn is_component_pressure(plane: &PlaneInfo) -> bool {
    plane.components_valid_until.is_some()
        && plane.altitude_provenance == AltitudeProvenance::PressureEstimate
}

impl PlaneInfoCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, CacheEntries> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn insert_locked(entries: &mut CacheEntries, plane: &PlaneInfo) -> bool {
        if entries.component_feed_geometric_only && is_component_pressure(plane) {
            return false;
        }
        let Some(old) = entries.planes.get_mut(&plane.hex) else {
            entries.planes.insert(plane.hex.clone(), plane.clone());
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

    /// `BulkInsertOrUpdateIfNewer` returning the samples that actually won (were inserted or
    /// updated), in input order. A sample that loses — older than or equal to the stored report
    /// for its hex — is excluded. Callers feed candidates in timestamp order per hex so that a
    /// strictly newer sample supersedes an older one and the first of two equal-time samples wins.
    pub fn bulk_insert_winners(&self, planes: &[PlaneInfo]) -> Vec<PlaneInfo> {
        self.bulk_insert_validated(planes, |_, _| true, |_| {}).0
    }

    /// Atomic validated admission (#50): hold one lock across validation, admission and the
    /// `emit` hook so workers sharing this cache serialize each aircraft's admission and
    /// live-event enqueue in that order. Candidates must be in `(hex, time)` order. For each
    /// candidate, `validate` receives the current stored report for its hex — the accepted live
    /// state, including any earlier winner in this batch or from another worker — and a candidate
    /// it rejects is dropped. A candidate that passes and is strictly newer is admitted and
    /// becomes a winner. `emit` runs with the winners while the lock is still held, before any
    /// slower downstream write, so live events cannot be observed out of admission order.
    ///
    /// Returns `(winners, validated)`: `winners` are the strictly newer samples that were
    /// admitted (in input order), while `validated` additionally includes older/equal-time
    /// samples that passed `validate` but did not win — those stay eligible for history (#28)
    /// but must not reach live events or metadata. Samples rejected by `validate` appear in
    /// neither.
    pub fn bulk_insert_validated<F, G>(
        &self,
        planes: &[PlaneInfo],
        mut validate: F,
        emit: G,
    ) -> (Vec<PlaneInfo>, Vec<PlaneInfo>)
    where
        F: FnMut(&PlaneInfo, Option<&PlaneInfo>) -> bool,
        G: FnOnce(&[PlaneInfo]),
    {
        let mut map = self.lock();
        let mut winners = Vec::new();
        let mut validated = Vec::new();
        for plane in planes {
            let current = map.planes.get(&plane.hex).cloned();
            if !validate(plane, current.as_ref()) {
                continue;
            }
            validated.push(plane.clone());
            if Self::insert_locked(&mut map, plane) {
                winners.push(plane.clone());
            }
        }
        emit(&winners);
        (winners, validated)
    }

    /// Apply the live height policy under the same lock used for feed updates.
    pub fn set_component_feed_geometric_only(&self, geometric_only: bool) {
        let mut cache = self.lock();
        cache.component_feed_geometric_only = geometric_only;
        if geometric_only {
            cache
                .planes
                .retain(|_, plane| !is_component_pressure(plane));
        }
    }

    /// The stored report for `hex` as it is (C# `TryGetValue` / indexer), without
    /// extrapolation.
    pub fn latest(&self, hex: &str) -> Option<PlaneInfo> {
        self.lock().planes.get(hex).cloned()
    }

    /// The C# indexer `Planes[hex] = info` (`MapDlg.cs:5390`): store a copy, replacing any
    /// entry for the hex whatever its time. The `old_*` fields are taken as given.
    pub fn set(&self, plane: PlaneInfo) {
        self.lock().planes.insert(plane.hex.clone(), plane);
    }

    /// `Planes.Clear()` (`MapDlg.cs:5386`).
    pub fn clear(&self) {
        self.lock().planes.clear();
    }

    /// `Delete`; `true` if the plane was in the cache.
    pub fn delete(&self, hex: &str) -> bool {
        self.lock().planes.remove(hex).is_some()
    }

    /// Number of planes.
    pub fn len(&self) -> usize {
        self.lock().planes.len()
    }

    /// `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.lock().planes.is_empty()
    }

    /// `Get` (`PlaneInfoCache.cs:122-211`): the plane extrapolated to `at`, or `None` if unknown
    /// or its last report is not within `[at - ttl_min, at]`.
    ///
    /// With a previous report, alt/track/speed are extrapolated linearly; they are used if
    /// `0 < alt <= 50000`, `0 < track <= 360`, `0 < speed <= 700` and the resulting motion is
    /// within 45° of the reported track (no wraparound). Otherwise the position is dead-reckoned
    /// with the reported speed and track.
    pub fn get(&self, hex: &str, at: DateTime<Utc>, ttl_min: i64) -> Option<PlaneInfo> {
        let plane = self.lock().planes.get(hex)?.clone();
        Self::extrapolate_entry(&plane, at, ttl_min)
    }

    /// Extrapolate a stored `plane` to `at`, exactly as [`Self::get`] does, but without touching
    /// the lock. Used by the atomic validated admission (#50), which needs the extrapolated
    /// position of the current entry while the lock is already held.
    pub fn extrapolate_entry(
        plane: &PlaneInfo,
        at: DateTime<Utc>,
        ttl_min: i64,
    ) -> Option<PlaneInfo> {
        let from = ttl_start(at, ttl_min);
        if !is_eligible(plane, from, at) {
            return None;
        }
        let mut info = plane.copied();
        let (alt, track, speed) = extrapolate(plane, at).unwrap_or((0.0, 0.0, 0.0));
        if alt > 0.0
            && alt <= 50000.0
            && track > 0.0
            && track <= 360.0
            && speed > 0.0
            && speed <= 700.0
        {
            let dist = ramp_distance_km(info.speed_kts, speed, secs(at - info.time));
            let p = latlon::destination_point(info.lat, info.lon, track, dist);
            let calc = latlon::bearing(info.lat, info.lon, p.lat, p.lon);
            if heading_separation(info.track, calc) < 45.0 {
                let start_speed = info.speed_kts;
                dead_reckon_speed_ramp(&mut info, at, track, start_speed, speed);
                info.alt_ft = alt;
                info.speed_kts = speed;
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
        map.planes
            .values()
            .filter(|p| is_eligible(p, from, at))
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
                        dead_reckon_speed_ramp(&mut info, at, track, plane.speed_kts, speed);
                        return info;
                    }
                }
                let (track, speed) = (info.track, info.speed_kmh());
                dead_reckon(&mut info, at, track, speed);
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
    old.altitude_provenance = plane.altitude_provenance;
    old.components_valid_until = plane.components_valid_until;
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

fn is_eligible(plane: &PlaneInfo, from: DateTime<Utc>, at: DateTime<Utc>) -> bool {
    plane.time >= from
        && plane.time <= at
        && plane
            .components_valid_until
            .is_none_or(|expiry| at <= expiry)
}

/// `at - ttl_min` minutes; on overflow the earliest representable time (feed times are not
/// trusted, and C# would throw on the whole cycle).
fn ttl_start(at: DateTime<Utc>, ttl_min: i64) -> DateTime<Utc> {
    Duration::try_minutes(ttl_min)
        .and_then(|d| at.checked_sub_signed(d))
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}
