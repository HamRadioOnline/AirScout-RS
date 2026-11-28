//! The per-plane checks and recovery of `AirScout.PlaneFeeds/PlaneFeed.cs` (worker loop body,
//! L138-466 of the original file), in the original order:
//! ground skip, hex (recover from reg), lat + area, lon + area, alt (recover from cache) +
//! window, call (cache, database, `[unknown]`), reg (same), track, speed (recover from cache),
//! type (database), manufacturer/model/category (database), model prefix strip, extended
//! plausibility check. (Upstream line refs: `PlaneFeed.cs:336`, `:375`, `:391`.)

use as_aircraft::check;
use as_aircraft::db::AircraftDb;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_geo::latlon;
use chrono::{DateTime, Utc};

use crate::{FeedArgs, FeedPlane};

/// Why a plane was rejected (counted as an error in C#). Silent skips (on ground, outside the
/// area or the altitude window) are not listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// No timestamp. C# would continue with `DateTime.MinValue`.
    Time,
    /// Invalid hex and no usable registration.
    Hex,
    /// Invalid hex, registration not in the database.
    Reg,
    Lat,
    Lon,
    /// Invalid altitude and nothing in the cache.
    Alt,
    Track,
    /// Invalid speed and nothing in the cache.
    Speed,
    /// Reported track differs from the movement by more than 45° (folded, #47).
    PlausibilityTrack,
    /// Position too far from the extrapolated one.
    PlausibilityDist,
}

/// `Planes_ExtendedPlausibilityCheck_*` and `LogErrors`.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineConfig {
    pub extended_plausibility: bool,
    /// [km per minute since the last report].
    pub max_error_dist: f64,
    pub log_errors: bool,
}

/// Result of one cycle.
#[derive(Debug, Clone, Default)]
pub struct Processed {
    pub planes: Vec<PlaneInfo>,
    pub rejected: Vec<(FeedPlane, Reject)>,
}

const UNKNOWN: &str = "[unknown]";

/// Shortest angular separation in degrees modulo 360° (#47): opposite bearings (90° versus
/// 270°) are 180° apart, while a turn across north (350° versus 10°) is 20° apart.
fn heading_separation(a: f64, b: f64) -> f64 {
    ((b - a + 540.0).rem_euclid(360.0) - 180.0).abs()
}

/// Full fractional seconds between two instants (the `TimeSpan.TotalSeconds` of C#), used for
/// forward-only comparisons (#50).
pub(crate) fn elapsed_secs(from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
    let d = to - from;
    d.num_nanoseconds()
        .map(|n| n as f64 / 1e9)
        .unwrap_or(d.num_milliseconds() as f64 / 1e3)
}

/// Check one plane. `Ok(Some)` keeps it, `Ok(None)` skips it silently, `Err` rejects it.
fn check_one(
    ac: &FeedPlane,
    args: &FeedArgs,
    cfg: &PipelineConfig,
    cache: &PlaneInfoCache,
    db: &AircraftDb,
) -> Result<Option<PlaneInfo>, Reject> {
    if ac.ground {
        return Ok(None);
    }
    let time = ac.time.ok_or(Reject::Time)?;
    let mut p = PlaneInfo {
        hex: ac.hex.clone(),
        lat: ac.lat,
        lon: ac.lon,
        alt_ft: ac.alt_ft,
        altitude_provenance: ac.altitude_provenance,
        call: ac.call.clone(),
        reg: ac.reg.clone(),
        track: ac.track,
        speed_kts: ac.speed_kts,
        time,
        components_valid_until: ac.components_valid_until,
        from: ac.from.clone(),
        to: ac.to.clone(),
        vspeed: ac.vspeed,
        // C# casts any int into the enum; values outside it become None here
        category: PlaneCategory::from_i32(ac.category).unwrap_or_default(),
        type_code: ac.type_code.clone(),
        model: ac.model.clone(),
        manufacturer: ac.manufacturer.clone(),
        ..PlaneInfo::default()
    };
    // C# database finds return null on any error
    let by_hex = |hex: &str| db.aircraft_find_by_hex(hex).ok().flatten();

    if !check::hex(&p.hex) {
        if !check::reg(&p.reg, db) {
            return Err(Reject::Hex);
        }
        let ad = db
            .aircraft_find_by_reg(&p.reg)
            .ok()
            .flatten()
            .ok_or(Reject::Reg)?;
        p.hex = ad.hex;
    }
    if !check::lat(p.lat) {
        return Err(Reject::Lat);
    }
    if p.lat < args.min_lat || p.lat > args.max_lat {
        return Ok(None);
    }
    if !check::lon(p.lon) {
        return Err(Reject::Lon);
    }
    if p.lon < args.min_lon || p.lon > args.max_lon {
        return Ok(None);
    }
    let cached = cache.latest(&p.hex);
    // A component-timed report (#366) carries the deadline of its own observations; a
    // cached height or speed has an older one, so it must not be borrowed under it.
    let component_timed = p.components_valid_until.is_some();
    if !check::alt(p.alt_ft) {
        if component_timed {
            return Err(Reject::Alt);
        }
        let cached = cached.as_ref().ok_or(Reject::Alt)?;
        p.alt_ft = cached.alt_ft;
        p.altitude_provenance = cached.altitude_provenance;
    }
    let alt_m = p.alt_m();
    if alt_m < f64::from(args.min_alt_m) || alt_m > f64::from(args.max_alt_m) {
        return Ok(None);
    }
    if !check::call(&p.call, db) {
        p.call = cached.as_ref().map_or(UNKNOWN.into(), |c| c.call.clone());
    }
    if !check::call(&p.call, db) {
        p.call = by_hex(&p.hex).map_or(UNKNOWN.into(), |a| a.call);
    }
    if !check::reg(&p.reg, db) {
        p.reg = cached.as_ref().map_or(UNKNOWN.into(), |c| c.reg.clone());
    }
    if !check::reg(&p.reg, db) {
        p.reg = by_hex(&p.hex).map_or(UNKNOWN.into(), |a| a.reg);
    }
    if !check::track(p.track) {
        return Err(Reject::Track);
    }
    if !check::speed(p.speed_kts) {
        if component_timed {
            return Err(Reject::Speed);
        }
        p.speed_kts = cached.as_ref().ok_or(Reject::Speed)?.speed_kts;
    }
    if !check::type_code(&p.type_code, db) {
        p.type_code = by_hex(&p.hex)
            .map(|aircraft| aircraft.type_code)
            .filter(|code| check::type_code(code, db))
            .unwrap_or_else(|| UNKNOWN.into());
        if p.type_code == UNKNOWN {
            p.model = UNKNOWN.into();
            p.manufacturer = UNKNOWN.into();
        }
    }
    let canonical_type = p.type_code.trim().to_ascii_uppercase();
    let recover_type_fields = p.manufacturer.is_empty()
        || p.model.is_empty()
        || p.manufacturer == UNKNOWN
        || p.model == UNKNOWN;
    if let Some(aircraft_type) = db
        .aircraft_type_find_by_icao(&canonical_type)
        .ok()
        .flatten()
    {
        if recover_type_fields {
            p.manufacturer = aircraft_type.manufacturer;
            p.model = aircraft_type.model;
        }
        if aircraft_type.category != PlaneCategory::None {
            p.category = aircraft_type.category;
        }
    } else if recover_type_fields {
        p.manufacturer = UNKNOWN.into();
        p.model = UNKNOWN.into();
    }
    if canonical_type == "A388" {
        p.category = PlaneCategory::Superheavy;
    }
    if let Some(rest) = p.model.strip_prefix(p.manufacturer.as_str()) {
        p.model = rest.trim().to_string();
    }
    // extended plausibility check against the last known report (forward-only, #50/#47)
    plausibility_pass(&p, cached.as_ref(), cfg)?;
    Ok(Some(p))
}

