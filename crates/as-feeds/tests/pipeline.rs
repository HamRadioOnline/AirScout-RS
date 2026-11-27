//! Validation and recovery pipeline (`PlaneFeed.cs` worker loop body).
use as_aircraft::db::{Aircraft, AircraftDb, AircraftRegistration, AircraftType, Airline};
use as_aircraft::{AltitudeProvenance, PlaneCategory, PlaneInfo, PlaneInfoCache};
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
        my_lat: 50.020833,
        my_lon: 10.041667,
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
fn pipeline_preserves_geometric_altitude_provenance() {
    let mut p = fp("3C0001", 50.0, 8.0);
    p.altitude_provenance = AltitudeProvenance::Geometric;
    let r = run(vec![p], &PlaneInfoCache::new());
    assert_eq!(r.planes.len(), 1);
    assert_eq!(
        r.planes[0].altitude_provenance,
        AltitudeProvenance::Geometric
    );
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
    prev.altitude_provenance = AltitudeProvenance::PressureEstimate;
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
    assert_eq!(
        r.planes[0].altitude_provenance,
        AltitudeProvenance::PressureEstimate
    );
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
fn type_fields_from_db_when_model_is_empty() {
    let mut observation = fp("3C0001", 50.0, 8.0);
    observation.model.clear();
    let mapped = run(vec![observation], &PlaneInfoCache::new());
    assert_eq!(mapped.planes[0].manufacturer, "AIRBUS");
    assert_eq!(mapped.planes[0].model, "A-380-800");
    assert_eq!(mapped.planes[0].category, PlaneCategory::Superheavy);
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
fn plausibility_track_uses_shortest_separation() {
    // moving east (bearing ~90°); reporting the opposite 270° is a 180° separation and fails
    // (#47: the old fold mapped both to 90° and wrongly accepted it)
    let mut p = fp("3C0001", 50.0, 8.2);
    p.time = Some(t0() + Duration::seconds(60));
    p.track = 270.0;
    let r = run(vec![p.clone()], &cached_prev());
    assert!(r.planes.is_empty());
    assert_eq!(r.rejected[0].1, Reject::PlausibilityTrack);

    // a turn across north (bearing ~350°, reported 10°) is a 20° separation and passes
    let mut north = fp("3C0001", 50.1, 7.97);
    north.time = Some(t0() + Duration::seconds(60));
    north.track = 10.0;
    let r = run(vec![north.clone()], &cached_prev());
    assert_eq!(r.planes.len(), 1);
    assert_eq!(r.planes[0].track, 10.0);

    // a ~40° separation stays within the 45° tolerance and passes (the exact 45° boundary is
    // covered by the heading_separation unit test below)
    let mut diagonal = fp("3C0001", 50.0, 8.2);
    diagonal.time = Some(t0() + Duration::seconds(60));
    diagonal.track = 130.0;
    let r = run(vec![diagonal], &cached_prev());
    assert_eq!(r.planes.len(), 1);
    assert_eq!(r.planes[0].track, 130.0);
}

#[test]
fn plausibility_track_ignores_zero_displacement_updates() {
    // A south-bound aircraft whose position quantizes to the same coordinates one second later
    // has a zero-displacement bearing of 0°, which must not be compared against its 180° track
    // (#47: `atan2(0, 0) == 0` would otherwise reject it as a 180° separation).
    let cache = PlaneInfoCache::new();
    cache.insert_or_update_if_newer(&PlaneInfo::new(
        t0(),
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        180.0,
        35000.0,
        450.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    ));
    let mut p = fp("3C0001", 50.0, 8.0);
    p.time = Some(t0() + Duration::seconds(1));
    p.track = 180.0;
    let r = run(vec![p], &cache);
    assert_eq!(r.planes.len(), 1);
    assert_eq!(r.planes[0].track, 180.0);
}

#[test]
fn plausibility_distance_uses_full_positive_elapsed_time() {
    // 30 s forward at 30 km/min grants a 15 km budget, measured from the dead-reckoned position
    // (the cached plane at 450 kts advances ~7 km east in 30 s). A step well inside the budget
    // passes; one well beyond it fails (#50).
    let cache = cached_prev();
    let mut ok = fp("3C0001", 50.0, 8.15);
    ok.time = Some(t0() + Duration::seconds(30));
    let r = run(vec![ok.clone()], &cache);
    assert_eq!(r.planes.len(), 1);

    let mut far = fp("3C0001", 50.0, 8.5);
    far.time = Some(t0() + Duration::seconds(30));
    let r = run(vec![far], &cache);
    assert!(r.planes.is_empty());
    assert_eq!(r.rejected[0].1, Reject::PlausibilityDist);
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

#[test]
fn finding_366_component_expiry_survives_feed_enrichment_and_cache_update() {
    let cache = PlaneInfoCache::new();
    let mut previous = fp("3C0001", 50.0, 8.0);
    previous.time = Some(t0() - Duration::seconds(5));
    let previous = run(vec![previous], &cache).planes.remove(0);
    cache.insert_or_update_if_newer(&previous);

    // A fresh position arrived fifty seconds after the motion measurement.
    let mut report = fp("3C0001", 50.01, 8.0);
    report.components_valid_until = Some(t0() + Duration::seconds(10));
    let config = PipelineConfig {
        extended_plausibility: false,
        ..cfg()
    };
    let position = process(vec![report], &args(), &config, &cache, &db())
        .planes
        .remove(0);
    assert!(cache.insert_or_update_if_newer(&position));

    let boundary = t0() + Duration::seconds(10);
    assert!(cache.get("3C0001", boundary, 5).is_some());
    assert_eq!(cache.get_all(boundary, 5).len(), 1);
    let expired = boundary + Duration::nanoseconds(1);
    assert!(cache.get("3C0001", expired, 5).is_none());
    assert!(cache.get_all(expired, 5).is_empty());
}

#[test]
fn finding_366_component_reports_do_not_borrow_cached_motion_or_height() {
    // A cached value has its own (older) observation time; pairing it with the fresh
    // report's component deadline would keep stale motion or height eligible.
    let cache = PlaneInfoCache::new();
    let mut prev = fp("3C0001", 50.0, 7.98);
    prev.time = Some(t0() - Duration::seconds(10));
    let prev = run(vec![prev], &cache).planes.remove(0);
    cache.insert_or_update_if_newer(&prev);
    let config = PipelineConfig {
        extended_plausibility: false,
        ..cfg()
    };

    let mut fast = fp("3C0001", 50.0, 8.0);
    fast.speed_kts = 900.0;
    fast.components_valid_until = Some(t0() + Duration::seconds(10));
    let r = process(vec![fast], &args(), &config, &cache, &db());
    assert!(r.planes.is_empty());
    assert_eq!(r.rejected[0].1, Reject::Speed);

    let mut no_alt = fp("3C0001", 50.0, 8.0);
    no_alt.alt_ft = f64::MIN;
    no_alt.components_valid_until = Some(t0() + Duration::seconds(10));
    let r = process(vec![no_alt], &args(), &config, &cache, &db());
    assert!(r.planes.is_empty());
    assert_eq!(r.rejected[0].1, Reject::Alt);
}

#[test]
fn finding_366_geometric_policy_removes_only_prior_component_pressure_reports() {
    let cache = PlaneInfoCache::new();
    let mut pressure = fp("3C0001", 50.0, 8.0);
    pressure.altitude_provenance = AltitudeProvenance::PressureEstimate;
    pressure.components_valid_until = Some(t0() + Duration::minutes(5));
    let mut direct = pressure.clone();
    direct.hex = "3C0002".into();
    direct.altitude_provenance = AltitudeProvenance::Geometric;
    let mut other_source = pressure.clone();
    other_source.hex = "3C0003".into();
    other_source.components_valid_until = None;
    cache.bulk_insert_or_update_if_newer(
        &run(vec![pressure.clone(), direct, other_source], &cache).planes,
    );

    cache.set_component_feed_geometric_only(true);
    let hexes: Vec<_> = cache.get_all(t0(), 5).into_iter().map(|p| p.hex).collect();
    assert_eq!(hexes, ["3C0002", "3C0003"]);
    let late_pressure = run(vec![pressure], &cache).planes.remove(0);
    assert!(!cache.insert_or_update_if_newer(&late_pressure));
}

#[test]
fn finding_48_recovers_manufacturer_for_canonical_unknown_marker() {
    let mut plane = fp("3C0001", 50.0, 8.0);
    plane.manufacturer = "[unknown]".into();
    let recovered = run(vec![plane], &PlaneInfoCache::new());
    assert_eq!(recovered.planes[0].manufacturer, "AIRBUS");
    assert_eq!(recovered.planes[0].model, "A-380-800");
}

#[test]
fn finding_51_wire_categories_survive_enrichment_and_yield_to_canonical_a388() {
    let db = db();
    db.airline_insert_or_update_if_newer(&Airline {
        icao: "TST".into(),
        iata: "TE".into(),
        airline: "Synthetic airline".into(),
        country: String::new(),
        last_updated: t0(),
    })
    .unwrap();
    for parse in [
        as_feeds::feeds::readsb::parse as fn(&str) -> Result<Vec<FeedPlane>, _>,
        as_feeds::feeds::dump1090::parse,
    ] {
        for (emitter, category) in [
            ("A1", 1),
            ("A2", 1),
            ("A3", 2),
            ("A4", 2),
            ("A5", 3),
            ("A7", 0),
        ] {
            let json = format!(
                r#"{{"now":100,"aircraft":[
                {{"hex":"3C0002","category":"{emitter}","flight":" TE1ST ","seen_pos":0,"lat":50,"lon":8,
                  "gs":450,"speed":450,"track":90,"alt_baro":35000,"altitude":35000}},
                {{"hex":"3C0001","category":"{emitter}","seen_pos":0,"lat":50,"lon":8,
                  "gs":450,"speed":450,"track":90,"alt_baro":35000,"altitude":35000}}
            ]}}"#
            );
            let raw = parse(&json).unwrap();
            assert_eq!(raw[0].category, category);
            let result = process(raw, &args(), &cfg(), &PlaneInfoCache::new(), &db);
            assert_eq!(result.planes.len(), 2);
            let unseen = result
                .planes
                .iter()
                .find(|plane| plane.hex == "3C0002")
                .unwrap();
            assert_eq!(unseen.category as i32, category);
            assert_eq!(unseen.call, "TE1ST");
            let known = result
                .planes
                .iter()
                .find(|plane| plane.hex == "3C0001")
                .unwrap();
            assert_eq!(known.category, PlaneCategory::Superheavy);
        }
    }
}
