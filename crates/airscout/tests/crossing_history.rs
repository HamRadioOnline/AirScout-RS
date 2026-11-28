//! Crossing history (Task 10): the algorithm of `CrossingHistoryDlg.bw_History_DoWork`, the
//! ambiguity pass and the CSV, the path set-up and the job. Synthetic data only, no network.
mod common;
use airscout::analysis::{run, AnalysisJob};
use airscout::crossing_csv::CROSSING_HEADER;
use airscout::crossing_history::{
    chart_buckets, crossing_history, crossing_path, crossing_range, CrossingParams,
};
use airscout::history::HistoryData;
use airscout::jobs::{JobCtx, JobEvent};
use airscout::services::{open_data, PathRequest, PathService, PathTarget, ServiceEvent};
use as_aircraft::PlaneInfo;
use as_elevation::ElevationModel;
use as_geo::latlon::GPoint;
use as_settings::Settings;
use as_stations::{GeoSource, Location};
use chrono::{DateTime, Duration, TimeZone, Utc};
use common::*;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;

fn go(
    positions: &mut [PlaneInfo],
    p: &CrossingParams,
    signal: &dyn Fn(DateTime<Utc>) -> Option<f64>,
) -> Vec<PlaneInfo> {
    crossing_history(
        positions,
        &path(),
        p,
        signal,
        &AtomicBool::new(false),
        &mut |_| {},
    )
}

#[test]
fn crossing_history_finds_plane_on_path() {
    let mut positions = history();
    let found = go(&mut positions, &params(), &no_signal);
    assert!(found.len() >= 2, "{} crossings", found.len());
    for c in &found {
        assert_eq!(c.hex, "abc123");
        assert!(c.int_qrb <= 30.0, "qrb {}", c.int_qrb);
        assert_eq!(c.potential, 100);
        assert_eq!(c.signal_strength, f64::MIN);
    }
    // the planes are found at the step times, one step apart
    assert!(found.windows(2).all(|w| w[1].time > w[0].time));
}

#[test]
fn finding_127_history_windows_keep_source_data_immutable() {
    let mut positions = vec![
        report("abc123", 0, 50.0, 8.5),
        report("abc123", 10, 50.1, 8.5),
        report("abc123", 20, 50.2, 8.5),
    ];
    let before = positions.clone();
    let mut p = params();
    p.to = t0() + Duration::seconds(10);
    let first = go(&mut positions, &p, &no_signal);
    let second = go(&mut positions, &p, &no_signal);
    assert_eq!(positions, before);
    assert_eq!(first, second);
}

#[test]
fn finding_128_history_steps_keep_running_after_gaps() {
    let mut positions = vec![
        crossing("abc123", 50 * 60),
        crossing("abc123", 70 * 60),
        crossing("abc123", 180 * 60),
    ];
    positions.iter_mut().for_each(|p| p.speed_kts = 0.0);
    let mut p = params();
    p.from = t0();
    p.to = t0() + Duration::minutes(180);
    p.step_s = 60 * 60;
    p.ttl_min = 30;
    p.max_distance_km = 100.0;
    let found = go(&mut positions, &p, &no_signal);
    let times: Vec<_> = found.into_iter().map(|c| c.time).collect();
    assert_eq!(
        times,
        vec![t0() + Duration::minutes(60), t0() + Duration::minutes(180)]
    );
}

#[test]
fn finding_128_preroll_and_exact_step_supersession() {
    let mut positions = vec![
        report("abc123", 30 * 60, 50.0, 8.5),
        report("abc123", 60 * 60, 50.2, 8.5),
    ];
    positions.iter_mut().for_each(|p| p.speed_kts = 0.0);
    let mut p = params();
    p.from = t0() + Duration::minutes(60);
    p.to = p.from;
    p.step_s = 60;
    p.ttl_min = 30;
    p.max_distance_km = 100.0;
    let found = go(&mut positions, &p, &no_signal);
    assert_eq!(found.len(), 1);
    // The report exactly at the step supersedes the pre-roll report exactly one TTL old.
    assert!((found[0].lat - 50.2).abs() < 1e-9);
}

#[test]
fn startindex_gap_ends_loop_without_panic() {
    // the last report is older than ttl for the later steps: C# crashes at positions[-1]
    let mut positions: Vec<PlaneInfo> = history()
        .into_iter()
        .filter(|p| p.time <= t0() + Duration::seconds(300))
        .collect();
    let mut p = params();
    p.to = t0() + Duration::hours(2);
    let found = go(&mut positions, &p, &no_signal);
    let last = positions.iter().map(|x| x.time).max().unwrap();
    assert!(!found.is_empty());
    assert!(found.iter().all(|c| c.time <= last + Duration::minutes(5)));
}

