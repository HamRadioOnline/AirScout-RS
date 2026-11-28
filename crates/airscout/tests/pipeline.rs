//! Tick pipeline tests (`MapDlg.UpdatePlanes` / `DrawPlanes` / alarm).
use airscout::pipeline::{tick, TickInput};
use as_aircraft::enums::PathMode;
use as_aircraft::{PlaneCategory, PlaneInfo};
use as_propagation::{PathParams, PropagationPath};
use as_settings::Settings;
use chrono::{DateTime, TimeZone, Utc};
use std::time::Instant;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
}

fn path(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> PropagationPath {
    PropagationPath::new(
        PathParams {
            lat1,
            lon1,
            h1: 100.0,
            lat2,
            lon2,
            h2: 100.0,
            qrg_ghz: 10.0,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.0,
        0.0,
        true,
    )
}

/// JO50 -> JO61 (about 50.5N 1E to 51.5N 3E).
fn jo50_jo61() -> PropagationPath {
    path(50.5, 1.0, 51.5, 3.0)
}

fn plane(hex: &str, lat: f64, lon: f64, track: f64, alt_ft: f64, cat: PlaneCategory) -> PlaneInfo {
    PlaneInfo::new(
        now(),
        &format!("C{hex}"),
        "",
        hex,
        lat,
        lon,
        track,
        alt_ft,
        400.0,
        "A320",
        "Airbus",
        "A320",
        cat,
    )
}

fn settings() -> Settings {
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_distance = 100.0;
    s
}

/// A plane north-west of the path, flying south-east across it, at `alt_ft`.
fn crossing(hex: &str, alt_ft: f64) -> PlaneInfo {
    plane(hex, 51.3, 1.6, 135.0, alt_ft, PlaneCategory::Medium)
}

fn run(
    paths: &[PropagationPath],
    planes: Vec<PlaneInfo>,
    s: &Settings,
) -> airscout::pipeline::TickResult {
    run_mode(paths, planes, s, PathMode::Single)
}

fn run_mode(
    paths: &[PropagationPath],
    planes: Vec<PlaneInfo>,
    s: &Settings,
    mode: PathMode,
) -> airscout::pipeline::TickResult {
    tick(TickInput {
        at: now(),
        paths,
        planes,
        settings: s,
        mode,
    })
}

const HIGH_FT: f64 = 32000.0;
const LOW_FT: f64 = 500.0;

#[test]
fn merge_prefers_higher_potential_with_int_point() {
    let s = settings();
    let a = jo50_jo61();
    // same geometry, but the antennas are higher than the plane: no potential on this path
    let b = path(50.5, 1.0, 51.5, 3.0);
    let b = PropagationPath::new(
        PathParams {
            h1: 12000.0,
            h2: 12000.0,
            ..b.params
        },
        0.0,
        0.0,
        true,
    );
    let ra = run(
        std::slice::from_ref(&a),
        vec![crossing("AAAAAA", HIGH_FT)],
        &s,
    );
    let rb = run(
        std::slice::from_ref(&b),
        vec![crossing("AAAAAA", HIGH_FT)],
        &s,
    );
    assert_eq!(ra.active["AAAAAA"].potential, 100);
    assert!(rb.active["AAAAAA"].potential < 100);
    assert!(rb.active["AAAAAA"].int_point.is_some());
    for paths in [[a.clone(), b.clone()], [b, a]] {
        let r = run(&paths, vec![crossing("AAAAAA", HIGH_FT)], &s);
        assert_eq!(r.active.len(), 1);
        assert_eq!(r.active["AAAAAA"].potential, 100);
    }
}

#[test]
fn merge_equal_potential_prefers_nearer() {
    let s = settings();
    let a = jo50_jo61();
    let c = path(50.6, 1.0, 51.6, 3.0); // shifted north: the same plane crosses closer to it
    let qrb = |p: &PropagationPath| {
        let r = run(
            std::slice::from_ref(p),
            vec![crossing("AAAAAA", HIGH_FT)],
            &s,
        );
        assert_eq!(r.active["AAAAAA"].potential, 100);
        r.active["AAAAAA"].int_qrb
    };
    let (qa, qc) = (qrb(&a), qrb(&c));
    assert!((qa - qc).abs() > 1.0, "paths must differ: {qa} vs {qc}");
    for paths in [[a.clone(), c.clone()], [c, a]] {
        let r = run(&paths, vec![crossing("AAAAAA", HIGH_FT)], &s);
        assert_eq!(r.active["AAAAAA"].int_qrb, qa.min(qc));
    }
}

#[test]
fn finding_82_global_nearest_survives_empty_later_path() {
    let s = settings();
    let near = jo50_jo61();
    let far = path(40.0, 10.0, 41.0, 12.0); // no plane within its circumcircle
    let planes = || vec![crossing("AAAAAA", HIGH_FT)];
    assert!(run(std::slice::from_ref(&near), planes(), &s)
        .nearest
        .is_some());
    let r = run(&[near.clone(), far.clone()], planes(), &s);
    assert_eq!(r.nearest.unwrap().hex, "AAAAAA");
    assert_eq!(r.highest_potential, 100);
    assert!(r.active.contains_key("AAAAAA"));
    let r = run(&[far, near], planes(), &s);
    assert_eq!(r.nearest.unwrap().hex, "AAAAAA");
    assert_eq!(r.highest_potential, 100);
}

#[test]
fn filter_hides_low_and_light_planes() {
    let mut s = settings();
    s.planes_min_alt_m = 5000;
    s.gui.planes_filter_min_category = PlaneCategory::Medium;
    let planes = vec![
        plane("AAAAAA", 51.3, 1.6, 135.0, HIGH_FT, PlaneCategory::Medium),
        plane("BBBBBB", 51.3, 1.6, 135.0, 3000.0, PlaneCategory::Medium), // too low
        plane("CCCCCC", 51.3, 1.6, 135.0, HIGH_FT, PlaneCategory::Light), // too light
    ];
    let r = run(&[jo50_jo61()], planes, &s);
    assert_eq!(r.active.len(), 3);
    let shown: Vec<_> = r.shown.iter().map(|p| p.hex.as_str()).collect();
    assert_eq!(shown, ["AAAAAA"]);
}

#[test]
fn highest_potential_counts_only_filtered_planes() {
    let mut s = settings();
    s.planes_min_alt_m = 12000; // above 32000 ft = 9754 m
    let r = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert_eq!(r.active["AAAAAA"].potential, 100);
    assert_eq!(r.highest_potential, 0);
}

#[test]
fn chart_points_split_by_alt_diff() {
    let s = settings();
    let planes = vec![crossing("AAAAAA", HIGH_FT), crossing("BBBBBB", LOW_FT)];
    let r = run(&[jo50_jo61()], planes, &s);
    assert_eq!(r.chart_hi.len(), 1);
    assert_eq!(r.chart_lo.len(), 1);
    let hi = &r.chart_hi[0];
    assert_eq!(hi.call, "CAAAAAA");
    let ip = r.active["AAAAAA"].int_point.unwrap();
    let want = as_geo::latlon::distance(50.5, 1.0, ip.lat, ip.lon);
    assert!((hi.dist_km - want).abs() < 1e-9);
    assert!((hi.alt_m - r.active["AAAAAA"].alt_m()).abs() < 1e-9);
    assert_eq!(r.chart_lo[0].call, "CBBBBBB");
}

#[test]
fn multi_has_no_chart_points() {
    let s = settings();
    let p = jo50_jo61();
    let r = run_mode(
        &[p.clone(), p],
        vec![crossing("AAAAAA", HIGH_FT)],
        &s,
        PathMode::Multi,
    );
    assert!(r.chart_hi.is_empty() && r.chart_lo.is_empty());
    assert_eq!(r.shown.len(), 1);
}

#[test]
fn single_mode_with_one_path_has_chart_points() {
    let s = settings();
    let r = run_mode(
        &[jo50_jo61()],
        vec![crossing("AAAAAA", HIGH_FT)],
        &s,
        PathMode::Single,
    );
    assert_eq!(r.chart_hi.len(), 1);
}

#[test]
fn multi_reports_potential_per_path() {
    let s = settings();
    let far = path(40.0, 10.0, 41.0, 12.0); // no plane within its circumcircle
    let r = run_mode(
        &[far, jo50_jo61()],
        vec![crossing("AAAAAA", HIGH_FT)],
        &s,
        PathMode::Multi,
    );
    assert_eq!(r.per_path_potential, [0, 100]);
}

#[test]
fn alarm_needs_activate_and_distance() {
    let mut s = settings();
    let planes = || vec![crossing("AAAAAA", HIGH_FT)];
    let qrb = run(&[jo50_jo61()], planes(), &s).active["AAAAAA"].int_qrb;
    s.gui.alarm_distance_km = qrb + 1.0;
    s.gui.alarm_activate = false;
    assert!(run(&[jo50_jo61()], planes(), &s).alarm.is_none());
    s.gui.alarm_activate = true;
    assert_eq!(
        run(&[jo50_jo61()], planes(), &s).alarm.unwrap().hex,
        "AAAAAA"
    );
    s.gui.alarm_distance_km = qrb - 1.0;
    assert!(run(&[jo50_jo61()], planes(), &s).alarm.is_none());
}

#[test]
fn finding_358_alarm_uses_strict_forward_distance_cutoff() {
    let mut s = settings();
    s.gui.alarm_activate = true;
    s.gui.alarm_distance_km = 2.0;
    // 1 km north of the equator path, moving east (parallel/receding from crossing): near but no
    // forward crossing ETA.
    let p = plane(
        "AAAAAA",
        0.008993216,
        1.0,
        90.0,
        HIGH_FT,
        PlaneCategory::Medium,
    );
    let r = run(&[path(0.0, 0.0, 0.0, 2.0)], vec![p], &s);
    assert!(r.alarm.is_none(), "{r:#?}");
}

#[test]
fn finding_27_common_floor_and_zero_disabled_angle_limits() {
    let mut s = settings();
    s.gui.alarm_activate = true;
    s.gui.alarm_distance_km = 1_000.0;
    let base = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert_eq!(base.per_path_potential, [100]);
    assert_eq!(base.shown.len(), 1);
    assert!(base.alarm.is_some());

    s.planes_min_alt_m = 12_000;
    let floor = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert_eq!(floor.per_path_potential, [0]);
    assert!(floor.shown.is_empty());
    assert!(floor.alarm.is_none());

    let mut s = settings();
    let base = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    let p = &base.active["AAAAAA"];
    let t1 = p.theta1.abs().to_degrees();
    let t2 = p.theta2.abs().to_degrees();
    let sq = p.squint.to_degrees();
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_squint = 0.0;
    s.band_settings.get_mut(&band).unwrap().max_elevation = 0.0;
    assert_eq!(
        run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s)
            .shown
            .len(),
        1
    );

    s.band_settings.get_mut(&band).unwrap().max_squint = sq.max(0.1) - 0.05;
    assert!(run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s)
        .shown
        .is_empty());

    let mut s = settings();
    s.band_settings.get_mut(&band).unwrap().max_elevation = t1.min(t2) - 0.01;
    assert!(run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s)
        .shown
        .is_empty());
}

