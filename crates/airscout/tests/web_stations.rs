//! The web station routes (stage 4d Task 5): `/location.json`, `/qrv.json` and the C#
//! web lookups behind them.
mod common;

use airscout::services::open_data;
use airscout::web_api::{WebApi, WebCtx};
use airscout::web_stations::{location_find_or_create, qrv_defaulted, web_elevation, web_min_max};
use as_aircraft::PlaneInfoCache;
use as_elevation::ElevationModel;
use as_geo::band::Band;
use as_settings::{ElevationEnabled, Settings};
use as_stations::{default_antenna_gain, default_antenna_height, default_power, Qrv};
use as_stations::{GeoSource, Location};
use as_web::query::Query;
use as_web::route::{route, Route};
use as_web::server::Api;
use chrono::{Duration, Utc};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// The fixture settings: best case off (preflight C5), GLOBE only.
fn settings() -> Settings {
    Settings {
        path_best_case_elevation: false,
        band: Band::B1_2G,
        elevation_enabled: ElevationEnabled {
            globe: true,
            srtm3: false,
            srtm1: false,
            aster3: false,
            aster1: false,
        },
        ..Settings::default()
    }
}

/// A precise position inside JO50AA (not the locator centre).
const TE1ST_LAT: f64 = 50.01;
const TE1ST_LON: f64 = 10.03;

/// A web API on `dir` with `TE1ST` stored at JO50AA (`FromUser`, precise) and a 1.2G
/// QRV row for it.
fn api(dir: &Path, s: Settings) -> WebApi {
    let paths = open_data(dir).unwrap();
    let l = Location::new(
        "TE1ST",
        "JO50AA",
        TE1ST_LAT,
        TE1ST_LON,
        GeoSource::FromUser,
        5,
        Utc::now() - Duration::hours(1),
    );
    paths
        .stations
        .location_insert_or_update_if_newer(&l)
        .unwrap();
    let q = Qrv::new(
        "TE1ST",
        "JO50AA",
        Band::B1_2G,
        15.0,
        23.0,
        400.0,
        Utc::now(),
    )
    .unwrap();
    paths.stations.qrv_insert_or_update_if_newer(&q).unwrap();
    WebApi::new(WebCtx {
        paths,
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(s)),
        aircraft: None,
        tmp_dir: dir.join("tmp"),
        stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

/// The body `api` answers for the raw request target `raw`.
fn get(api: &WebApi, raw: &str) -> String {
    match route(raw) {
        Route::Api(r) => api.answer(r, raw, &Query::from_raw_url(raw)),
        other => panic!("{raw} routes to {other:?}"),
    }
}

fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

fn count(api: &WebApi) -> u64 {
    api.ctx.paths.stations.location_count().unwrap()
}

#[test]
fn location_from_db_with_elevation() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings());
    let body = get(&a, "/location.json?call=TE1ST");
    assert!(body.starts_with("{\n  \"Elevation\":"), "{body}");
    let v = json(&body);
    assert!(v["Elevation"].as_i64().unwrap() > 0, "{body}");
    assert_eq!(v["BestCaseElevation"], false);
    assert_eq!(v["Call"], "TE1ST");
    assert_eq!(v["Loc"], "JO50AA");
    assert_eq!(v["Lat"].as_f64(), Some(TE1ST_LAT));
    assert_eq!(v["Lon"].as_f64(), Some(TE1ST_LON));
    assert_eq!(v["Source"], 2);
    assert_eq!(v["Hits"], 5);
    let elv = web_elevation(
        &a.ctx.paths.store,
        &settings().elevation_enabled,
        TE1ST_LAT,
        TE1ST_LON,
    );
    assert_eq!(v["Elevation"].as_i64(), Some(i64::from(elv)));
}

#[test]
fn location_synthesised_from_loc_not_stored() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings());
    let before = count(&a);
    let v = json(&get(&a, "/location.json?call=TE2ND&loc=JO60AA"));
    assert_eq!(v["Call"], "TE2ND");
    assert_eq!(v["Loc"], "JO60AA");
    assert_eq!(v["Source"], 1);
    assert_eq!(v["Hits"], 0);
    assert_eq!(v["BestCaseElevation"], false);
    let (lat, lon) = as_geo::maidenhead::lat_lon_from_loc(
        "JO60AA",
        as_geo::maidenhead::PositionInRectangle::MiddleMiddle,
    )
    .unwrap();
    assert_eq!(v["Lat"].as_f64(), Some(lat));
    assert_eq!(v["Lon"].as_f64(), Some(lon));
    assert_eq!(count(&a), before);
}

#[test]
fn unknown_call_without_loc_answers() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    assert_eq!(
        get(&a, "/location.json?call=TE9XX"),
        "Error: TE9XX not found in database!"
    );
}

