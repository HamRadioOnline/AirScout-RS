//! Validation and recovery pipeline (`PlaneFeed.cs` worker loop body).
use as_aircraft::db::{Aircraft, AircraftDb, AircraftRegistration, AircraftType, Airline};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_feeds::pipeline::{process, PipelineConfig, Reject};
use as_feeds::{FeedArgs, FeedPlane};
use chrono::{DateTime, Duration, TimeZone, Utc};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

fn db() -> AircraftDb {
    let db = AircraftDb::open_in_memory().unwrap();
    let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    db.airline_insert_or_update_if_newer(&Airline {
        icao: "DLH".into(),
        iata: "LH".into(),
        airline: "Lufthansa".into(),
        country: "Germany".into(),
        last_updated: old,
    })
    .unwrap();
    db.registration_insert_or_update_if_newer(&AircraftRegistration {
        prefix: "D".into(),
        country: "Germany".into(),
        remarks: String::new(),
        last_updated: old,
    })
    .unwrap();
    db.aircraft_insert_or_update_if_newer(&Aircraft {
        hex: "3C0001".into(),
        call: "DLH459".into(),
        reg: "D-AIMA".into(),
        type_code: "A388".into(),
        last_updated: old,
    })
    .unwrap();
    db.aircraft_type_insert_or_update_if_newer(&AircraftType {
        icao: "A388".into(),
        iata: "388".into(),
        manufacturer: "AIRBUS".into(),
        model: "AIRBUS A-380-800".into(),
        category: PlaneCategory::Superheavy,
        last_updated: old,
    })
    .unwrap();
    db
}

fn args() -> FeedArgs {
    FeedArgs {
        min_lat: 47.5,
        max_lat: 54.25,
        min_lon: 4.0,
        max_lon: 16.5,
        min_alt_m: 5000,
        max_alt_m: 12200,
        my_lat: 50.937067,
        my_lon: 10.68327,
        ..FeedArgs::default()
    }
}

fn cfg() -> PipelineConfig {
    PipelineConfig {
        extended_plausibility: true,
        max_error_dist: 30.0,
        log_errors: false,
    }
}

fn fp(hex: &str, lat: f64, lon: f64) -> FeedPlane {
    FeedPlane {
        time: Some(t0()),
        hex: hex.into(),
        call: "DLH459".into(),
        reg: "D-AIMA".into(),
        lat,
        lon,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 450.0,
        type_code: "A388".into(),
        manufacturer: "AIRBUS".into(),
        model: "A380".into(),
        ..FeedPlane::default()
    }
}

fn run(raw: Vec<FeedPlane>, cache: &PlaneInfoCache) -> as_feeds::pipeline::Processed {
    process(raw, &args(), &cfg(), cache, &db())
}

#[test]
fn ground_plane_skipped() {
    let mut p = fp("3C0001", 50.0, 8.0);
    p.ground = true;
    let r = run(vec![p], &PlaneInfoCache::new());
    assert!(r.planes.is_empty());
    assert!(r.rejected.is_empty());
}

#[test]
fn valid_plane_passes_and_model_prefix_stripped() {
    let r = run(vec![fp("3C0001", 50.0, 8.0)], &PlaneInfoCache::new());
    assert_eq!(r.planes.len(), 1);
    let p = &r.planes[0];
    assert_eq!(p.hex, "3C0001");
    assert_eq!(p.call, "DLH459");
    assert_eq!(p.alt_ft, 35000.0);
    assert_eq!(p.time, t0());
    // manufacturer/model came from the feed; no prefix to strip
    assert_eq!(p.model, "A380");
}

#[test]
fn hex_invalid_recovered_from_reg() {
    let r = run(vec![fp("", 50.0, 8.0)], &PlaneInfoCache::new());
    assert_eq!(r.planes.len(), 1);
    assert_eq!(r.planes[0].hex, "3C0001");

    let mut bad = fp("XYZ", 50.0, 8.0);
    bad.reg = "".into();
    let mut unknown_reg = fp("XYZ", 50.0, 8.0);
    unknown_reg.reg = "D-ZZZZ".into();
    let r = run(vec![bad, unknown_reg], &PlaneInfoCache::new());
    assert!(r.planes.is_empty());
    let why: Vec<Reject> = r.rejected.iter().map(|(_, w)| *w).collect();
    assert_eq!(why, vec![Reject::Hex, Reject::Reg]);
}

#[test]
fn outside_bbox_skipped_silently() {
    let r = run(
        vec![
            fp("3C0001", 47.49, 8.0),
            fp("3C0001", 54.26, 8.0),
            fp("3C0001", 50.0, 3.99),
            fp("3C0001", 50.0, 16.51),
        ],
        &PlaneInfoCache::new(),
    );
    assert!(r.planes.is_empty());
    assert!(r.rejected.is_empty());
}

#[test]
fn on_bbox_border_kept() {
    let r = run(
        vec![fp("3C0001", 47.5, 4.0), fp("3C0002", 54.25, 16.5)],
        &PlaneInfoCache::new(),
    );
    assert_eq!(r.planes.len(), 2);
}

#[test]
fn invalid_lat_is_an_error_not_a_skip() {
    let r = run(vec![fp("3C0001", 91.0, 8.0)], &PlaneInfoCache::new());
    assert_eq!(r.rejected[0].1, Reject::Lat);
}

