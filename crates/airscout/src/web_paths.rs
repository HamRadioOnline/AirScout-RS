//! `/elevationpath.json`, `/propagationpath.json` and `/nearestplanes.json` of the C# web
//! server (`AirScout/Webserver.cs:621-1048`).
//!
//! The routes read stored elevation tiles only: no tile is downloaded. A GET writes the
//! computed elevation or propagation path when it is valid (1:1, #386); the station lookups
//! are read-only (#265), so no `Location` row is written.
//!
//! Requests that never complete in C# answer here (L8 deviation): no elevation model, a
//! band without band settings, a position without a locator, and every exception
//! (`"Error: {e}"`).
//!
//! No lock is held across a path calculation: the stores lock per query. Every path
//! calculation runs with [`WebCtx::stop`] as its cancel token. A cancelled calculation
//! saves nothing and answers [`CANCELLED`]. C# answers an empty body for a `null` path
//! (`:753-754`), which only a cancel gives; the text says why instead.

use crate::services::local_obstruction;
use crate::web_api::WebCtx;
use crate::web_stations::{location_find, qrv_defaulted, qrv_json, web_elevation, WebLocation};
use as_aircraft::crossing::get_nearest_planes;
use as_aircraft::PlaneInfo;
use as_elevation::{ElevationModel, ElevationPath};
use as_geo::band::Band;
use as_geo::latlon::{GPoint, EARTH_RADIUS_KM};
use as_geo::maidenhead;
use as_propagation::{PathParams, PropagationPath};
use as_settings::{BandSettings, Settings};
use as_stations::{callsign, GeoSource, Qrv};
use as_web::dto::{
    ElevationPathJson, GPointJson, LocationJson, PlaneInfoJson, PropagationPathJson,
};
use as_web::json::{to_indented, Num};
use as_web::query::{parse_float, Query};
use chrono::{DateTime, Utc};
use std::fmt::Display;

const MY_NOT_FOUND: &str = "Error: MyLocation not found in database!";
const DX_NOT_FOUND: &str = "Error: DXLocation not found in database!";
/// The answer of a path calculation that the server stop cancelled.
pub const CANCELLED: &str = "Error: path calculation cancelled!";

/// A route answer: the body, or the error text that is the body.
type Answer = Result<String, String>;

fn error(e: impl Display) -> String {
    format!("Error: {e}")
}

/// A query parameter as C# reads it: upper-cased, or empty when missing.
fn param(q: &Query, key: &str) -> String {
    q.get(key).unwrap_or_default()
}

fn check_call(call: &str) -> Result<(), String> {
    if callsign::check(call) {
        Ok(())
    } else {
        Err(format!("Error: {call} is not a valid callsign!"))
    }
}

/// An empty locator passes, as in C#.
fn check_loc(loc: &str) -> Result<(), String> {
    if loc.is_empty() || maidenhead::check(loc) {
        Ok(())
    } else {
        Err(format!("Error: {loc} is not a valid Maidenhead locator!"))
    }
}

/// `double.TryParse` of a non-empty value; an empty value is "not given" (`NaN` in C#).
fn coordinate(v: &str, what: &str) -> Result<Option<f64>, String> {
    if v.is_empty() {
        return Ok(None);
    }
    parse_float(v)
        .map(Some)
        .ok_or_else(|| format!("Error: {v} is not a valid {what}!"))
}

/// The position parameters of one station.
struct StationQuery {
    call: String,
    lat: Option<f64>,
    lon: Option<f64>,
    loc: String,
}

/// The parameters of the elevation and propagation path routes, checked in the C# order
/// (`Webserver.cs:668-683`): both calls, then latitude, longitude and locator of My,
/// then those of DX.
fn path_query(q: &Query) -> Result<(StationQuery, StationQuery), String> {
    let my_call = param(q, "MYCALL");
    let dx_call = param(q, "DXCALL");
    check_call(&my_call)?;
    check_call(&dx_call)?;
    let station = |prefix: &str, call: String| -> Result<StationQuery, String> {
        let lat = coordinate(&param(q, &format!("{prefix}LAT")), "latitude")?;
        let lon = coordinate(&param(q, &format!("{prefix}LON")), "longitude")?;
        let loc = param(q, &format!("{prefix}LOC"));
        check_loc(&loc)?;
        Ok(StationQuery {
            call,
            lat,
            lon,
            loc,
        })
    };
    Ok((station("MY", my_call)?, station("DX", dx_call)?))
}

