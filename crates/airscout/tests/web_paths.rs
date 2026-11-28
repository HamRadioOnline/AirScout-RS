//! The web path routes (stage 4d Task 6): `/elevationpath.json`, `/propagationpath.json`
//! and `/nearestplanes.json`.
mod common;

use airscout::services::open_data;
use airscout::web_api::{WebApi, WebCtx};
use airscout::web_paths::{elevation_path, nearest_planes, propagation_path};
use airscout::web_stations::web_elevation;
use as_aircraft::{AltitudeProvenance, PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_elevation::ElevationModel;
use as_geo::band::Band;
use as_geo::latlon::{self, EARTH_RADIUS_KM};
use as_geo::maidenhead;
use as_propagation::PathParams;
use as_settings::{default_band_table, ElevationEnabled, Settings};
use as_stations::{default_antenna_height, GeoSource, Location, Qrv};
use as_web::query::Query;
use as_web::route::{route, Route};
use as_web::server::Api;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// The fixture settings: best case off (preflight C5), GLOBE only, band 1.2G. The 144M
/// band reaches 50 km from the path, 1.2G 10 km.
fn settings() -> Settings {
    let mut s = Settings {
        path_best_case_elevation: false,
        band: Band::B1_2G,
        elevation_enabled: ElevationEnabled::only(ElevationModel::Globe),
        ..Settings::default()
    };
    s.band_settings.get_mut(&Band::B144M).unwrap().max_distance = 50.0;
    s.band_settings.get_mut(&Band::B1_2G).unwrap().max_distance = 10.0;
    s
}

/// Precise positions inside JO50AA and JO60AA (not the locator centres).
const TE1ST: (f64, f64) = (50.01, 10.03);
const TE2ND: (f64, f64) = (50.01, 12.03);
/// The antenna height of the stored 1.2G QRV row of TE1ST.
const TE1ST_HEIGHT: f64 = 15.0;

fn store(paths: &airscout::services::Paths, call: &str, loc: &str, (lat, lon): (f64, f64)) {
    let l = Location::new(
        call,
        loc,
        lat,
        lon,
        GeoSource::FromUser,
        5,
        Utc::now() - Duration::hours(1),
    );
    paths
        .stations
        .location_insert_or_update_if_newer(&l)
        .unwrap();
}

/// A web API on `dir` with `TE1ST` stored at JO50AA (with a 1.2G QRV row) and, if
/// `with_dx`, `TE2ND` at JO60AA (without QRV rows).
fn api(dir: &Path, s: Settings, with_dx: bool) -> WebApi {
    let paths = open_data(dir).unwrap();
    // as the app sets it (`Path_NearFieldSuppression`)
    paths.engine.set_near_field_suppression_m(100.0);
    store(&paths, "TE1ST", "JO50AA", TE1ST);
    if with_dx {
        store(&paths, "TE2ND", "JO60AA", TE2ND);
    }
    let q = Qrv::new(
        "TE1ST",
        "JO50AA",
        Band::B1_2G,
        TE1ST_HEIGHT,
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

const PAIR: &str = "mycall=TE1ST&dxcall=TE2ND";

#[test]
fn elevation_path_matches_store() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let body = get(&a, &format!("/elevationpath.json?{PAIR}"));
    assert!(body.starts_with("{\n  \"Valid\":"), "{body}");
    let v = json(&body);
    let m = ElevationModel::Globe;
    let ep = a
        .ctx
        .paths
        .store
        .find_or_create_path(
            m,
            TE1ST.0,
            TE1ST.1,
            TE2ND.0,
            TE2ND.1,
            m.default_step_width_m(),
            false,
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(v["Count"].as_u64(), Some(ep.count as u64));
    let path: Vec<i64> = ep.path.iter().map(|&e| i64::from(e)).collect();
    let served: Vec<i64> = v["Path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_i64().unwrap())
        .collect();
    assert_eq!(served, path);
    assert!(path.iter().any(|&e| e > 0), "terrain from the GLOBE tile");
    assert_eq!(v["StepWidth"].as_f64(), Some(m.default_step_width_m()));
    assert_eq!(v["Lat1"].as_f64(), Some(TE1ST.0));
    assert_eq!(v["Lon2"].as_f64(), Some(TE2ND.1));
    assert_eq!(v["Location1"]["Call"], "TE1ST");
    assert_eq!(v["Location2"]["Call"], "TE2ND");
    assert_eq!(v["QRV1"]["AntennaHeight"].as_f64(), Some(TE1ST_HEIGHT));
    assert_eq!(v["QRV1"]["Band"], Band::B1_2G as i32);
    assert_eq!(
        v["QRV2"]["AntennaHeight"].as_f64(),
        Some(default_antenna_height(Band::B1_2G))
    );
}

/// A `WebApi` over an already-populated `Paths`, storing nothing further.
fn from_paths(dir: &Path, s: Settings, paths: airscout::services::Paths) -> WebApi {
    WebApi::new(WebCtx {
        paths,
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(s)),
        aircraft: None,
        tmp_dir: dir.join("tmp"),
        stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

/// A stored QRV row for TE1ST at `loc` on 1.2G with a distinctive 30 m height.
fn seed_qrv(paths: &airscout::services::Paths, loc: &str) {
    paths
        .stations
        .qrv_insert_or_update_if_newer(
            &Qrv::new("TE1ST", loc, Band::B1_2G, 30.0, 23.0, 400.0, Utc::now()).unwrap(),
        )
        .unwrap();
}

#[test]
fn coarse_locator_alone_uses_band_defaults_not_seeded_qrv() {
    // A QRV row sits at JO50's midpoint JO50MM, but no Location for TE1ST/JO50: a coarse
    // locator with no trusted position must not fabricate a JO50MM key (#13).
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    seed_qrv(&paths, "JO50MM");
    store(&paths, "TE2ND", "JO60AA", TE2ND);
    let a = from_paths(dir.path(), settings(), paths);
    let v = json(&get(
        &a,
        "/elevationpath.json?mycall=TE1ST&myloc=JO50&dxcall=TE2ND&dxloc=JO60AA",
    ));
    assert_eq!(v["QRV1"]["Loc"], "JO50", "{v}");
    assert_eq!(
        v["QRV1"]["AntennaHeight"].as_f64(),
        Some(default_antenna_height(Band::B1_2G)),
        "{v}"
    );
}

#[test]
fn coarse_locator_with_stored_station_uses_trusted_position() {
    // A FromUser record inside JO50AA stored under the coarse locator JO50 derives the
    // six-character key from its trusted position (#13).
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    store(&paths, "TE1ST", "JO50", TE1ST);
    seed_qrv(&paths, "JO50AA");
    store(&paths, "TE2ND", "JO60AA", TE2ND);
    let a = from_paths(dir.path(), settings(), paths);
    let v = json(&get(
        &a,
        "/elevationpath.json?mycall=TE1ST&myloc=JO50&dxcall=TE2ND&dxloc=JO60AA",
    ));
    assert_eq!(v["QRV1"]["Loc"], "JO50AA", "{v}");
    assert_eq!(v["QRV1"]["AntennaHeight"].as_f64(), Some(30.0), "{v}");
}

#[test]
fn coarse_locator_with_supplied_coordinates_derives_key() {
    // Supplied coordinates are trusted and derive the six-character key even with a coarse
    // locator (#13).
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    seed_qrv(&paths, "JO50AA");
    store(&paths, "TE2ND", "JO60AA", TE2ND);
    let a = from_paths(dir.path(), settings(), paths);
    let v = json(&get(
        &a,
        "/elevationpath.json?mycall=TE1ST&mylat=50.01&mylon=10.03&myloc=JO50&dxcall=TE2ND&dxloc=JO60AA",
    ));
    assert_eq!(v["QRV1"]["Loc"], "JO50AA", "{v}");
    assert_eq!(v["QRV1"]["AntennaHeight"].as_f64(), Some(30.0), "{v}");
}

#[test]
fn coordinates_give_null_locations() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let body = get(
        &a,
        &format!("/elevationpath.json?mylat=50.5&mylon=10.5&{PAIR}"),
    );
    assert!(body.contains("\"Location1\": null"), "{body}");
    let v = json(&body);
    assert_eq!(v["Lat1"].as_f64(), Some(50.5));
    assert_eq!(v["Lon1"].as_f64(), Some(10.5));
    assert_eq!(v["Location2"]["Call"], "TE2ND");
    // the QRV row is looked up at the locator of the given position
    let loc = maidenhead::loc_from_lat_lon(50.5, 10.5, false, 3, false)
        .unwrap()
        .unwrap();
    assert_eq!(v["QRV1"]["Loc"], loc.as_str());
}

#[test]
fn only_one_coordinate_uses_the_database() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let v = json(&get(&a, &format!("/elevationpath.json?mylat=50.5&{PAIR}")));
    assert_eq!(v["Lat1"].as_f64(), Some(TE1ST.0));
    assert_eq!(v["Location1"]["Call"], "TE1ST");
}

/// The path parameters the route must use for TE1ST to TE2ND on `band`.
fn expected_params(a: &WebApi, band: Band) -> PathParams {
    let s = settings();
    let store = &a.ctx.paths.store;
    let en = &s.elevation_enabled;
    let bs = s.band_settings[&band];
    let m = ElevationModel::Globe;
    PathParams {
        lat1: TE1ST.0,
        lon1: TE1ST.1,
        h1: f64::from(web_elevation(store, en, TE1ST.0, TE1ST.1)) + TE1ST_HEIGHT,
        lat2: TE2ND.0,
        lon2: TE2ND.1,
        h2: f64::from(web_elevation(store, en, TE2ND.0, TE2ND.1)) + default_antenna_height(band),
        qrg_ghz: band.to_ghz(),
        radius_km: EARTH_RADIUS_KM * bs.k_factor,
        f1_clearance: bs.f1_clearance,
        step_width_m: m.default_step_width_m(),
        local_obstruction: f64::MIN,
    }
}

#[test]
fn propagation_path_matches_engine() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let body = get(&a, &format!("/propagationpath.json?{PAIR}"));
    assert!(body.starts_with("{\n  \"Valid\":"), "{body}");
    let v = json(&body);
    let p = expected_params(&a, Band::B1_2G);
    let pp = a
        .ctx
        .paths
        .engine
        .create_path(ElevationModel::Globe, &p, false, None)
        .unwrap()
        .unwrap();
    assert_eq!(v["Eps1_Min"].as_f64(), Some(pp.eps1_min));
    assert_eq!(v["Eps2_Min"].as_f64(), Some(pp.eps2_min));
    assert_eq!(v["h1"].as_f64(), Some(p.h1));
    assert_eq!(v["h2"].as_f64(), Some(p.h2));
    assert_eq!(v["QRG"].as_f64(), Some(p.qrg_ghz));
    assert_eq!(v["Radius"].as_f64(), Some(p.radius_km));
    assert_eq!(v["F1_Clearance"].as_f64(), Some(p.f1_clearance));
    assert_eq!(v["LocalObstruction"].as_f64(), Some(f64::MIN));
    assert_eq!(v["Location1"]["Call"], "TE1ST");
    assert_eq!(v["Location2"]["Call"], "TE2ND");
    assert_eq!(v["QRV1"]["AntennaHeight"].as_f64(), Some(TE1ST_HEIGHT));
}

#[test]
fn propagation_honors_band_param() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let b144 = json(&get(&a, &format!("/propagationpath.json?{PAIR}&band=144M")));
    // the requested band drives the QRG, F1 clearance and QRV defaults (#262)
    assert_eq!(b144["QRG"].as_f64(), Some(Band::B144M.to_ghz()));
    assert_eq!(
        b144["F1_Clearance"].as_f64(),
        Some(default_band_table()[&Band::B144M].f1_clearance)
    );
    assert_eq!(
        b144["QRV1"]["AntennaHeight"].as_f64(),
        Some(default_antenna_height(Band::B144M))
    );
    // no band: the settings band (1.2G)
    let b = json(&get(&a, &format!("/propagationpath.json?{PAIR}")));
    assert_eq!(b["QRG"].as_f64(), Some(Band::B1_2G.to_ghz()));
}

#[test]
fn propagation_bad_band_text() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    for band in ["7M", "none", "all"] {
        assert_eq!(
            get(&a, &format!("/propagationpath.json?{PAIR}&band={band}")),
            format!("Error: {} is not a valid band value!", band.to_uppercase())
        );
    }
}

#[test]
fn propagation_does_not_store_locations() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), false);
    let db = &a.ctx.paths.stations;
    assert!(db.location_find("TE2ND", None).unwrap().is_none());
    let before = db.location_count().unwrap();
    let body = get(
        &a,
        &format!("/propagationpath.json?{PAIR}&dxlat=50.03&dxlon=12.05"),
    );
    let v = json(&body);
    // answered from the in-memory locator centre; nothing is stored (#265)
    let centre =
        maidenhead::gpoint_from_loc("JO60AA", maidenhead::PositionInRectangle::MiddleMiddle)
            .unwrap();
    assert_eq!(v["Location2"]["Call"], "TE2ND");
    assert_eq!(v["Location2"]["Source"], 1);
    assert_eq!(v["Location2"]["Lat"].as_f64(), Some(centre.lat));
    assert_eq!(v["Lat2"].as_f64(), Some(centre.lat));
    assert_eq!(v["Lon2"].as_f64(), Some(centre.lon));
    assert_eq!(db.location_count().unwrap(), before);
    assert!(db.location_find("TE2ND", None).unwrap().is_none());
}