#[test]
fn alt_window_edges() {
    let mk = |alt: f64| {
        let mut p = fp("3C0001", 50.0, 8.0);
        p.alt_ft = alt;
        p
    };
    // 16404.2 ft = 5000 m and 40026.248 ft = 12200 m exactly in f64
    let cases = [
        (16404.2, true),
        (16404.1, false),
        (40026.248, true),
        (40026.3, false),
    ];
    for (alt, keep) in cases {
        let r = run(vec![mk(alt)], &PlaneInfoCache::new());
        assert_eq!(r.planes.len() == 1, keep, "{alt}");
        assert!(r.rejected.is_empty());
    }
    // invalid altitude without cache: error
    let r = run(vec![mk(f64::MIN)], &PlaneInfoCache::new());
    assert_eq!(r.rejected[0].1, Reject::Alt);
}

#[test]
fn alt_and_speed_recovered_from_cache() {
    let cache = PlaneInfoCache::new();
    let mut prev = PlaneInfo::new(
        t0() - Duration::seconds(10),
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        7.98,
        90.0,
        36000.0,
        440.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    );
    prev.time = t0() - Duration::seconds(10);
    cache.insert_or_update_if_newer(&prev);
    let mut p = fp("3C0001", 50.0, 8.0);
    p.alt_ft = f64::MIN;
    p.speed_kts = f64::MIN;
    let r = process(
        vec![p],
        &args(),
        &PipelineConfig {
            extended_plausibility: false,
            ..cfg()
        },
        &cache,
        &db(),
    );
    assert_eq!(r.planes[0].alt_ft, 36000.0);
    assert_eq!(r.planes[0].speed_kts, 440.0);
}

#[test]
fn missing_call_from_db_then_unknown() {
    let mut a = fp("3C0001", 50.0, 8.0);
    a.call = String::new();
    let mut b = fp("3C0002", 50.0, 8.0);
    b.call = String::new();
    b.reg = String::new();
    b.type_code = String::new();
    let r = run(vec![a, b], &PlaneInfoCache::new());
    assert_eq!(r.planes[0].call, "DLH459");
    assert_eq!(r.planes[1].call, "[unknown]");
    assert_eq!(r.planes[1].reg, "[unknown]");
    assert_eq!(r.planes[1].type_code, "[unknown]");
    assert_eq!(r.planes[1].manufacturer, "[unknown]");
    assert_eq!(r.planes[1].category, PlaneCategory::None);
}

#[test]
fn type_info_from_db_and_unkonwn_typo() {
    // manufacturer "[unknown]" does not trigger the DB lookup (C# compares with "[unkonwn]")
    let mut a = fp("3C0001", 50.0, 8.0);
    a.manufacturer = "[unknown]".into();
    a.model = "A380".into();
    // empty model triggers it; the manufacturer prefix is then stripped from the model
    let mut b = fp("3C0001", 50.0, 8.0);
    b.model = String::new();
    let r = run(vec![a], &PlaneInfoCache::new());
    assert_eq!(r.planes[0].manufacturer, "[unknown]");
    assert_eq!(r.planes[0].model, "A380");
    let r = run(vec![b], &PlaneInfoCache::new());
    assert_eq!(r.planes[0].manufacturer, "AIRBUS");
    assert_eq!(r.planes[0].model, "A-380-800");
    assert_eq!(r.planes[0].category, PlaneCategory::Superheavy);
}

fn cached_prev() -> PlaneInfoCache {
    let cache = PlaneInfoCache::new();
    cache.insert_or_update_if_newer(&PlaneInfo::new(
        t0(),
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    ));
    cache
}

#[test]
fn plausibility_rejects_jump() {
    let mut p = fp("3C0001", 52.0, 8.0);
    p.time = Some(t0() + Duration::seconds(60));
    p.track = 0.0;
    let r = run(vec![p], &cached_prev());
    assert!(r.planes.is_empty());
    assert_eq!(r.rejected[0].1, Reject::PlausibilityDist);
}

#[test]
fn plausibility_track_fold_reproduced() {
    // moving east (bearing ~90°) but reporting 270°: the fold maps both to 90° (#47)
    let mut p = fp("3C0001", 50.0, 8.2);
    p.time = Some(t0() + Duration::seconds(60));
    p.track = 270.0;
    let r = run(vec![p.clone()], &cached_prev());
    assert_eq!(r.planes.len(), 1);
    assert_eq!(r.planes[0].track, 270.0);
    // 200° folds to 160°: 70° off, rejected and the track replaced by the bearing
    p.track = 200.0;
    let r = run(vec![p], &cached_prev());
    assert_eq!(r.rejected[0].1, Reject::PlausibilityTrack);
}

#[test]
fn plausibility_off_accepts_jump() {
    let mut p = fp("3C0001", 52.0, 8.0);
    p.time = Some(t0() + Duration::seconds(60));
    let r = process(
        vec![p],
        &args(),
        &PipelineConfig {
            extended_plausibility: false,
            ..cfg()
        },
        &cached_prev(),
        &db(),
    );
    assert_eq!(r.planes.len(), 1);
}
