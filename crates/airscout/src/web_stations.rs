//! `/location.json`, `/qrv.json` and the station lookups of the C# web server
//! (`AirScout/Webserver.cs:209-374`, `:500-619`).
//!
//! The lookups read stored data only: no elevation tile is downloaded and no station is
//! stored (read-only remote lookups, #265).
//!
//! Requests that never complete in C# answer here (L8 deviation): an unknown call
//! without a locator, and every exception (a database error, a QRV locator shorter
//! than 6 characters, a position `IsPrecise` rejects) answers `"Error: {e}"`.

use crate::services::ground_elevation_with;
use crate::web_api::WebCtx;
use as_elevation::{ElevationModel, ElevationStore, ElvMinMaxInfo, ELV_MISSING};
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::{ElevationEnabled, Settings};
use as_stations::{
    callsign, default_antenna_gain, default_antenna_height, default_power, GeoSource, Location,
    Qrv, StationDb,
};
use as_web::dto::{LocationJson, QrvJson};
use as_web::json::{to_indented, Num};
use as_web::query::Query;
use chrono::Utc;

/// The models of the web lookups, most detailed first. ASTER is included (#264), so an
/// ASTER-only user gets real ground elevation on the web routes (`Webserver.cs:216-221`,
/// `:381-419`).
pub(crate) const WEB_MODEL_ORDER: [ElevationModel; 5] = [
    ElevationModel::Aster1,
    ElevationModel::Aster3,
    ElevationModel::Srtm1,
    ElevationModel::Srtm3,
    ElevationModel::Globe,
];

/// A location as the web routes serve it: the stored or synthesised row, with the
/// ground elevation and, with best case, the moved position.
#[derive(Clone, Debug, PartialEq)]
pub struct WebLocation {
    pub loc: Location,
    pub elevation: i16,
    pub best_case: bool,
    /// The position before the best-case shift, where a local obstruction is stored (#105).
    pub raw: GPoint,
}

impl From<&WebLocation> for LocationJson {
    fn from(w: &WebLocation) -> Self {
        LocationJson {
            elevation: w.elevation,
            best_case_elevation: w.best_case,
            call: w.loc.call.clone(),
            loc: w.loc.loc.clone(),
            lat: Num(w.loc.lat),
            lon: Num(w.loc.lon),
            source: w.loc.source as i32,
            hits: w.loc.hits,
            last_updated: w.loc.last_updated,
        }
    }
}

/// The JSON object of a QRV row (`QRVDesignator`).
pub fn qrv_json(q: &Qrv) -> QrvJson {
    QrvJson {
        call: q.call.clone(),
        loc: q.loc.clone(),
        band: q.band as i32,
        antenna_height: Num(q.antenna_height),
        antenna_gain: Num(q.antenna_gain),
        power: Num(q.power),
        last_updated: q.last_updated,
    }
}

/// [`web_elevation`] with the lookup given, for tests without tiles.
fn web_elevation_with(
    en: &ElevationEnabled,
    lookup: impl FnMut(ElevationModel) -> Result<i16, as_elevation::ElevationError>,
) -> i16 {
    // the walk yields an `i16` value or 0, so the cast is exact
    ground_elevation_with(en, &WEB_MODEL_ORDER, lookup) as i16
}

/// `GetElevation` (`Webserver.cs:209-225`): the stored elevation of the first enabled
/// model of [`WEB_MODEL_ORDER`] that has one. An invalid coordinate gives 0, a model
/// error counts as missing, and a missing value gives 0.
pub fn web_elevation(store: &ElevationStore, en: &ElevationEnabled, lat: f64, lon: f64) -> i16 {
    if !maidenhead::check_lat_lon(lat, lon) {
        return 0;
    }
    web_elevation_with(en, |m| store.elevation(m, lat, lon, false))
}

/// `GetMinMaxElevationLoc` (`Webserver.cs:376-427`): the tile info of the first enabled model
/// of [`WEB_MODEL_ORDER`] whose maximum is known, or `None` when no tile covers the square.
/// A missing maximum counts as "tile not stored" (it walks on to the next model), and a store
/// error counts as "tile not stored".
pub fn web_min_max(
    store: &ElevationStore,
    en: &ElevationEnabled,
    loc: &str,
) -> Option<ElvMinMaxInfo> {
    let missing = i32::from(ELV_MISSING);
    for m in WEB_MODEL_ORDER {
        if !en.is_enabled(m) {
            continue;
        }
        if let Ok(Some(info)) = store.max_elv_loc(m, loc) {
            if info.max_elv != missing {
                return Some(info);
            }
        }
    }
    None
}

