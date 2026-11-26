//! Path CSV export and the plane history export (Task 9). No network, no display.
use airscout::analysis::{self, AnalysisJob};
use airscout::app::PathEntry;
use airscout::exports::{
    filter_plane_lists, path_file_name, plane_file_name, write_path_csv, write_plane_csv, HexOrCall,
};
use airscout::jobs::{JobCtx, JobEvent};
use airscout::services::ServiceEvent;
use as_aircraft::check::Lookup;
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_elevation::{ElevationModel, ElevationPath, ElevationStore, ElevationTile};
use as_geo::maidenhead;
use as_geo::propagation::epsilon_from_heights;
use as_propagation::{PathParams, PropagationPath};
use chrono::{DateTime, TimeZone, Utc};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

const HEADER: &str = "Distance[km];Lat[deg];Lon[deg];Elevation[m];Min_h1[m];Min_h2[m];Min_h[m];Max_h[m];F1[m];eps1[deg];eps2[deg];eps1_min[deg];eps2_min[deg];ElevationModel";

/// JO50 tiles with a ramp of 100 m + 1 m per grid cell column, so the profile is not flat.
fn tiles() -> Vec<ElevationTile> {
    let mut v = Vec::new();
    for a in 'A'..='X' {
        for b in 'A'..='X' {
            let index = format!("JO50{a}{b}");
            let r = maidenhead::bounds_from_loc(&index).unwrap();
            let base = 100 + (b as i16 - 'A' as i16) * 5;
            v.push(
                ElevationTile::new(
                    &index,
                    i32::from(base),
                    r.min_lat,
                    r.min_lon,
                    i32::from(base) + 10,
                    r.min_lat + 0.01,
                    r.min_lon + 0.02,
                    2,
                    2,
                    vec![base, base, base, base + 10],
                    Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap(),
                )
                .unwrap(),
            );
        }
    }
    v
}

fn params() -> PathParams {
    PathParams {
        lat1: 50.5,
        lon1: 11.0,
        h1: 150.0,
        lat2: 50.6,
        lon2: 11.2,
        h2: 210.0,
        qrg_ghz: 10.368,
        radius_km: 6371.0 * 4.0 / 3.0,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    }
}

fn entry() -> PathEntry {
    PathEntry {
        index: 0,
        call: "DX0AAA".into(),
        path: PropagationPath::new(params(), 0.001, 0.002, true),
        terrain: Vec::new(),
        my_gain_dbi: 0.0,
        dx_gain_dbi: 0.0,
    }
}

fn ctx() -> (JobCtx, Receiver<ServiceEvent>) {
    let (tx, rx) = channel();
    (JobCtx::new(1, Arc::new(AtomicBool::new(false)), tx), rx)
}

fn events(rx: &Receiver<ServiceEvent>) -> Vec<JobEvent> {
    rx.try_iter()
        .filter_map(|e| match e {
            ServiceEvent::Job { event, .. } => Some(event),
            _ => None,
        })
        .collect()
}

#[test]
fn path_csv_row_count_and_14_columns() {
    let dir = tempfile::tempdir().unwrap();
    ElevationStore::open(dir.path())
        .unwrap()
        .insert_tiles_atomic(ElevationModel::Globe, &tiles())
        .unwrap();
    let (ctx, rx) = ctx();
    analysis::run(
        &ctx,
        dir.path(),
        AnalysisJob::ExportPath {
            entry: Box::new(entry()),
            model: ElevationModel::Globe,
            max_alt_m: 12200.0,
            my_call: "DL/MY0AAA".into(),
            dx_call: "DX0AAA".into(),
        },
    );
    let file = dir
        .path()
        .join("Tmp/Path Information DL_MY0AAA to DX0AAA.csv");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{file:?}: {e}"));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], HEADER);
    let dist = entry().path.distance();
    let count = (dist * 1000.0 / 1000.0) as usize + 1;
    assert_eq!(lines.len() - 1, count);
    for l in &lines {
        assert_eq!(l.split(';').count(), 14, "{l}");
    }
    assert!(lines[1].ends_with(";GLOBE"));
    assert!(lines[1].starts_with("0;"));
    assert!(lines[2].starts_with("1;"));
    let ev = events(&rx);
    assert!(
        matches!(ev.last(), Some(JobEvent::Done(s)) if s.contains("Path Information DL_MY0AAA to DX0AAA.csv")),
        "{ev:?}"
    );
}