/// The GLOBE fixture ends at 12 E, so TE2ND (12.03 E) sits at sea level behind terrain,
/// and the minimum height at mid-path is about 24 km. 120 000 ft (36.6 km) is above it.
const ABOVE_PATH_FT: f64 = 120_000.0;

/// A plane at `lat`/`lon` flying `track`, above the path, reported at `now`.
fn plane(now: DateTime<Utc>, hex: &str, lat: f64, lon: f64, track: f64) -> PlaneInfo {
    PlaneInfo::new(
        now,
        "TST123",
        "",
        hex,
        lat,
        lon,
        track,
        ABOVE_PATH_FT,
        450.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

fn nearest(a: &WebApi, query: &str, now: DateTime<Utc>) -> Value {
    let s = a.ctx.live.read().unwrap().clone();
    let raw = format!("/nearestplanes.json?{query}");
    let body = nearest_planes(&a.ctx, &s, &Query::from_raw_url(&raw), now);
    json(&body)
}

#[test]
fn nearest_planes_lists_crossing_plane() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let now = Utc::now();
    let mid = latlon::midpoint(TE1ST.0, TE1ST.1, TE2ND.0, TE2ND.1);
    a.ctx
        .cache
        .insert_or_update_if_newer(&plane(now, "ABC001", mid.lat, mid.lon, 0.0));
    let v = nearest(&a, PAIR, now);
    let planes = v.as_array().unwrap();
    assert_eq!(planes.len(), 1, "{v}");
    assert_eq!(planes[0]["Hex"], "ABC001");
    assert!(planes[0]["Potential"].as_i64().unwrap() > 0, "{v}");
    assert!(planes[0]["IntPoint"]["Lat"].is_number(), "{v}");
}

#[test]
fn nearest_planes_exposes_altitude_provenance() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let now = Utc::now();
    let mid = latlon::midpoint(TE1ST.0, TE1ST.1, TE2ND.0, TE2ND.1);
    let mut p = plane(now, "ABC001", mid.lat, mid.lon, 0.0);
    p.altitude_provenance = AltitudeProvenance::PressureEstimate;
    a.ctx.cache.insert_or_update_if_newer(&p);
    let v = nearest(&a, PAIR, now);
    assert_eq!(v[0]["AltitudeEstimate"], true);
    assert_eq!(v[0]["AltitudeProvenance"], "PressureEstimate");
}

