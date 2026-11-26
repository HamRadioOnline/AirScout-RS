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

fn fold(t: f64) -> f64 {
    if t <= 180.0 {
        t
    } else {
        360.0 - t
    }
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
        call: ac.call.clone(),
        reg: ac.reg.clone(),
        track: ac.track,
        speed_kts: ac.speed_kts,
        time,
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
    if !check::alt(p.alt_ft) {
        p.alt_ft = cached.as_ref().ok_or(Reject::Alt)?.alt_ft;
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
        p.speed_kts = cached.as_ref().ok_or(Reject::Speed)?.speed_kts;
    }
    if !check::type_code(&p.type_code, db) {
        match by_hex(&p.hex) {
            Some(a) => p.type_code = a.type_code,
            None => {
                p.type_code = UNKNOWN.into();
                p.model = UNKNOWN.into();
                p.manufacturer = UNKNOWN.into();
                p.category = PlaneCategory::None;
            }
        }
    }
    // the original compares the manufacturer with "[unkonwn]" (typo, #48)
    if p.manufacturer.is_empty()
        || p.model.is_empty()
        || p.manufacturer == "[unkonwn]"
        || p.model == UNKNOWN
    {
        match db.aircraft_type_find_by_icao(&p.type_code).ok().flatten() {
            Some(td) => {
                p.manufacturer = td.manufacturer;
                p.model = td.model;
                p.category = td.category;
            }
            None => {
                p.manufacturer = UNKNOWN.into();
                p.model = UNKNOWN.into();
                p.category = PlaneCategory::None;
            }
        }
    }
    if let Some(rest) = p.model.strip_prefix(p.manufacturer.as_str()) {
        p.model = rest.trim().to_string();
    }
    // extended plausibility check against the last known report
    let last = cached.filter(|l| (p.time - l.time).num_milliseconds() as f64 / 1000.0 <= 300.0);
    let est = if last.is_some() {
        cache.get(&p.hex, p.time, 5)
    } else {
        None
    };
    if let (true, Some(last), Some(est)) = (cfg.extended_plausibility, last, est) {
        let track = latlon::bearing(last.lat, last.lon, p.lat, p.lon);
        let dist = latlon::distance(est.lat, est.lon, p.lat, p.lon);
        // folding maps e.g. 90° and 270° to the same value (#47)
        if (fold(track) - fold(p.track)).abs() > 45.0 {
            return Err(Reject::PlausibilityTrack);
        }
        let minutes = (p.time - last.time).num_milliseconds() as f64 / 60_000.0;
        if dist.abs() > cfg.max_error_dist * minutes {
            return Err(Reject::PlausibilityDist);
        }
    }
    Ok(Some(p))
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