/// The elevation and the best-case shift of `LocationFind` and its siblings
/// (`Webserver.cs:269-286`). Nothing is stored. `from_best` sets the source to
/// `FromBest`, which `LocationFindOrCreate` does not do (`:306-321`). Without a tile that
/// covers the square the station is not shifted (#263): it keeps its locator position,
/// `best_case == false` and its source.
fn web_location(
    store: &ElevationStore,
    en: &ElevationEnabled,
    loc: Location,
    best: bool,
    from_best: bool,
) -> Result<WebLocation, String> {
    let raw = GPoint::new(loc.lat, loc.lon);
    let mut w = WebLocation {
        elevation: web_elevation(store, en, loc.lat, loc.lon),
        loc,
        best_case: false,
        raw,
    };
    if best && !maidenhead::is_precise(w.loc.lat, w.loc.lon, 3).map_err(|e| e.to_string())? {
        if let Some(max) = web_min_max(store, en, &w.loc.loc) {
            w.loc.lat = max.max_lat;
            w.loc.lon = max.max_lon;
            // a tile elevation is an `i16`
            w.elevation = max.max_elv.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
            if from_best {
                w.loc.source = GeoSource::FromBest;
            }
            w.best_case = true;
        }
    }
    Ok(w)
}

/// The best-case switch of a request: the settings if `best` is empty, else whether it
/// reads `TRUE` (`Webserver.cs:236-245`).
fn best_case(s: &Settings, best: &str) -> bool {
    if best.is_empty() {
        s.path_best_case_elevation
    } else {
        best.trim().to_uppercase() == "TRUE"
    }
}

/// `LocationFind` (`Webserver.cs:228-289`).
/// - An invalid call, or a non-empty invalid locator, gives `None`.
/// - An empty `loc` gives the latest stored location of the call, or `None`.
/// - A `loc` that is not stored gives its locator centre (`FromLoc`, 0 hits, now),
///   which is not stored.
pub fn location_find(
    ctx: &WebCtx,
    s: &Settings,
    call: &str,
    loc: &str,
    best: &str,
) -> Result<Option<WebLocation>, String> {
    if !callsign::check(call) || (!loc.is_empty() && !maidenhead::check(loc)) {
        return Ok(None);
    }
    let best = best_case(s, best);
    let db = &ctx.paths.stations;
    let l = if loc.is_empty() {
        match db.location_find(call, None).map_err(|e| e.to_string())? {
            Some(l) => l,
            None => return Ok(None),
        }
    } else {
        match db
            .location_find(call, Some(loc))
            .map_err(|e| e.to_string())?
        {
            Some(l) => l,
            None => {
                let c = maidenhead::gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle)
                    .map_err(|e| e.to_string())?;
                Location::new(call, loc, c.lat, c.lon, GeoSource::FromLoc, 0, Utc::now())
            }
        }
    };
    let store = &ctx.paths.store;
    web_location(store, &s.elevation_enabled, l, best, true).map(Some)
}

/// `LocationFindAll` (`Webserver.cs:326-374`): every stored location of the call, most
/// recent first, each as [`location_find`] gives it. An invalid call gives none.
pub fn location_find_all(
    ctx: &WebCtx,
    s: &Settings,
    call: &str,
    best: &str,
) -> Result<Vec<WebLocation>, String> {
    if !callsign::check(call) {
        return Ok(Vec::new());
    }
    let best = best_case(s, best);
    let rows = ctx
        .paths
        .stations
        .locations_by_call(call)
        .map_err(|e| e.to_string())?;
    let store = &ctx.paths.store;
    rows.into_iter()
        .map(|l| web_location(store, &s.elevation_enabled, l, best, true))
        .collect()
}

/// `LocationFindOrCreate` (`Webserver.cs:291-324`): the stored location, or a new one at
/// the locator centre, which is stored. Best case follows the settings, and the source
/// stays as stored (1:1). An invalid call or locator gives `None`. No route calls this any
/// more: the remote lookups are read-only (#265), so this port is kept for completeness and
/// is exercised only by its test.
pub fn location_find_or_create(
    ctx: &WebCtx,
    s: &Settings,
    call: &str,
    loc: &str,
) -> Result<Option<WebLocation>, String> {
    if !callsign::check(call) || !maidenhead::check(loc) {
        return Ok(None);
    }
    let Some(l) = ctx
        .paths
        .stations
        .location_find_or_create(call, loc)
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let store = &ctx.paths.store;
    let best = s.path_best_case_elevation;
    web_location(store, &s.elevation_enabled, l, best, false).map(Some)
}

/// `QRVFindOrCreateDefault` with the zero defaults of the path routes
/// (`Webserver.cs:724-741`, `:871-889`, `:977-995`): a zero height, gain or power is the
/// band default. Nothing is stored. `trusted` is a station position inside the coarse
/// locator that may derive the six-character key for an exact lookup (#13).
pub fn qrv_defaulted(
    db: &StationDb,
    call: &str,
    loc: &str,
    band: Band,
    trusted: Option<GPoint>,
) -> Result<Qrv, String> {
    let mut q = db
        .qrv_find_or_create_default(call, loc, band, trusted)
        .map_err(|e| e.to_string())?;
    if q.antenna_height == 0.0 {
        q.antenna_height = default_antenna_height(band);
    }
    if q.antenna_gain == 0.0 {
        q.antenna_gain = default_antenna_gain(band);
    }
    if q.power == 0.0 {
        q.power = default_power(band);
    }
    Ok(q)
}