#[test]
fn nearest_planes_is_indented_array() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let body = get(&a, &format!("/nearestplanes.json?{PAIR}"));
    assert_eq!(body, "[]");
    let now = Utc::now();
    a.ctx
        .cache
        .insert_or_update_if_newer(&plane(now, "ABC001", 50.02, 11.04, 0.0));
    let body = get(&a, &format!("/nearestplanes.json?{PAIR}"));
    assert!(body.starts_with("[\n  {\n    \"Track\":"), "{body}");
}

#[test]
fn nearest_planes_uses_band_param() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let now = Utc::now();
    let mid = latlon::midpoint(TE1ST.0, TE1ST.1, TE2ND.0, TE2ND.1);
    // 30 km north of the path, flying away: between the 1.2G (10 km) and 144M (50 km) limits
    let p = latlon::destination_point(mid.lat, mid.lon, 0.0, 30.0);
    a.ctx
        .cache
        .insert_or_update_if_newer(&plane(now, "ABC002", p.lat, p.lon, 0.0));
    let wide = nearest(&a, &format!("{PAIR}&band=144M"), now);
    assert!(wide[0]["IntPoint"].is_object(), "{wide}");
    // no band: the settings band 1.2G
    let narrow = nearest(&a, PAIR, now);
    assert_eq!(narrow[0]["Potential"], 0, "{narrow}");
    assert!(narrow[0]["IntPoint"].is_object(), "{narrow}");
    let narrow = nearest(&a, &format!("{PAIR}&band=1.2G"), now);
    assert_eq!(narrow[0]["Potential"], 0, "{narrow}");
}