/// The extended plausibility check against the accepted live state `current` (the stored report
/// for the same hex): the shortest-angular-separation track check once there is a displacement
/// (#47), then the distance budget (#50). `current` is filtered to forward-only comparisons
/// within 300 s; with no such previous report — or when the extended check is off — it passes.
pub fn plausibility_pass(
    p: &PlaneInfo,
    current: Option<&PlaneInfo>,
    cfg: &PipelineConfig,
) -> Result<(), Reject> {
    if !cfg.extended_plausibility {
        return Ok(());
    }
    let Some(last) = current.filter(|l| {
        // forward-only (#50): an older report never borrows a newer one as its baseline
        p.time > l.time && elapsed_secs(l.time, p.time) <= 300.0
    }) else {
        return Ok(());
    };
    let Some(est) = PlaneInfoCache::extrapolate_entry(last, p.time, 5) else {
        return Ok(());
    };
    let (track, dist) = plausibility_offsets(p, last, &est);
    // shortest angular separation, but only once there is a displacement with a defined
    // bearing (#47): a zero-displacement update has `atan2(0, 0) == 0` and must not be
    // compared against the reported track, while the distance check stays active.
    let displaced = last.lat != p.lat || last.lon != p.lon;
    if displaced && track.is_finite() && heading_separation(track, p.track) > 45.0 {
        return Err(Reject::PlausibilityTrack);
    }
    let minutes = elapsed_secs(last.time, p.time) / 60.0;
    if dist.abs() > cfg.max_error_dist * minutes {
        return Err(Reject::PlausibilityDist);
    }
    Ok(())
}

pub(crate) fn plausibility_offsets(
    plane: &PlaneInfo,
    last: &PlaneInfo,
    estimated: &PlaneInfo,
) -> (f64, f64) {
    (
        latlon::bearing(last.lat, last.lon, plane.lat, plane.lon),
        latlon::distance(estimated.lat, estimated.lon, plane.lat, plane.lon),
    )
}

impl PipelineConfig {
    /// Report the rejection reason without interpolating feed-supplied fields.
    pub(crate) fn rejection_warning(&self, why: Reject) -> Option<String> {
        self.log_errors
            .then(|| format!("Rejected aircraft position: {why:?}"))
    }
}

/// Run all checks over one feed response.
pub fn process(
    raw: Vec<FeedPlane>,
    args: &FeedArgs,
    cfg: &PipelineConfig,
    cache: &PlaneInfoCache,
    db: &AircraftDb,
) -> Processed {
    let mut out = Processed::default();
    for ac in raw {
        match check_one(&ac, args, cfg, cache, db) {
            Ok(Some(p)) => out.planes.push(p),
            Ok(None) => {}
            Err(why) => out.rejected.push((ac, why)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::heading_separation;

    #[test]
    fn heading_separation_uses_shortest_angular_distance() {
        // opposite bearings are 180° apart (#47: the old fold mapped both to 90°)
        assert_eq!(heading_separation(90.0, 270.0), 180.0);
        // a turn across north is 20°, not 340°
        assert_eq!(heading_separation(350.0, 10.0), 20.0);
        assert_eq!(heading_separation(10.0, 350.0), 20.0);
        // the 45° boundary is inclusive: exactly 45° is not rejected
        assert_eq!(heading_separation(90.0, 135.0), 45.0);
        assert_eq!(heading_separation(0.0, 45.0), 45.0);
        assert_eq!(heading_separation(0.0, 0.0), 0.0);
    }
}