/// A query parameter as C# reads it: upper-cased, or empty when missing.
fn param(q: &Query, key: &str) -> String {
    q.get(key).unwrap_or_default()
}

fn invalid_call(call: &str) -> String {
    format!("Error: {call} is not a valid callsign!")
}

fn invalid_loc(loc: &str) -> String {
    format!("Error: {loc} is not a valid Maidenhead locator!")
}

/// `/location.json` (`Webserver.cs:500-561`).
///
/// C# answers `"Error while parsing parameters!"` when `ParseQueryString` throws. That
/// parser never throws on a string, and [`Query::from_raw_url`] cannot fail, so the
/// text has no path here.
pub fn location(ctx: &WebCtx, s: &Settings, q: &Query) -> String {
    let call = param(q, "CALL");
    let loc = param(q, "LOC");
    let best = param(q, "BESTCASEELEVATION");
    if !best.is_empty() && best != "TRUE" && best != "FALSE" {
        return format!(
            "Error: {best} is not a valid value for best case elevation (true, false)!"
        );
    }
    if !callsign::check(&call) {
        return invalid_call(&call);
    }
    if loc == "ALL" {
        return match location_find_all(ctx, s, &call, &best) {
            Ok(rows) => to_indented(&rows.iter().map(LocationJson::from).collect::<Vec<_>>()),
            Err(e) => format!("Error: {e}"),
        };
    }
    if !loc.is_empty() && !maidenhead::check(&loc) {
        return invalid_loc(&loc);
    }
    match location_find(ctx, s, &call, &loc, &best) {
        Ok(Some(l)) => to_indented(&LocationJson::from(&l)),
        // C# dereferences null here and the request never completes (L8 deviation)
        Ok(None) => format!("Error: {call} not found in database!"),
        Err(e) => format!("Error: {e}"),
    }
}

const QRV_NOT_FOUND: &str = "Error: QRV info not found in database!";

/// `/qrv.json` (`Webserver.cs:563-619`). One QRV row, or every band of the station for
/// `band=ALL`. An empty band is the settings band.
pub fn qrv(ctx: &WebCtx, s: &Settings, q: &Query) -> String {
    let call = param(q, "CALL");
    let loc = param(q, "LOC");
    let band_text = param(q, "BAND");
    if !callsign::check(&call) {
        return invalid_call(&call);
    }
    if !maidenhead::check(&loc) {
        return invalid_loc(&loc);
    }
    let band = if band_text.is_empty() {
        s.band
    } else {
        Band::parse(&band_text)
    };
    if band == Band::None {
        return format!("Error: {band_text} is not a valid band value!");
    }
    let db = &ctx.paths.stations;
    if band != Band::All {
        return match db.qrv_find(&call, &loc, band) {
            Ok(Some(r)) => to_indented(&qrv_json(&r)),
            Ok(None) => QRV_NOT_FOUND.to_string(),
            Err(e) => format!("Error: {e}"),
        };
    }
    match db.qrv_get_all(&call, &loc) {
        Ok(rows) if rows.is_empty() => QRV_NOT_FOUND.to_string(),
        Ok(rows) => to_indented(&rows.iter().map(qrv_json).collect::<Vec<_>>()),
        Err(e) => format!("Error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use as_elevation::TILE_MISSING;

    fn only(aster1: bool, srtm1: bool) -> ElevationEnabled {
        ElevationEnabled {
            globe: true,
            srtm3: true,
            srtm1,
            aster3: aster1,
            aster1,
        }
    }

    #[test]
    fn web_elevation_asks_aster_first() {
        // ASTER-only users now get real ground elevation (#264): ASTER1 is asked first
        let mut asked = Vec::new();
        let e = web_elevation_with(&only(true, false), |m| {
            asked.push(m);
            Ok(match m {
                ElevationModel::Aster1 | ElevationModel::Aster3 => 500,
                _ => ELV_MISSING,
            })
        });
        assert_eq!(e, 500);
        assert_eq!(asked, [ElevationModel::Aster1]);
    }

    #[test]
    fn web_elevation_falls_through_to_srtm1() {
        let e = web_elevation_with(&only(true, true), |m| {
            Ok(match m {
                ElevationModel::Srtm1 => 500,
                ElevationModel::Globe => 100,
                _ => ELV_MISSING,
            })
        });
        assert_eq!(e, 500);
    }

    #[test]
    fn web_elevation_tile_missing_is_zero() {
        let e = web_elevation_with(&only(false, true), |_| Ok(TILE_MISSING));
        assert_eq!(e, 0);
    }
}