#[test]
fn nearest_planes_bad_band_text() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    assert_eq!(
        get(&a, &format!("/nearestplanes.json?{PAIR}&band=7M")),
        "Error: 7M is not a valid band value!"
    );
}

const ALL_PATH_ROUTES: [&str; 3] = [
    "/elevationpath.json",
    "/propagationpath.json",
    "/nearestplanes.json",
];

#[test]
fn no_elevation_model_answers() {
    let dir = common::data_dir_with_globe();
    let mut s = settings();
    s.elevation_enabled.globe = false;
    let a = api(dir.path(), s, true);
    for r in ALL_PATH_ROUTES {
        assert_eq!(
            get(&a, &format!("{r}?{PAIR}")),
            "Error: no elevation model enabled!",
            "{r}"
        );
    }
}

#[test]
fn missing_band_settings_answers() {
    let dir = common::data_dir_with_globe();
    let mut s = settings();
    s.band_settings.remove(&Band::B1_2G);
    let a = api(dir.path(), s, false);
    let count = a.ctx.paths.stations.location_count().unwrap();
    for r in &ALL_PATH_ROUTES[1..] {
        assert_eq!(
            get(&a, &format!("{r}?{PAIR}&dxlat=50.03&dxlon=12.05")),
            "Error: no band settings for 1.2G!",
            "{r}"
        );
    }
    assert_eq!(
        get(&a, &format!("/nearestplanes.json?{PAIR}&band=1.2G")),
        "Error: no band settings for 1.2G!"
    );
    // answered before any location is stored
    assert_eq!(a.ctx.paths.stations.location_count().unwrap(), count);
    // the elevation path reads no band settings
    assert!(get(
        &a,
        &format!("/elevationpath.json?{PAIR}&dxlat=50.03&dxlon=12.05")
    )
    .starts_with('{'));
}