/// The model of the settings; C# runs on with none (L8 deviation).
fn model(s: &Settings) -> Result<ElevationModel, String> {
    match s.elevation_enabled.effective_model() {
        ElevationModel::None => Err("Error: no elevation model enabled!".to_string()),
        m => Ok(m),
    }
}

/// `Path_Band_Settings[band]`, which throws in C# for a band without a row (L8 deviation).
fn band_settings(s: &Settings, band: Band) -> Result<BandSettings, String> {
    s.band_settings
        .get(&band)
        .copied()
        .ok_or_else(|| format!("Error: no band settings for {}!", band.name()))
}

/// The station position: the given coordinates if both are given, else the position of
/// `LocationFind(call, loc)`, which is returned too.
fn position(
    ctx: &WebCtx,
    s: &Settings,
    st: &StationQuery,
    not_found: &str,
) -> Result<(GPoint, Option<WebLocation>), String> {
    if let (Some(lat), Some(lon)) = (st.lat, st.lon) {
        return Ok((GPoint::new(lat, lon), None));
    }
    match location_find(ctx, s, &st.call, &st.loc, "").map_err(error)? {
        Some(l) => Ok((GPoint::new(l.loc.lat, l.loc.lon), Some(l))),
        None => Err(not_found.to_string()),
    }
}

/// `LocFromLatLon(lat, lon, false, 3)`. C# gets `null` for a position out of range and
/// fails later (L8 deviation).
fn loc6(p: GPoint) -> Result<String, String> {
    maidenhead::loc_from_lat_lon(p.lat, p.lon, false, 3, false)
        .map_err(error)?
        .ok_or_else(|| format!("Error: no Maidenhead locator for {}, {}!", p.lat, p.lon))
}

/// The trusted station position for a six-character QRV key (#13), or `None` when the location
/// is a `FromLoc` midpoint or a terrain-selected (`FromBest`) point — positional fallback only.
fn trusted_point(l: &WebLocation) -> Option<GPoint> {
    match l.loc.source {
        GeoSource::FromLoc | GeoSource::FromBest => None,
        _ => Some(GPoint::new(l.loc.lat, l.loc.lon)),
    }
}

/// The locator and trusted position for a station's QRV lookup (#13). Supplied coordinates are
/// trusted and give their six-character locator; otherwise the original supplied locator is kept
/// (a coarse locator stays coarse), with a trusted position only for a stored station whose
/// source is not `FromLoc`. `loc` is `None` when the coordinates were supplied.
fn qrv_lookup(
    st: &StationQuery,
    loc: Option<&WebLocation>,
) -> Result<(String, Option<GPoint>), String> {
    if let (Some(lat), Some(lon)) = (st.lat, st.lon) {
        return Ok((loc6(GPoint::new(lat, lon))?, None));
    }
    let Some(l) = loc else {
        return Ok((st.loc.clone(), None));
    };
    // an empty supplied locator means "the latest stored location", whose own locator is the key
    let key_loc = if st.loc.is_empty() {
        l.loc.loc.clone()
    } else {
        st.loc.clone()
    };
    Ok((key_loc, trusted_point(l)))
}

/// `PropagationPathFindOrCreateFromLatLon` with the arguments of `:897-910` and
/// `:1003-1016`: the band's GHz, K factor and F1 clearance, the model's step width and
/// the local obstruction at My for the antenna height of My.
#[allow(clippy::too_many_arguments)]
fn propagation(
    ctx: &WebCtx,
    model: ElevationModel,
    band: Band,
    bs: &BandSettings,
    (my, h1): (GPoint, f64),
    (dx, h2): (GPoint, f64),
    my_raw: GPoint,
    dx_raw: GPoint,
    my_antenna_height: f64,
) -> Result<Option<PropagationPath>, String> {
    let store = &ctx.paths.store;
    // the obstruction is stored at the settings position, not at the shifted one (#105)
    let obstruction =
        local_obstruction(store, model, my_raw, dx_raw, my_antenna_height).map_err(error)?;
    let p = PathParams {
        lat1: my.lat,
        lon1: my.lon,
        h1,
        lat2: dx.lat,
        lon2: dx.lon,
        h2,
        qrg_ghz: band.to_ghz(),
        radius_km: EARTH_RADIUS_KM * bs.k_factor,
        f1_clearance: bs.f1_clearance,
        step_width_m: model.default_step_width_m(),
        local_obstruction: obstruction,
    };
    ctx.paths
        .engine
        .find_or_create_path(model, &p, true, Some(&ctx.stop))
        .map_err(error)
}