#[test]
fn path_csv_eps_values() {
    let pp = PropagationPath::new(params(), 0.001, 0.002, true);
    let mut elv = ElevationPath::new(50.5, 11.0, 50.6, 11.2, 1000.0);
    for (i, v) in elv.path.iter_mut().enumerate() {
        *v = 100 + i as i16 * 3;
    }
    let mut out = Vec::new();
    let cancel = AtomicBool::new(false);
    assert!(write_path_csv(&mut out, &pp, &elv, ElevationModel::Globe, 12200.0, &cancel).unwrap());
    let text = String::from_utf8(out).unwrap();
    let row: Vec<&str> = text.lines().nth(1 + 5).unwrap().split(';').collect();
    let p = params();
    let d = pp.distance();
    let e = f64::from(elv.path[5]);
    let deg = |r: f64| r / std::f64::consts::PI * 180.0;
    let num = |i: usize| row[i].parse::<f64>().unwrap();
    assert_eq!(num(0), 5.0);
    assert_eq!(num(3), e);
    assert_eq!(num(7), 12200.0);
    let eps1 = deg(epsilon_from_heights(p.h1, 5.0, e, p.radius_km));
    let eps2 = deg(epsilon_from_heights(p.h2, d - 5.0, e, p.radius_km));
    assert!((num(9) - eps1).abs() < 1e-12, "{} vs {eps1}", row[9]);
    assert!((num(10) - eps2).abs() < 1e-12, "{} vs {eps2}", row[10]);
    assert!((num(11) - deg(0.001)).abs() < 1e-12);
    assert!((num(12) - deg(0.002)).abs() < 1e-12);
    // Min_h is the larger of Min_h1 and Min_h2
    assert_eq!(num(6), num(4).max(num(5)));
    assert_eq!(row[13], "GLOBE");
}

#[test]
fn path_csv_cancel_stops_the_export() {
    let pp = PropagationPath::new(params(), 0.001, 0.002, true);
    let elv = ElevationPath::new(50.5, 11.0, 50.6, 11.2, 1000.0);
    let cancel = AtomicBool::new(true);
    let mut out = Vec::new();
    assert!(!write_path_csv(&mut out, &pp, &elv, ElevationModel::Globe, 1.0, &cancel).unwrap());
}

#[test]
fn file_names() {
    assert_eq!(
        path_file_name("A/B", "C/D"),
        "Path Information A_B to C_D.csv"
    );
    let t = |h, m, s| Utc.with_ymd_and_hms(2024, 5, 1, h, m, s).unwrap();
    assert_eq!(
        plane_file_name("ABC123", t(1, 2, 3), t(13, 14, 15)),
        "Plane History_ABC123_20240501_010203_to_20240501_131415.csv"
    );
}

#[test]
fn file_names_are_valid_on_windows() {
    let bad = "A/B\\C:D*E?F\"G<H>I|J";
    assert_eq!(
        path_file_name(bad, "X"),
        "Path Information A_B_C_D_E_F_G_H_I_J to X.csv"
    );
    let t = Utc.with_ymd_and_hms(2024, 5, 1, 1, 2, 3).unwrap();
    let n = plane_file_name(bad, t, t);
    assert!(
        !n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']),
        "{n}"
    );
}

struct Lines;
impl Lookup for Lines {
    fn airline_by_icao(&self, icao: &str) -> bool {
        icao == "DLH"
    }
    fn airline_by_iata(&self, _: &str) -> bool {
        false
    }
    fn registration_by_reg(&self, _: &str) -> bool {
        false
    }
    fn airport_by_icao(&self, _: &str) -> bool {
        false
    }
    fn type_by_icao(&self, _: &str) -> bool {
        false
    }
    fn registration_min_len(&self) -> usize {
        2
    }
    fn type_icao_min_len(&self) -> usize {
        2
    }
}

#[test]
fn plane_lists_filtered() {
    let hexes = ["abc123", "ABCDEF", "XYZ123", "12345", "1234567", "ABCDEF"]
        .map(String::from)
        .to_vec();
    let calls = ["DLH123", "DLH", "DLH-12", "XXX999", "dlh9ab"]
        .map(String::from)
        .to_vec();
    let cancel = AtomicBool::new(false);
    let (h, c) = filter_plane_lists(hexes, calls, &Lines, &cancel).unwrap();
    // upper-cased, 6 hex digits, no duplicates
    assert_eq!(h, ["ABC123", "ABCDEF"]);
    // at least 4 characters, no '-', known airline
    assert_eq!(c, ["DLH123", "DLH9AB"]);
}

fn at(s: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + s, 0).unwrap()
}