#[test]
fn missing_dx_location_text() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), false);
    for r in ALL_PATH_ROUTES {
        assert_eq!(
            get(&a, &format!("{r}?{PAIR}")),
            "Error: DXLocation not found in database!",
            "{r}"
        );
        assert_eq!(
            get(&a, &format!("{r}?mycall=TE2ND&dxcall=TE1ST")),
            "Error: MyLocation not found in database!",
            "{r}"
        );
    }
}

#[test]
fn bad_latitude_text() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    for r in &ALL_PATH_ROUTES[..2] {
        let get = |q: &str| get(&a, &format!("{r}?{PAIR}&{q}"));
        assert_eq!(get("mylat=abc"), "Error: ABC is not a valid latitude!");
        assert_eq!(get("dxlon=1x"), "Error: 1X is not a valid longitude!");
    }
}

#[test]
fn checks_run_in_csharp_order() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    for r in &ALL_PATH_ROUTES[..2] {
        let get = |q: &str| get(&a, &format!("{r}?{q}"));
        assert_eq!(
            get("mycall=1&dxcall=2&mylat=x"),
            "Error: 1 is not a valid callsign!"
        );
        assert_eq!(
            get("mycall=TE1ST&dxcall=2&mylat=x"),
            "Error: 2 is not a valid callsign!"
        );
        // per station: lat, lon, loc of My before those of DX (Webserver.cs:668-683)
        assert_eq!(
            get(&format!("{PAIR}&dxlat=x&myloc=ZZ99")),
            "Error: ZZ99 is not a valid Maidenhead locator!"
        );
        assert_eq!(
            get(&format!("{PAIR}&mylon=y&mylat=x")),
            "Error: X is not a valid latitude!"
        );
        assert_eq!(
            get(&format!("{PAIR}&dxloc=ZZ99&dxlon=y")),
            "Error: Y is not a valid longitude!"
        );
    }
    let get = |q: &str| get(&a, &format!("/nearestplanes.json?{q}"));
    assert_eq!(
        get(&format!("{PAIR}&band=7M&dxloc=ZZ99")),
        "Error: ZZ99 is not a valid Maidenhead locator!"
    );
}