/// `/elevationpath.json` (`Webserver.cs:621-763`) on the settings band.
pub fn elevation_path(ctx: &WebCtx, s: &Settings, q: &Query) -> String {
    elevation_path_answer(ctx, s, q).unwrap_or_else(|e| e)
}

fn elevation_path_answer(ctx: &WebCtx, s: &Settings, q: &Query) -> Answer {
    let (my_q, dx_q) = path_query(q)?;
    let model = model(s)?;
    let (my, my_loc) = position(ctx, s, &my_q, MY_NOT_FOUND)?;
    let (my_qrv_loc, my_trusted) = qrv_lookup(&my_q, my_loc.as_ref())?;
    let (dx, dx_loc) = position(ctx, s, &dx_q, DX_NOT_FOUND)?;
    let (dx_qrv_loc, dx_trusted) = qrv_lookup(&dx_q, dx_loc.as_ref())?;
    let db = &ctx.paths.stations;
    let my_qrv = qrv_defaulted(db, &my_q.call, &my_qrv_loc, s.band, my_trusted).map_err(error)?;
    let dx_qrv = qrv_defaulted(db, &dx_q.call, &dx_qrv_loc, s.band, dx_trusted).map_err(error)?;
    let step = model.default_step_width_m();
    let found = ctx
        .paths
        .store
        .find_or_create_path(
            model,
            my.lat,
            my.lon,
            dx.lat,
            dx.lon,
            step,
            true,
            Some(&ctx.stop),
        )
        .map_err(error)?;
    let Some(ep) = found else {
        return Err(CANCELLED.to_string());
    };
    Ok(to_indented(&elevation_path_json(
        &ep,
        [my_loc.as_ref(), dx_loc.as_ref()],
        [&my_qrv, &dx_qrv],
    )))
}

/// `/propagationpath.json` (`Webserver.cs:765-921`).
///
/// The requested `band` parameter drives the QRG, the F1 clearance and the QRV defaults; an empty
/// `band` uses the settings band, and an invalid, `None` or `All` value answers the standard band
/// error text (#262). The station lookups are read-only (#265).
pub fn propagation_path(ctx: &WebCtx, s: &Settings, q: &Query) -> String {
    propagation_path_answer(ctx, s, q).unwrap_or_else(|e| e)
}

fn propagation_path_answer(ctx: &WebCtx, s: &Settings, q: &Query) -> Answer {
    let (my_q, dx_q) = path_query(q)?;
    let model = model(s)?;
    let band_text = param(q, "BAND");
    let band = if band_text.is_empty() {
        s.band
    } else {
        Band::parse(&band_text)
    };
    if band == Band::None || band == Band::All {
        return Err(format!("Error: {band_text} is not a valid band value!"));
    }
    // before any location is looked up
    let bs = band_settings(s, band)?;
    let (my, my_found) = position(ctx, s, &my_q, MY_NOT_FOUND)?;
    let my_loc6 = loc6(my)?;
    let my_loc = location_find(ctx, s, &my_q.call, &my_loc6, "")
        .map_err(error)?
        .ok_or_else(|| MY_NOT_FOUND.to_string())?;
    let (dx, dx_found) = position(ctx, s, &dx_q, DX_NOT_FOUND)?;
    let dx_loc6 = loc6(dx)?;
    let dx_loc = location_find(ctx, s, &dx_q.call, &dx_loc6, "")
        .map_err(error)?
        .ok_or_else(|| DX_NOT_FOUND.to_string())?;
    let db = &ctx.paths.stations;
    let (my_qrv_loc, my_trusted) = qrv_lookup(&my_q, my_found.as_ref())?;
    let (dx_qrv_loc, dx_trusted) = qrv_lookup(&dx_q, dx_found.as_ref())?;
    let my_qrv = qrv_defaulted(db, &my_q.call, &my_qrv_loc, band, my_trusted).map_err(error)?;
    let dx_qrv = qrv_defaulted(db, &dx_q.call, &dx_qrv_loc, band, dx_trusted).map_err(error)?;
    let at = |l: &WebLocation, q: &Qrv| {
        (
            GPoint::new(l.loc.lat, l.loc.lon),
            f64::from(l.elevation) + q.antenna_height,
        )
    };
    let path = propagation(
        ctx,
        model,
        band,
        &bs,
        at(&my_loc, &my_qrv),
        at(&dx_loc, &dx_qrv),
        my_loc.raw,
        dx_loc.raw,
        my_qrv.antenna_height,
    )?;
    let Some(pp) = path else {
        return Err(CANCELLED.to_string());
    };
    Ok(to_indented(&propagation_path_json(
        &pp,
        [&my_loc, &dx_loc],
        [&my_qrv, &dx_qrv],
    )))
}