#[test]
fn location_all_returns_array() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings());
    let newer = Location::new(
        "TE1ST",
        "JO50BB",
        50.07,
        10.11,
        GeoSource::FromUser,
        1,
        Utc::now(),
    );
    a.ctx
        .paths
        .stations
        .location_insert_or_update_if_newer(&newer)
        .unwrap();
    let body = get(&a, "/location.json?call=te1st&loc=all");
    assert!(body.starts_with("[\n  {\n    \"Elevation\":"), "{body}");
    let v = json(&body);
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["Loc"], "JO50BB");
    assert_eq!(rows[1]["Loc"], "JO50AA");
    assert!(rows.iter().all(|r| r["Elevation"].as_i64().unwrap() > 0));
}

#[test]
fn location_all_unknown_is_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    assert_eq!(get(&a, "/location.json?call=TE9XX&loc=ALL"), "[]");
}

#[test]
fn best_case_moves_to_max() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings());
    let max = a
        .ctx
        .paths
        .store
        .max_elv_loc(ElevationModel::Globe, "JO50BB")
        .unwrap()
        .unwrap();
    let before = count(&a);
    let v = json(&get(
        &a,
        "/location.json?call=TE2ND&loc=JO50BB&bestcaseelevation=true",
    ));
    assert_eq!(v["Source"], 3);
    assert_eq!(v["BestCaseElevation"], true);
    assert_eq!(v["Lat"].as_f64(), Some(max.max_lat));
    assert_eq!(v["Lon"].as_f64(), Some(max.max_lon));
    assert_eq!(v["Elevation"].as_i64(), Some(i64::from(max.max_elv)));
    assert_eq!(count(&a), before);
    // a precise stored position does not move
    let v = json(&get(
        &a,
        "/location.json?call=TE1ST&loc=JO50AA&bestcaseelevation=TRUE",
    ));
    assert_eq!(v["Source"], 2);
    assert_eq!(v["BestCaseElevation"], false);
    assert_eq!(v["Lat"].as_f64(), Some(TE1ST_LAT));
}

#[test]
fn best_case_follows_settings_when_not_given() {
    let dir = common::data_dir_with_globe();
    let mut s = settings();
    s.path_best_case_elevation = true;
    let a = api(dir.path(), s);
    let v = json(&get(&a, "/location.json?call=TE2ND&loc=JO50BB"));
    assert_eq!(v["Source"], 3);
    let v = json(&get(
        &a,
        "/location.json?call=TE2ND&loc=JO50BB&bestcaseelevation=false",
    ));
    assert_eq!(v["Source"], 1);
}

#[test]
fn best_case_without_tiles_keeps_position() {
    let dir = common::data_dir_with_globe();
    let mut s = settings();
    s.elevation_enabled.globe = false;
    let a = api(dir.path(), s);
    let v = json(&get(
        &a,
        "/location.json?call=TE2ND&loc=JO60AA&bestcaseelevation=true",
    ));
    // no tile covers the square: the station keeps its locator centre and source (#263)
    let (lat, lon) = as_geo::maidenhead::lat_lon_from_loc(
        "JO60AA",
        as_geo::maidenhead::PositionInRectangle::MiddleMiddle,
    )
    .unwrap();
    assert_eq!(v["Lat"].as_f64(), Some(lat));
    assert_eq!(v["Lon"].as_f64(), Some(lon));
    assert_eq!(v["Source"], 1);
    assert_eq!(v["BestCaseElevation"], false);
}

#[test]
fn web_min_max_without_tile_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    assert!(web_min_max(&a.ctx.paths.store, &settings().elevation_enabled, "JO60AA").is_none());
}

#[test]
fn web_elevation_invalid_coordinate_is_zero() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings());
    let en = settings().elevation_enabled;
    assert_eq!(web_elevation(&a.ctx.paths.store, &en, 91.0, 10.0), 0);
    assert_eq!(web_elevation(&a.ctx.paths.store, &en, f64::NAN, 10.0), 0);
    assert!(web_elevation(&a.ctx.paths.store, &en, TE1ST_LAT, TE1ST_LON) > 0);
}

#[test]
fn parameter_errors_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    let cases = [
        (
            "/location.json?call=TE1ST&bestcaseelevation=maybe",
            "Error: MAYBE is not a valid value for best case elevation (true, false)!",
        ),
        ("/location.json?call=x", "Error: X is not a valid callsign!"),
        ("/location.json", "Error:  is not a valid callsign!"),
        (
            "/location.json?call=TE1ST&loc=zz99",
            "Error: ZZ99 is not a valid Maidenhead locator!",
        ),
        (
            "/qrv.json?call=x&loc=JO50AA",
            "Error: X is not a valid callsign!",
        ),
        (
            "/qrv.json?call=TE1ST&loc=zz99",
            "Error: ZZ99 is not a valid Maidenhead locator!",
        ),
        (
            "/qrv.json?call=TE1ST",
            "Error:  is not a valid Maidenhead locator!",
        ),
    ];
    for (raw, want) in cases {
        assert_eq!(get(&a, raw), want, "{raw}");
    }
}