/// The stored elevation and propagation paths of the GLOBE model.
fn stored_paths(a: &WebApi) -> (i64, i64) {
    let m = ElevationModel::Globe;
    let p = &a.ctx.paths;
    let elv = p.store.with_conn(m, |c| {
        Ok(c.query_row("SELECT COUNT(*) FROM ElevationPath", [], |r| {
            r.get::<_, i64>(0)
        }))
    });
    let prop = p.engine.with_conn(m, |c| {
        c.query_row("SELECT COUNT(*) FROM PropagationPath", [], |r| {
            r.get::<_, i64>(0)
        })
    });
    (elv.unwrap().unwrap(), prop.unwrap().unwrap())
}

/// Both ends inside the GLOBE fixture, so a computed path is valid and saved.
const INSIDE: &str = "mylat=50.2&mylon=10.3&dxlat=50.6&dxlon=11.7";

#[test]
fn stop_cancels_path_calculation() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    // the stored GLOBE data counts as complete for JO50, so a computed path is saved
    let store = &a.ctx.paths.store;
    store.set_status_bit(ElevationModel::Globe, as_elevation::DbStatus::COMPLETE);
    store.set_bounds(as_geo::latlon::GRect::new(50.0, 10.0, 51.0, 12.0));
    a.ctx.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    // a request after stop answers before the route runs
    assert_eq!(
        get(&a, &format!("/elevationpath.json?{PAIR}&{INSIDE}")),
        "Error: web server stopped"
    );
    // a calculation that sees the stop flag is cancelled
    let s = settings();
    let q = Query::from_raw_url(&format!("/x?{PAIR}&{INSIDE}"));
    for text in [
        elevation_path(&a.ctx, &s, &q),
        propagation_path(&a.ctx, &s, &q),
    ] {
        assert_eq!(text, "Error: path calculation cancelled!");
    }
    let q = Query::from_raw_url(&format!("/x?{PAIR}"));
    assert_eq!(
        nearest_planes(&a.ctx, &s, &q, Utc::now()),
        "Error: path calculation cancelled!"
    );
    assert_eq!(stored_paths(&a), (0, 0));
    // the same requests without the stop flag compute and save their paths
    a.ctx.stop.store(false, std::sync::atomic::Ordering::SeqCst);
    for r in &ALL_PATH_ROUTES[..2] {
        let v = json(&get(&a, &format!("{r}?{PAIR}&{INSIDE}")));
        assert_eq!(v["Valid"], true, "{r}: {v}");
    }
    let (elv, prop) = stored_paths(&a);
    assert!(elv >= 1 && prop == 1, "{elv} {prop}");
}