/// The live Min-Alt filter (`Planes_Filter_Min_Alt`, edited in the main window and on the
/// Options Planes tab) raises the common floor; it never lowers `planes_min_alt_m`.
#[test]
fn live_min_alt_filter_raises_common_floor() {
    let mut s = settings();
    s.gui.alarm_activate = true;
    s.gui.alarm_distance_km = 1_000.0;
    s.gui.planes_filter_min_alt_m = 12_000;
    let r = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert_eq!(r.per_path_potential, [0]);
    assert!(r.shown.is_empty());
    assert!(r.alarm.is_none());

    s.gui.planes_filter_min_alt_m = 0;
    s.planes_min_alt_m = 12_000;
    let r = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert!(r.shown.is_empty());

    s.planes_min_alt_m = 0;
    let r = run(&[jo50_jo61()], vec![crossing("AAAAAA", HIGH_FT)], &s);
    assert_eq!(r.shown.len(), 1);
}

#[test]
fn pipeline_1000_planes_is_fast() {
    let s = settings();
    let planes: Vec<_> = (0..1000)
        .map(|i| {
            let (x, y) = (i % 40, i / 40);
            plane(
                &format!("{i:06X}"),
                49.5 + f64::from(y) * 0.1,
                0.0 + f64::from(x) * 0.1,
                f64::from(i * 7 % 360),
                HIGH_FT,
                PlaneCategory::Medium,
            )
        })
        .collect();
    let paths = [jo50_jo61()];
    let t = Instant::now();
    let r = run(&paths, planes, &s);
    let el = t.elapsed();
    assert!(!r.active.is_empty());
    assert!(el.as_millis() < 500, "tick took {el:?}");
}