#[test]
fn qrv_routes() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    // empty band: the settings band (1.2G)
    for raw in [
        "/qrv.json?call=TE1ST&loc=JO50AA",
        "/qrv.json?call=te1st&loc=jo50aa&band=1.2g",
    ] {
        let body = get(&a, raw);
        assert!(body.starts_with("{\n  \"Call\": \"TE1ST\","), "{body}");
        let v = json(&body);
        assert_eq!(v["Loc"], "JO50AA");
        assert_eq!(v["Band"], 1296);
        assert_eq!(v["AntennaHeight"].as_f64(), Some(15.0));
        assert_eq!(v["AntennaGain"].as_f64(), Some(23.0));
        assert_eq!(v["Power"].as_f64(), Some(400.0));
    }
    assert_eq!(
        get(&a, "/qrv.json?call=TE1ST&loc=JO50AA&band=144M"),
        "Error: QRV info not found in database!"
    );
    let body = get(&a, "/qrv.json?call=TE1ST&loc=JO50AA&band=all");
    assert!(body.starts_with("[\n  {"), "{body}");
    let v = json(&body);
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["Band"], 1296);
    assert_eq!(
        get(&a, "/qrv.json?call=TE2ND&loc=JO60AA&band=all"),
        "Error: QRV info not found in database!"
    );
    assert_eq!(
        get(&a, "/qrv.json?call=TE1ST&loc=JO50AA&band=7m"),
        "Error: 7M is not a valid band value!"
    );
    assert_eq!(
        get(&a, "/qrv.json?call=TE1ST&loc=JO50AA&band=none"),
        "Error: NONE is not a valid band value!"
    );
}

#[test]
fn qrv_coarse_locator_is_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    // a 4-character locator is a valid Maidenhead locator but can never match a stored
    // six-character key: exact lookup answers "not found", not an invalid-locator error (#13)
    assert_eq!(
        get(&a, "/qrv.json?call=TE1ST&loc=JO50"),
        "Error: QRV info not found in database!"
    );
    assert_eq!(
        get(&a, "/qrv.json?call=TE1ST&loc=JO50&band=all"),
        "Error: QRV info not found in database!"
    );
}

#[test]
fn qrv_defaulted_replaces_zeros() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    let db = &a.ctx.paths.stations;
    let z = Qrv::new("TE2ND", "JO60AA", Band::B144M, 0.0, 0.0, 0.0, Utc::now()).unwrap();
    db.qrv_insert_or_update_if_newer(&z).unwrap();
    let q = qrv_defaulted(db, "TE2ND", "JO60AA", Band::B144M, None).unwrap();
    assert_eq!(q.antenna_height, default_antenna_height(Band::B144M));
    assert_eq!(q.antenna_gain, default_antenna_gain(Band::B144M));
    assert_eq!(q.power, default_power(Band::B144M));
    // stored values stay
    let q = qrv_defaulted(db, "TE1ST", "JO50AA", Band::B1_2G, None).unwrap();
    assert_eq!(
        (q.antenna_height, q.antenna_gain, q.power),
        (15.0, 23.0, 400.0)
    );
    // not stored: the defaults, and nothing is written
    let before = db.qrv_count().unwrap();
    let q = qrv_defaulted(db, "TE2ND", "JO60AA", Band::B10G, None).unwrap();
    assert_eq!(q.power, default_power(Band::B10G));
    assert_eq!(db.qrv_count().unwrap(), before);
}

#[test]
fn find_or_create_stores_and_keeps_source() {
    let dir = common::data_dir_with_globe();
    let mut s = settings();
    s.path_best_case_elevation = true;
    let a = api(dir.path(), s.clone());
    let before = count(&a);
    let l = location_find_or_create(&a.ctx, &s, "TE2ND", "JO50BB")
        .unwrap()
        .unwrap();
    assert_eq!(count(&a), before + 1);
    // best case moves the position but, 1:1, not the source
    assert!(l.best_case);
    assert_eq!(l.loc.source, GeoSource::FromLoc);
    let max = a
        .ctx
        .paths
        .store
        .max_elv_loc(ElevationModel::Globe, "JO50BB")
        .unwrap()
        .unwrap();
    assert_eq!((l.loc.lat, l.loc.lon), (max.max_lat, max.max_lon));
    assert_eq!(i32::from(l.elevation), max.max_elv);
    // the stored row keeps the locator centre
    let stored = a
        .ctx
        .paths
        .stations
        .location_find("TE2ND", Some("JO50BB"))
        .unwrap()
        .unwrap();
    assert_ne!(stored.lat, l.loc.lat);
    assert!(location_find_or_create(&a.ctx, &s, "x", "JO50BB")
        .unwrap()
        .is_none());
    assert!(location_find_or_create(&a.ctx, &s, "TE2ND", "ZZ99")
        .unwrap()
        .is_none());
}