/// Coordinates a LAN client may send. Each answers; none panics or hangs.
#[test]
fn hostile_coordinates_answer() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let no_loc = "Error: no Maidenhead locator for";
    for r in &ALL_PATH_ROUTES[..2] {
        let get = |q: &str| get(&a, &format!("{r}?{PAIR}&{q}"));
        assert_eq!(
            get("mylat=NaN&mylon=10"),
            "Error: NAN is not a valid latitude!"
        );
        assert_eq!(
            get("mylat=10&mylon=-Infinity"),
            "Error: -INFINITY is not a valid longitude!"
        );
        assert_eq!(
            get("mylat=1e999&mylon=10"),
            "Error: 1E999 is not a valid latitude!"
        );
        for q in [
            "mylat=1e308&mylon=10",
            "mylat=95&mylon=10",
            "mylat=-90.5&mylon=10",
            "mylat=50&mylon=181",
            "dxlat=50&dxlon=-180.5",
            "dxlat=1e-320&dxlon=1e308",
        ] {
            assert!(get(q).starts_with(no_loc), "{r}?{q}: {}", get(q));
        }
        // equal endpoints: a path of one sample
        let v = json(&get("mylat=50.2&mylon=10.3&dxlat=50.2&dxlon=10.3"));
        assert_eq!(v["Distance"].as_f64(), Some(0.0), "{r}: {v}");
    }
    let v = json(&get(
        &a,
        "/elevationpath.json?mycall=TE1ST&dxcall=TE2ND&mylat=50.2&mylon=10.3&dxlat=50.2&dxlon=10.3",
    ));
    assert_eq!(v["Count"], 1);
    // nearest planes reads no coordinates; the stored positions are used
    assert_eq!(
        get(
            &a,
            &format!("/nearestplanes.json?{PAIR}&mylat=NaN&dxlat=1e999")
        ),
        "[]"
    );
}

#[test]
fn far_apart_points_answer() {
    let dir = common::data_dir_with_globe();
    let a = api(dir.path(), settings(), true);
    let started = std::time::Instant::now();
    for r in &ALL_PATH_ROUTES[..2] {
        // About 11 000 km, with most GLOBE samples outside the fixture.
        let far = "mylat=50.2&mylon=10.3&dxlat=-40&dxlon=100";
        let v = json(&get(&a, &format!("{r}?{PAIR}&{far}")));
        assert!(v["Distance"].as_f64().unwrap() > 10_000.0, "{r}");
        assert_eq!(v["Valid"], false, "{r}");
        // #15: near-antipodal samples cross 180° and the route returns a profile.
        let far = "mylat=50.2&mylon=10.3&dxlat=-50.1&dxlon=-169.8";
        let v = json(&get(&a, &format!("{r}?{PAIR}&{far}")));
        assert!(v["Distance"].as_f64().unwrap() > 19_000.0, "{r}");
        assert_eq!(
            v["Valid"], false,
            "{r}: the fixture only covers a small area"
        );
    }
    // a few seconds on the fixture; generous for slow CI runners
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn nearest_planes_equal_and_far_stations_answer() {
    for dx in [TE1ST, (-50.1, -169.8)] {
        let dir = common::data_dir_with_globe();
        let a = api(dir.path(), settings(), false);
        let loc = maidenhead::loc_from_lat_lon(dx.0, dx.1, false, 3, false)
            .unwrap()
            .unwrap();
        store(&a.ctx.paths, "TE2ND", &loc, dx);
        let now = Utc::now();
        a.ctx
            .cache
            .insert_or_update_if_newer(&plane(now, "ABC003", 50.02, 10.05, 0.0));
        let v = nearest(&a, PAIR, now);
        assert!(v.is_array(), "{dx:?}: {v}");
    }
}