/// `/nearestplanes.json` (`Webserver.cs:923-1048`): the planes of the cache near the path
/// on the requested band (empty: the settings band), as an indented array.
///
/// The QRV rows are those of the found location's call and locator (`:978`, `:988`), and
/// the heights use `GetElevation` at the found position (`:1007`, `:1010`).
pub fn nearest_planes(ctx: &WebCtx, s: &Settings, q: &Query, now: DateTime<Utc>) -> String {
    nearest_planes_answer(ctx, s, q, now).unwrap_or_else(|e| e)
}

fn nearest_planes_answer(ctx: &WebCtx, s: &Settings, q: &Query, now: DateTime<Utc>) -> Answer {
    let my_call = param(q, "MYCALL");
    let my_loc = param(q, "MYLOC");
    let dx_call = param(q, "DXCALL");
    let dx_loc = param(q, "DXLOC");
    let band_text = param(q, "BAND");
    check_call(&my_call)?;
    check_call(&dx_call)?;
    check_loc(&my_loc)?;
    check_loc(&dx_loc)?;
    let band = if band_text.is_empty() {
        s.band
    } else {
        Band::parse(&band_text)
    };
    if band == Band::None {
        return Err(format!("Error: {band_text} is not a valid band value!"));
    }
    let model = model(s)?;
    let bs = band_settings(s, band)?;
    let my = location_find(ctx, s, &my_call, &my_loc, "")
        .map_err(error)?
        .ok_or_else(|| MY_NOT_FOUND.to_string())?;
    let dx = location_find(ctx, s, &dx_call, &dx_loc, "")
        .map_err(error)?
        .ok_or_else(|| DX_NOT_FOUND.to_string())?;
    let db = &ctx.paths.stations;
    let my_qrv =
        qrv_defaulted(db, &my.loc.call, &my.loc.loc, band, trusted_point(&my)).map_err(error)?;
    let dx_qrv =
        qrv_defaulted(db, &dx.loc.call, &dx.loc.loc, band, trusted_point(&dx)).map_err(error)?;
    let en = &s.elevation_enabled;
    let at = |l: &WebLocation, q: &Qrv| {
        let elv = web_elevation(&ctx.paths.store, en, l.loc.lat, l.loc.lon);
        (
            GPoint::new(l.loc.lat, l.loc.lon),
            f64::from(elv) + q.antenna_height,
        )
    };
    let path = propagation(
        ctx,
        model,
        band,
        &bs,
        at(&my, &my_qrv),
        at(&dx, &dx_qrv),
        my.raw,
        dx.raw,
        my_qrv.antenna_height,
    )?;
    let Some(pp) = path else {
        return Err(CANCELLED.to_string());
    };
    let planes = ctx.cache.get_all(now, s.planes_position_ttl_min);
    let nearest = get_nearest_planes(
        now,
        &pp,
        &planes,
        s.gui.planes_filter_max_circumcircle_km,
        bs.max_distance,
        f64::from(s.planes_max_alt_m),
        s.planes_max_delta_m,
    );
    Ok(to_indented(
        &nearest.iter().map(plane_json).collect::<Vec<_>>(),
    ))
}