#[test]
fn pipeline_50_paths_1000_planes_is_fast() {
    let s = settings();
    let planes: Vec<_> = (0..1000)
        .map(|i| {
            let (x, y) = (i % 40, i / 40);
            plane(
                &format!("{i:06X}"),
                49.5 + f64::from(y) * 0.1,
                0.0 + f64::from(x) * 0.1,
                f64::from(i * 7 % 360),
                HIGH_FT,
                PlaneCategory::Medium,
            )
        })
        .collect();
    let paths: Vec<_> = (0..50)
        .map(|i| {
            let d = f64::from(i) * 0.02;
            path(50.5 + d, 1.0, 51.5 - d, 3.0)
        })
        .collect();
    let t = Instant::now();
    let r = run_mode(&paths, planes, &s, PathMode::Multi);
    let el = t.elapsed();
    assert_eq!(r.per_path_potential.len(), 50);
    assert!(!r.active.is_empty());
    assert!(el.as_millis() < 1000, "tick took {el:?}");
}

/// HISTORY must evaluate all paths at the same replay instant: the crossing pass cannot depend on
/// a separate wall clock.
#[test]
fn history_uses_a_single_analysis_instant_across_paths() {
    let s = settings();
    let a = jo50_jo61();
    let paths = [a.clone(), a];
    let at = |analysis_at| {
        tick(TickInput {
            at: analysis_at,
            paths: &paths,
            planes: vec![crossing("AAAAAA", HIGH_FT)],
            settings: &s,
            mode: PathMode::Multi,
        })
    };
    let same = at(now());
    assert_eq!(same.per_path_potential, vec![100, 100]);
}