fn row(hex: &str, call: &str, lat: f64, lon: f64, s: i64) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: call.into(),
        lat,
        lon,
        alt_ft: 30000.0,
        track: 90.0,
        speed_kts: 400.0,
        last_updated: at(s),
    }
}

fn plane_csv(rows: &[AircraftPosition]) -> Vec<String> {
    let mut out = Vec::new();
    assert!(write_plane_csv(&mut out, rows, &AtomicBool::new(false)).unwrap());
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn plane_history_dist_and_av_speed() {
    let rows = [
        row("AAAAAA", "DLH1", 50.0, 8.0, 0),
        row("AAAAAA", "DLH1", 50.0, 8.1, 360),
    ];
    let l = plane_csv(&rows);
    assert_eq!(
        l[0],
        "time;hex;call;lat[deg];lon[deg];alt[m];track[deg];speed[kmh];dist[km];av_speed[km/h]"
    );
    let f0: Vec<&str> = l[1].split(';').collect();
    let f1: Vec<&str> = l[2].split(';').collect();
    assert_eq!(f0.len(), 10);
    assert_eq!(f0[0], at(0).format("%Y-%m-%d %H:%M:%S").to_string());
    assert_eq!(f0[5], format!("{:.8}", as_aircraft::units::ft_m(30000.0)));
    assert_eq!(f0[7], format!("{:.8}", as_aircraft::units::kts_kmh(400.0)));
    assert_eq!((f0[8], f0[9]), ("0.00000000", "0.00000000"));
    let d = as_geo::latlon::distance(50.0, 8.0, 50.0, 8.1);
    assert_eq!(f1[8], format!("{d:.8}"));
    // 360 s = 0.1 h
    assert_eq!(f1[9], format!("{:.8}", d / 0.1));
}

#[test]
fn plane_history_same_second_writes_infinity() {
    let rows = [
        row("AAAAAA", "DLH1", 50.0, 8.0, 0),
        row("AAAAAA", "DLH1", 50.0, 8.1, 0),
        row("AAAAAA", "DLH1", 50.0, 8.1, 0),
    ];
    let l = plane_csv(&rows);
    assert!(l[2].ends_with(";Infinity"), "{}", l[2]);
    // same place, same second: 0 / 0
    assert!(l[3].ends_with(";0.00000000;NaN"), "{}", l[3]);
}

#[test]
fn plane_history_by_call_mixes_hexes() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    db.bulk_insert(&[
        row("AAAAAA", "DLH1", 50.0, 8.0, 0),
        row("BBBBBB", "DLH1", 51.0, 9.0, 60),
        row("CCCCCC", "OTHER", 52.0, 9.0, 90),
    ])
    .unwrap();
    drop(db);
    let (ctx, rx) = ctx();
    analysis::run(
        &ctx,
        dir.path(),
        AnalysisJob::ExportPlane {
            key: HexOrCall::Call("DLH1".into()),
            from: at(-10),
            to: at(100),
        },
    );
    let file = dir
        .path()
        .join(format!("Tmp/{}", plane_file_name("DLH1", at(-10), at(100))));
    let text = std::fs::read_to_string(&file).unwrap();
    let l: Vec<&str> = text.lines().collect();
    assert_eq!(l.len(), 3, "{l:?}");
    assert!(l[1].contains(";AAAAAA;") && l[2].contains(";BBBBBB;"));
    // the distance runs between the two different hexes
    let d = as_geo::latlon::distance(50.0, 8.0, 51.0, 9.0);
    assert!(l[2].contains(&format!(";{d:.8};")), "{}", l[2]);
    assert!(matches!(events(&rx).last(), Some(JobEvent::Done(_))));
}

#[test]
fn plane_lists_job_sends_filtered_lists() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    db.bulk_insert(&[
        row("aaaaaa", "X", 50.0, 8.0, 0),
        row("BBBBBB", "X", 50.0, 8.0, 0),
        row("ZZ", "X", 50.0, 8.0, 5),
    ])
    .unwrap();
    drop(db);
    let (ctx, rx) = ctx();
    analysis::run(
        &ctx,
        dir.path(),
        AnalysisJob::PlaneLists {
            from: at(-10),
            to: at(100),
        },
    );
    let lists = events(&rx)
        .into_iter()
        .find_map(|e| match e {
            JobEvent::PlaneLists { hexes, calls } => Some((hexes, calls)),
            _ => None,
        })
        .expect("PlaneLists event");
    assert_eq!(lists.0, ["AAAAAA", "BBBBBB"]);
    assert!(lists.1.is_empty());
}
