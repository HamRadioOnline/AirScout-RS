//! Fixtures shared by the crossing history tests.
#![allow(dead_code)]
use airscout::crossing_history::CrossingParams;
use as_aircraft::{PlaneCategory, PlaneInfo};
use as_propagation::{PathParams, PropagationPath};
use chrono::{DateTime, Duration, TimeZone, Utc};

pub fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()
}

/// 100 km east-west path at 50.0 N.
pub fn path() -> PropagationPath {
    PropagationPath::new(
        PathParams {
            lat1: 50.0,
            lon1: 8.0,
            h1: 300.0,
            lat2: 50.0,
            lon2: 9.0,
            h2: 300.0,
            qrg_ghz: 10.368,
            radius_km: 6371.0 * 4.0 / 3.0,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        -0.002,
        -0.002,
        true,
    )
}

pub fn params() -> CrossingParams {
    CrossingParams {
        from: t0(),
        to: t0() + Duration::minutes(30),
        step_s: 60,
        ttl_min: 5,
        max_radius_km: 0.0,
        max_distance_km: 30.0,
        max_alt_m: 12200.0,
        max_delta_m: 1000.0,
        with_signal_level: false,
    }
}

pub fn report(hex: &str, secs: i64, lat: f64, lon: f64) -> PlaneInfo {
    PlaneInfo::new(
        t0() + Duration::seconds(secs),
        "TST123",
        "",
        hex,
        lat,
        lon,
        1.0,
        32808.0,
        450.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

/// A plane flying north over the middle of the path: a report every 10 s, from 49.6 N.
pub fn northbound(hex: &str) -> Vec<PlaneInfo> {
    (0..=60)
        .map(|k| {
            let t = k * 10;
            report(hex, t, 49.6 + t as f64 * 0.002082, 8.5)
        })
        .collect()
}

/// The plane plus a parked decoy 33 km off the path (outside the 30 km limit) that reports 5 s
/// after every plane report, from the first step on (the plane's first pre-selected report is at
/// 40 s: the steps are 40 s + n * 60 s, on the plane's reports). The algorithm stops each cache fill after the first report newer
/// than the step; with aliasing that report overwrites the first object of its hex and the hex
/// is gone from the step. The decoy takes the overshoot, so the plane stays.
pub fn history() -> Vec<PlaneInfo> {
    let mut v = northbound("abc123");
    v.extend((6..60).map(|k| report("def456", k * 10 + 5, 50.3, 8.5)));
    v.sort_by_key(|p| p.time);
    v
}

pub fn no_signal(_: DateTime<Utc>) -> Option<f64> {
    None
}

pub fn crossing(hex: &str, secs: i64) -> PlaneInfo {
    report(hex, secs, 50.0, 8.5)
}

pub fn flags(c: &[PlaneInfo]) -> Vec<bool> {
    c.iter().map(|p| p.ambiguous).collect()
}

/// Data dir with the GLOBE tiles of square JO50 (lon 10..12, lat 50..51) from the fixtures.
pub fn data_dir_with_globe() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let zip = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/elevation/GLOBE/JO/JO50.zip"),
    )
    .unwrap();
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let store = as_elevation::ElevationStore::open(dir.path()).unwrap();
    as_elevation::update_square(
        &reqwest::blocking::Client::new(),
        &server.url(),
        as_elevation::ElevationModel::Globe,
        "JO50",
        &store,
        None,
        &mut |_| {},
    )
    .unwrap();
    dir
}