#[test]
fn finding_369_history_uses_configured_radius_not_half_path_default() {
    let mut positions = vec![report("abc123", 0, 50.495, 8.5)];
    let mut p = params();
    p.to = t0();
    p.step_s = 60;
    p.max_distance_km = 100.0;
    p.max_radius_km = 60.0;
    let found = go(&mut positions, &p, &no_signal);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn finding_369_entry_by_motion_is_evaluated_at_each_step() {
    let mut positions = vec![report("abc123", 0, 50.7, 8.5)];
    positions[0].track = 180.0;
    let mut p = params();
    p.from = t0();
    p.to = t0() + Duration::seconds(300);
    p.step_s = 300;
    p.ttl_min = 10;
    p.max_distance_km = 100.0;
    p.max_radius_km = 60.0;
    let found = go(&mut positions, &p, &no_signal);
    assert_eq!(found.iter().filter(|c| c.time == t0()).count(), 0);
    assert_eq!(
        found
            .iter()
            .filter(|c| c.time == t0() + Duration::seconds(300))
            .count(),
        1
    );
}

#[test]
fn no_positions_near_the_path_gives_nothing() {
    let mut positions = vec![report("abc123", 0, 10.0, 80.0)];
    assert!(go(&mut positions, &params(), &no_signal).is_empty());
    let mut empty = Vec::new();
    assert!(go(&mut empty, &params(), &no_signal).is_empty());
}

#[test]
fn cancel_stops_the_run() {
    let mut positions = northbound("abc123");
    let cancel = AtomicBool::new(true);
    let found = crossing_history(
        &mut positions,
        &path(),
        &params(),
        &no_signal,
        &cancel,
        &mut |_| {},
    );
    assert!(found.is_empty());
}

#[test]
fn progress_is_reported_per_step() {
    let mut positions = history();
    let mut calls = 0;
    let found = crossing_history(
        &mut positions,
        &path(),
        &params(),
        &no_signal,
        &AtomicBool::new(false),
        &mut |_| calls += 1,
    );
    assert!(!found.is_empty());
    assert!(calls >= 5, "{calls}");
}

#[test]
fn signal_level_filter() {
    // levels exist for every other minute only
    let signal = |t: DateTime<Utc>| ((t - t0()).num_seconds() % 120 < 60).then_some(-101.5);
    let mut p = params();
    p.max_distance_km = 100.0;
    let all = go(&mut history(), &p, &signal);
    assert!(all.iter().any(|c| c.signal_strength == f64::MIN));
    assert!(all.iter().any(|c| c.signal_strength == -101.5));
    p.with_signal_level = true;
    let only = go(&mut history(), &p, &signal);
    assert!(!only.is_empty() && only.len() < all.len());
    assert!(only.iter().all(|c| c.signal_strength == -101.5));
}

#[test]
fn range_is_cut_clamped_and_ordered() {
    let oldest = t0() - Duration::hours(30);
    let from = t0() + Duration::hours(5);
    let to = t0() + Duration::hours(9);
    let (f, t, s) = crossing_range(from, to, Some(oldest), 60);
    assert_eq!(f, Utc.with_ymd_and_hms(2024, 5, 1, 0, 0, 0).unwrap());
    assert_eq!((t, s), (to, 60));
    // older than the oldest report: clamped up
    let oldest = Utc.with_ymd_and_hms(2024, 5, 1, 7, 30, 0).unwrap();
    assert_eq!(crossing_range(from, to, Some(oldest), 60).0, oldest);
    // no stored position: no clamp
    assert_eq!(crossing_range(from, to, None, 60).0, f);
    // from after to: from = to
    let (f, t, _) = crossing_range(to + Duration::days(2), to, Some(oldest), 60);
    assert_eq!(f, t);
    // step minimum 1
    assert_eq!(crossing_range(from, to, None, 0).2, 1);
    assert_eq!(crossing_range(from, to, None, -5).2, 1);
}

#[test]
fn chart_buckets_hi_lo() {
    let mut a = crossing("aaaaaa", 0);
    a.potential = 100;
    let mut b = crossing("bbbbbb", 0);
    b.potential = 100;
    let mut c = crossing("cccccc", 0);
    c.potential = 75;
    let mut d = crossing("dddddd", 5);
    d.potential = 50;
    let mut e = crossing("eeeeee", 9);
    e.potential = 100;
    let (hi, lo) = chart_buckets(&[a, b, c, d, e]);
    let t = |s: i64| t0() + Duration::seconds(s);
    // both series carry every key: the missing side is 0
    assert_eq!(hi, [(t(0), 2), (t(5), 0), (t(9), 1)]);
    assert_eq!(lo, [(t(0), 1), (t(5), 1), (t(9), 0)]);
    assert_eq!(chart_buckets(&[]), (vec![], vec![]));
}

// ---- path and job (real databases in a temp data dir)

fn settings() -> Settings {
    Settings {
        my_call: "TST1AB".into(),
        my_lat: 50.0,
        my_lon: 8.0,
        dx_call: "TST2AB".into(),
        dx_lat: 50.0,
        dx_lon: 9.0,
        path_best_case_elevation: false,
        ..Settings::default()
    }
}

#[test]
fn crossing_path_uses_settings_positions_like_live_paths() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let s = settings();
    let loc = s.my_loc().unwrap();
    // A displaced stored location must not replace the settings position.
    paths
        .stations
        .location_insert_or_update_if_newer(&Location::new(
            "tst1ab",
            &loc,
            50.0123,
            8.0456,
            GeoSource::FromUser,
            0,
            Utc::now(),
        ))
        .unwrap();
    let p = crossing_path(&paths, &s, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!((p.params.lat1, p.params.lon1), (50.0, 8.0));
    assert_eq!((p.params.lat2, p.params.lon2), (50.0, 9.0));
    // no terrain data: ground 0, antenna height is the band default
    assert_eq!(p.params.h1, as_stations::default_antenna_height(s.band));
    assert_eq!(
        p.params.step_width_m,
        as_elevation::ElevationModel::Globe.default_step_width_m()
    );
}

#[test]
fn finding_133_history_path_matches_live_path_resolver() {
    let dir = data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let mut s = settings();
    s.path_best_case_elevation = true;
    s.elevation_enabled = as_settings::ElevationEnabled {
        globe: true,
        srtm3: false,
        srtm1: true,
        aster3: false,
        aster1: false,
    };
    s.gui.elevation_download = false;
    s.my_call = "TE1ST".into();
    s.dx_call = "TE2ND".into();
    s.my_lat = 50.01;
    s.my_lon = 10.03;
    s.dx_lat = 50.01;
    s.dx_lon = 11.03;

    let my_loc = s.my_loc().unwrap();
    paths
        .stations
        .location_insert_or_update_if_newer(&Location::new(
            "TE1ST",
            &my_loc,
            50.1200,
            10.2200,
            GeoSource::FromUser,
            0,
            Utc::now(),
        ))
        .unwrap();

    let history_path = crossing_path(&paths, &s, &AtomicBool::new(false))
        .unwrap()
        .unwrap();

    let (tx, rx) = channel();
    let svc = PathService::spawn(paths.clone(), tx);
    svc.request(PathRequest {
        id: 7,
        my: GPoint::new(s.my_lat, s.my_lon),
        my_call: s.my_call.clone(),
        targets: vec![PathTarget {
            call: s.dx_call.clone(),
            pos: GPoint::new(s.dx_lat, s.dx_lon),
        }],
        settings: s.clone(),
    });
    let live_path = loop {
        match rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap() {
            ServiceEvent::PathReady { path, .. } => break path,
            ServiceEvent::PathFailed { error, .. } => panic!("{error}"),
            _ => {}
        }
    };
    svc.stop();

    assert_eq!(history_path.params, live_path.params);
    assert_eq!(
        history_path.params.step_width_m,
        ElevationModel::Srtm1.default_step_width_m()
    );
}

#[test]
fn crossing_path_needs_a_model() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let mut s = settings();
    s.elevation_enabled = as_settings::ElevationEnabled {
        globe: false,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    assert!(crossing_path(&paths, &s, &AtomicBool::new(false)).is_err());
}

fn job_ctx() -> (JobCtx, std::sync::mpsc::Receiver<ServiceEvent>) {
    let (tx, rx) = channel();
    (JobCtx::new(5, Arc::new(AtomicBool::new(false)), tx), rx)
}

fn job_events(rx: &std::sync::mpsc::Receiver<ServiceEvent>) -> Vec<JobEvent> {
    rx.try_iter()
        .map(|e| match e {
            ServiceEvent::Job { id, event } => {
                assert_eq!(id, 5);
                event
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

fn history_of(positions: Vec<PlaneInfo>) -> Arc<HistoryData> {
    Arc::new(HistoryData {
        oldest: positions.first().map(|p| p.time),
        youngest: positions.last().map(|p| p.time),
        count: positions.len() as u64,
        positions,
    })
}

#[test]
fn caller_positions_untouched() {
    let dir = tempfile::tempdir().unwrap();
    // two reports of the same hex: the aliasing in the algorithm would change the first
    let mut positions = history();
    positions.extend(northbound("abc123"));
    positions.sort_by_key(|p| p.time);
    let data = history_of(positions);
    let before = data.positions.clone();
    let (ctx, rx) = job_ctx();
    let mut p = params();
    p.max_distance_km = 100.0;
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Crossings {
            data: data.clone(),
            params: Box::new(p),
            settings: Box::new(settings()),
        },
    );
    assert_eq!(data.positions, before, "the app's history is not changed");
    let ev = job_events(&rx);
    let result = ev
        .iter()
        .find_map(|e| match e {
            JobEvent::Crossings(r) => Some(r),
            _ => None,
        })
        .expect("Crossings event");
    assert!(!result.crossings.is_empty());
    assert_eq!(
        result
            .count_hi
            .iter()
            .map(|(_, n)| *n as usize)
            .sum::<usize>()
            + result
                .count_lo
                .iter()
                .map(|(_, n)| *n as usize)
                .sum::<usize>(),
        result.crossings.len()
    );
    assert!(matches!(ev.last(), Some(JobEvent::Done(_))), "{ev:?}");
}

#[test]
fn job_reads_signal_levels_from_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let sig = as_signals::SignalDb::open(dir.path()).unwrap();
    let data = history_of(history());
    let (ctx, rx) = job_ctx();
    let mut p = params();
    p.max_distance_km = 100.0;
    // a level for the second of every step of the first minute
    for s in 0..400 {
        sig.insert_or_update_if_newer(-90.0, t0() + Duration::seconds(s))
            .unwrap();
    }
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Crossings {
            data,
            params: Box::new(p),
            settings: Box::new(settings()),
        },
    );
    let ev = job_events(&rx);
    let r = ev
        .iter()
        .find_map(|e| match e {
            JobEvent::Crossings(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(r.crossings.iter().any(|c| c.signal_strength == -90.0));
}

#[test]
fn export_job_writes_the_csv_in_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let data = history_of(history());
    let (ctx, rx) = job_ctx();
    let mut p = params();
    p.max_distance_km = 100.0;
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Crossings {
            data,
            params: Box::new(p),
            settings: Box::new(settings()),
        },
    );
    let result = job_events(&rx)
        .into_iter()
        .find_map(|e| match e {
            JobEvent::Crossings(r) => Some(Arc::new(*r)),
            _ => None,
        })
        .unwrap();
    let n = result.crossings.len();
    run(
        &ctx,
        dir.path(),
        AnalysisJob::ExportCrossings {
            result,
            gap_s: 10.0,
            from: t0(),
            to: t0() + Duration::minutes(30),
            my_call: "TST1AB".into(),
            dx_call: "TST2AB".into(),
        },
    );
    let ev = job_events(&rx);
    let Some(JobEvent::Done(msg)) = ev.last() else {
        panic!("{ev:?}");
    };
    let file = dir
        .path()
        .join("Tmp")
        .join("Path Crossing History_TST1AB_TST2AB_20240501_120000_to_20240501_123000.csv");
    assert!(msg.contains(&file.display().to_string()), "{msg}");
    let text = std::fs::read_to_string(file).unwrap();
    assert_eq!(text.lines().count(), n + 1);
    assert_eq!(text.lines().next().unwrap(), CROSSING_HEADER);
    assert!(text.lines().any(|l| l.ends_with(";True")));
}

#[test]
fn crossings_carry_the_step_time_not_the_report_time() {
    // Finding #123: reports at 3 s past the grid of the steps (first step = first pre-selected
    // report). Every crossing time is a step time, none is a report time.
    let mut v: Vec<PlaneInfo> = history()
        .into_iter()
        .map(|mut p| {
            p.time += Duration::seconds(3);
            p
        })
        .collect();
    let mut p = params();
    p.step_s = 47;
    let found = go(&mut v, &p, &no_signal);
    assert!(!found.is_empty());
    let reports: Vec<_> = history()
        .iter()
        .map(|r| r.time + Duration::seconds(3))
        .collect();
    for c in &found {
        assert_eq!((c.time - p.from).num_seconds() % 47, 0, "{}", c.time);
    }
    eprintln!(
        "{:?}",
        found
            .iter()
            .map(|c| (c.time - t0()).num_seconds())
            .collect::<Vec<_>>()
    );
    assert!(found.iter().any(|c| !reports.contains(&c.time)));
}

#[test]
fn cancelled_path_build_ends_the_job_without_result() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let cancel = AtomicBool::new(true);
    assert!(crossing_path(&paths, &settings(), &cancel)
        .unwrap()
        .is_none());
    let (tx, rx) = channel();
    let ctx = JobCtx::new(5, Arc::new(AtomicBool::new(true)), tx);
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Crossings {
            data: history_of(history()),
            params: Box::new(params()),
            settings: Box::new(settings()),
        },
    );
    let ev = job_events(&rx);
    assert!(
        ev.iter().all(|e| matches!(e, JobEvent::Progress(_))),
        "{ev:?}"
    );
}
