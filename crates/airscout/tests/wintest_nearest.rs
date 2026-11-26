//! ASSETPATH answer: request parsing, path, nearest planes, ASNEAREST data and reply.
use airscout::services::open_data;
use airscout::wintest_nearest::{
    answer, nearest_data, parse_set_path, send_reply, set_path_path, NearestSettings,
    SetPathRequest,
};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_geo::band::Band;
use as_geo::latlon::{GPoint, EARTH_RADIUS_KM};
use as_settings::{default_band_table, Settings};
use as_wintest::encode;
use as_wintest::track::as_nearest;
use chrono::{DateTime, TimeZone, Utc};
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

mod common;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap()
}

fn req(band: Band) -> SetPathRequest {
    SetPathRequest {
        src: "KST".into(),
        band,
        my_call: "TE1ST".into(),
        my_loc: "JO50AA".into(),
        dx_call: "TE2ND".into(),
        dx_loc: "JO60AA".into(),
    }
}

fn settings() -> Settings {
    Settings {
        path_best_case_elevation: false,
        ..Settings::default()
    }
}

#[test]
fn parse_set_path_rules() {
    let p = |d: &str| parse_set_path("KST", d);
    let r = p("1440000,TE1ST,JO50AA,TE2ND,JO60AA").unwrap();
    assert_eq!(r.band, Band::B144M);
    assert_eq!(r.src, "KST");
    assert_eq!(r.my_call, "TE1ST");
    assert_eq!(r.dx_loc, "JO60AA");
    assert_eq!(
        p("12960000,TE1ST,JO50AA,TE2ND,JO60AA").unwrap().band,
        Band::B1_2G
    );
    assert_eq!(
        p("500000,TE1ST,JO50AA,TE2ND,JO60AA").unwrap().band,
        Band::B50M
    );
    assert!(p("28000,TE1ST,JO50AA,TE2ND,JO60AA").is_none());
    assert!(p("x,TE1ST,JO50AA,TE2ND,JO60AA").is_none());
    assert!(p("1440001x,TE1ST,JO50AA,TE2ND,JO60AA").is_none());
    assert!(p("1440000,TE1ST,JO50,TE2ND,JO60AA").is_none());
    assert_eq!(
        p("1440000,TE1ST,JO50AA12,TE2ND,JO60AA").unwrap().my_loc,
        "JO50AA"
    );
    assert!(p("1440000,TE1ST,JO50AA,TE2ND").is_none());
    assert!(p("1440000,1,JO50AA,TE2ND,JO60AA").is_none());
    // quotes are removed from every field
    assert_eq!(
        p("\"1440000\",\"TE1ST\",\"JO50AA\",\"TE2ND\",\"JO60AA\"")
            .unwrap()
            .my_call,
        "TE1ST"
    );
}

#[test]
fn nearest_data_without_planes() {
    assert_eq!(
        nearest_data(now(), &req(Band::B144M), &[]),
        "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,0"
    );
}

fn plane(call: &str, cat: PlaneCategory, qrb: f64, pot: i32, kts: f64, cross: bool) -> PlaneInfo {
    let mut p = PlaneInfo::new(
        now(),
        call,
        "",
        "ABC123",
        50.5,
        11.0,
        1.0,
        32808.0,
        kts,
        "A320",
        "Airbus",
        "A320",
        cat,
    );
    p.int_point = cross.then(|| GPoint::new(50.5, 11.0));
    p.int_qrb = qrb;
    p.potential = pot;
    p
}

#[test]
fn nearest_data_lists_crossing_planes_only() {
    let planes = [
        plane("TST123", PlaneCategory::Medium, 120.9, 85, 450.0, true),
        plane("TST456", PlaneCategory::Heavy, 50.2, 50, 0.0, true),
        plane("TST789", PlaneCategory::Light, 10.0, 0, 300.0, true),
        plane("TST000", PlaneCategory::Light, 10.0, 70, 300.0, false),
    ];
    assert_eq!(
        nearest_data(now(), &req(Band::B144M), &planes),
        "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,2,TST123,M,120,85,8,TST456,H,50,50,0"
    );
}

#[test]
fn answer_end_to_end_with_globe_tile() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let cache = PlaneInfoCache::new();
    let n = NearestSettings::from_settings(&settings());
    let cancel = AtomicBool::new(false);
    let out = answer(&paths, &cache, &n, &req(Band::B144M), now(), &cancel)
        .unwrap()
        .expect("answered");
    assert!(out.ends_with(",0"), "{out}");
    assert!(out.starts_with("2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,"));
    assert_eq!(paths.stations.location_count().unwrap(), 2);
    assert!(paths
        .stations
        .location_find("TE1ST", Some("JO50AA"))
        .unwrap()
        .is_some());
    assert!(paths
        .stations
        .location_find("TE2ND", Some("JO60AA"))
        .unwrap()
        .is_some());
}

#[test]
fn answer_path_uses_gui_band() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let mut s = settings();
    s.band = Band::B432M;
    let n = NearestSettings::from_settings(&s);
    let cancel = AtomicBool::new(false);
    let path = set_path_path(&paths, &n, &req(Band::B144M), &cancel)
        .unwrap()
        .expect("path");
    assert_eq!(path.params.qrg_ghz, 0.432);
    assert_eq!(
        path.params.radius_km,
        EARTH_RADIUS_KM * default_band_table()[&Band::B432M].k_factor
    );
}

#[test]
fn undefined_band_writes_nothing() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    // 28 MHz is no defined band: the request is dropped before any DB access
    assert!(parse_set_path("KST", "280000,TE1ST,JO50AA,TE2ND,JO60AA").is_none());
    assert!(parse_set_path("KST", "28000,TE1ST,JO50AA,TE2ND,JO60AA").is_none());
    // a band missing from the settings table is dropped silently by the full answer path
    let mut s = settings();
    s.band_settings.remove(&Band::B144M);
    let n = NearestSettings::from_settings(&s);
    let cancel = AtomicBool::new(false);
    let out = answer(
        &paths,
        &PlaneInfoCache::new(),
        &n,
        &req(Band::B144M),
        now(),
        &cancel,
    );
    assert!(matches!(out, Ok(None)), "{out:?}");
    assert_eq!(paths.stations.qrv_count().unwrap(), 0);
    assert_eq!(paths.stations.location_count().unwrap(), 0);
    // so is a missing GUI band
    let mut s = settings();
    s.band_settings.remove(&s.band.clone());
    let n = NearestSettings::from_settings(&s);
    let out = answer(
        &paths,
        &PlaneInfoCache::new(),
        &n,
        &req(Band::B144M),
        now(),
        &cancel,
    );
    assert!(matches!(out, Ok(None)), "{out:?}");
    assert_eq!(paths.stations.qrv_count().unwrap(), 0);
    assert_eq!(paths.stations.location_count().unwrap(), 0);
}

#[test]
fn reply_bytes_on_loopback() {
    let rx = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let to = rx.local_addr().unwrap();
    let d = "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,0".to_string();
    send_reply("AS", &req(Band::B144M), d.clone(), Ipv4Addr::LOCALHOST, to).unwrap();
    let mut buf = [0u8; 65535];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(
        &buf[..n],
        encode(&as_nearest("AS", "KST", d)).unwrap().as_slice()
    );
}