fn elevation_path_json(
    ep: &ElevationPath,
    [l1, l2]: [Option<&WebLocation>; 2],
    [q1, q2]: [&Qrv; 2],
) -> ElevationPathJson {
    ElevationPathJson {
        valid: ep.valid,
        selected: false,
        location1: l1.map(LocationJson::from),
        location2: l2.map(LocationJson::from),
        qrv1: Some(qrv_json(q1)),
        qrv2: Some(qrv_json(q2)),
        bearing12: Num(ep.bearing12()),
        bearing21: Num(ep.bearing21()),
        distance: Num(ep.distance()),
        lat1: Num(ep.lat1),
        lon1: Num(ep.lon1),
        lat2: Num(ep.lat2),
        lon2: Num(ep.lon2),
        step_width: Num(ep.step_width_m),
        // a path has at most `MAX_PATH_POINTS` (2e7) samples
        count: i32::try_from(ep.count).unwrap_or(i32::MAX),
        path: ep.path.clone(),
        last_updated: ep.last_updated,
    }
}

fn propagation_path_json(
    pp: &PropagationPath,
    [l1, l2]: [&WebLocation; 2],
    [q1, q2]: [&Qrv; 2],
) -> PropagationPathJson {
    let p = &pp.params;
    PropagationPathJson {
        valid: pp.valid,
        selected: false,
        local_obstructed: pp.local_obstructed,
        local_obstruction: Num(p.local_obstruction),
        location1: Some(LocationJson::from(l1)),
        location2: Some(LocationJson::from(l2)),
        qrv1: Some(qrv_json(q1)),
        qrv2: Some(qrv_json(q2)),
        bearing12: Num(pp.bearing12()),
        bearing21: Num(pp.bearing21()),
        distance: Num(pp.distance()),
        lat1: Num(p.lat1),
        lon1: Num(p.lon1),
        h1: Num(p.h1),
        lat2: Num(p.lat2),
        lon2: Num(p.lon2),
        h2: Num(p.h2),
        qrg: Num(p.qrg_ghz),
        radius: Num(p.radius_km),
        f1_clearance: Num(p.f1_clearance),
        step_width: Num(p.step_width_m),
        eps1_min: Num(pp.eps1_min),
        eps2_min: Num(pp.eps2_min),
        last_updated: pp.last_updated,
    }
}

/// The JSON object of a `PlaneInfo` (`AirScout.Core/PlaneInfo.cs:22-99`).
pub fn plane_json(p: &PlaneInfo) -> PlaneInfoJson {
    PlaneInfoJson {
        track: Num(p.track),
        int_point: p.int_point.map(|i| GPointJson {
            lat: Num(i.lat),
            lon: Num(i.lon),
        }),
        int_qrb: Num(p.int_qrb),
        alt_diff: Num(p.alt_diff),
        potential: p.potential,
        eps1: Num(p.eps1),
        eps2: Num(p.eps2),
        theta1: Num(p.theta1),
        theta2: Num(p.theta2),
        angle: Num(p.angle),
        squint: Num(p.squint),
        signal_strength: Num(p.signal_strength),
        ambiguous: p.ambiguous,
        comment: p.comment.clone(),
        old_time: p.old_time,
        old_lat: Num(p.old_lat),
        old_lon: Num(p.old_lon),
        old_alt: Num(p.old_alt_ft),
        old_track: Num(p.old_track),
        old_speed: Num(p.old_speed_kts),
        time: p.time,
        call: p.call.clone(),
        reg: p.reg.clone(),
        hex: p.hex.clone(),
        lat: Num(p.lat),
        lon: Num(p.lon),
        alt: Num(p.alt_ft),
        alt_m: Num(p.alt_m()),
        speed: Num(p.speed_kts),
        speed_kmh: Num(p.speed_kmh()),
        type_: p.type_code.clone(),
        manufacturer: p.manufacturer.clone(),
        model: p.model.clone(),
        category: p.category as i32,
        from: p.from.clone(),
        to: p.to.clone(),
        v_speed: p.vspeed,
        altitude_estimate: Some(p.altitude_estimate()),
        altitude_provenance: Some(p.altitude_provenance.as_str().into()),
    }
}
