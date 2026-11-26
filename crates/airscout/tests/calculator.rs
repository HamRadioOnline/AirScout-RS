//! Background path calculator: fills the path database, throttles, cancels (no network).
use airscout::calculator::{Calculator, RunOutcome, Timing};
use airscout::services::{open_data, MyStation, Paths};
use as_elevation::{ElevationModel, ElevationTile};
use as_geo::band::Band;
use as_geo::latlon::{GPoint, EARTH_RADIUS_KM};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_propagation::PathParams;
use as_settings::{BandSettings, ElevationEnabled, Settings};
use as_stations::{GeoSource, Location};
use chrono::{DateTime, TimeZone, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const BANDS: [Band; 3] = [Band::B144M, Band::B432M, Band::B1_2G];
const MODEL: ElevationModel = ElevationModel::Globe;

fn stamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap()
}

/// Flat 100 m tiles for all of JO50; the maximum of every tile is 300 m, 0.01 deg north and
/// 0.02 deg east of its south-west corner.
fn flat_tiles() -> Vec<ElevationTile> {
    let mut v = Vec::new();
    for a in 'A'..='X' {
        for b in 'A'..='X' {
            let index = format!("JO50{a}{b}");
            let r = maidenhead::bounds_from_loc(&index).unwrap();
            v.push(
                ElevationTile::new(
                    &index,
                    100,
                    r.min_lat,
                    r.min_lon,
                    300,
                    r.min_lat + 0.01,
                    r.min_lon + 0.02,
                    2,
                    2,
                    vec![100; 4],
                    stamp(),
                )
                .unwrap(),
            );
        }
    }
    v
}

fn settings(my: GPoint) -> Settings {
    let mut s = Settings {
        my_call: "DL2ALF".into(),
        my_lat: my.lat,
        my_lon: my.lon,
        path_best_case_elevation: false,
        path_max_length_km: 500.0,
        ..Settings::default()
    };
    // inside JO50, which the fixture's tiles cover completely
    s.area.min_lat = 50.2;
    s.area.max_lat = 50.8;
    s.area.min_lon = 10.2;
    s.area.max_lon = 11.8;
    s.elevation_enabled = ElevationEnabled {
        globe: true,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    s.band_settings = BANDS
        .iter()
        .enumerate()
        .map(|(i, b)| {
            (
                *b,
                BandSettings {
                    k_factor: 1.2 + 0.1 * i as f64,
                    f1_clearance: 0.6,
                    ..BandSettings::default()
                },
            )
        })
        .collect();
    s
}

fn station(db: &Paths, call: &str, p: GPoint) {
    let loc = maidenhead::loc_from_lat_lon(p.lat, p.lon, false, 3, false)
        .unwrap()
        .unwrap();
    db.stations
        .location_insert_or_update_if_newer(&Location::new(
            call,
            &loc,
            p.lat,
            p.lon,
            GeoSource::FromUser,
            0,
            stamp(),
        ))
        .unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    paths: Paths,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .store
        .insert_tiles_atomic(MODEL, &flat_tiles())
        .unwrap();
    paths
        .stations
        .set_table_timestamp("Location", stamp())
        .unwrap();
    Fixture { _dir: dir, paths }
}

fn calc(f: &Fixture, settings: Settings) -> Calculator {
    f.paths.set_my_station(MyStation {
        call: settings.my_call.clone(),
        lat: settings.my_lat,
        lon: settings.my_lon,
    });
    Calculator {
        paths: f.paths.clone(),
        settings,
        model: MODEL,
        cancel: Arc::new(AtomicBool::new(false)),
        timing: Timing {
            per_location: Duration::ZERO,
            busy_wait: Duration::from_millis(20),
        },
    }
}

fn my() -> GPoint {
    GPoint::new(50.52, 10.55)
}

fn run(c: &Calculator) -> (RunOutcome, Vec<String>) {
    let mut msgs = Vec::new();
    let r = c.run_once(&mut |m| msgs.push(m)).unwrap();
    (r, msgs)
}

/// The path the calculator must have stored: heights are the 100 m ground plus 10 m antenna.
fn expected(c: &Calculator, a: GPoint, h1: f64, b: GPoint, h2: f64, band: Band) -> PathParams {
    PathParams {
        lat1: a.lat,
        lon1: a.lon,
        h1,
        lat2: b.lat,
        lon2: b.lon,
        h2,
        qrg_ghz: band.to_ghz(),
        radius_km: EARTH_RADIUS_KM * c.settings.band_settings[&band].k_factor,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    }
}

fn stored(c: &Calculator, p: &PathParams) -> bool {
    c.paths.engine.find_path(MODEL, p).unwrap().is_some()
}

#[test]
fn calculator_fills_missing_paths_for_all_bands() {
    let f = fixture();
    let (d1, d2) = (GPoint::new(50.61, 10.71), GPoint::new(50.43, 10.32));
    station(&f.paths, "DL1AAA", d1);
    station(&f.paths, "DL1BBB", d2);
    let c = calc(&f, settings(my()));
    let (r, msgs) = run(&c);
    assert_eq!(
        r,
        RunOutcome::Finished {
            found: 0,
            created: 6,
            unsaved: 0
        }
    );
    assert!(msgs.iter().any(|m| m == "GLOBE started."));
    assert!(msgs
        .iter()
        .any(|m| m.starts_with("GLOBE calculating path[ 144M]: DL2ALF<>DL1AAA")));
    assert!(msgs.last().unwrap().ends_with("finished."));
    for band in BANDS {
        for d in [d1, d2] {
            assert!(
                stored(&c, &expected(&c, my(), 110.0, d, 110.0, band)),
                "{band:?} {d:?}"
            );
        }
    }
    // a second run finds everything stored
    std::fs::remove_file(c.paths.data_dir.join("calculator-globe.stamp")).unwrap();
    assert_eq!(
        run(&c).0,
        RunOutcome::Finished {
            found: 6,
            created: 0,
            unsaved: 0
        }
    );
}

#[test]
fn calculator_uses_stored_qrv_antenna_height() {
    let f = fixture();
    let d = GPoint::new(50.61, 10.71);
    station(&f.paths, "DL1AAA", d);
    let loc = maidenhead::loc_from_lat_lon(d.lat, d.lon, false, 3, false)
        .unwrap()
        .unwrap();
    f.paths
        .stations
        .qrv_insert_or_update_if_newer(
            &as_stations::Qrv::new("DL1AAA", &loc, Band::B144M, 25.0, 0.0, 0.0, stamp()).unwrap(),
        )
        .unwrap();
    let c = calc(&f, settings(my()));
    run(&c);
    assert!(stored(
        &c,
        &expected(&c, my(), 110.0, d, 125.0, Band::B144M)
    ));
    assert!(stored(
        &c,
        &expected(&c, my(), 110.0, d, 110.0, Band::B432M)
    ));
}

#[test]
fn calculator_skips_stations_beyond_max_length() {
    let f = fixture();
    station(&f.paths, "DL1NEAR", GPoint::new(50.61, 10.71));
    // about 74 km east of My, inside the area
    station(&f.paths, "DL1FAR", GPoint::new(50.52, 11.6));
    let mut s = settings(my());
    s.path_max_length_km = 50.0;
    let c = calc(&f, s);
    let (r, msgs) = run(&c);
    assert_eq!(
        r,
        RunOutcome::Finished {
            found: 0,
            created: 3,
            unsaved: 0
        }
    );
    assert!(!msgs.iter().any(|m| m.contains("DL1FAR")));
}

#[test]
fn calculator_skips_when_stamp_unchanged() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    let c = calc(&f, settings(my()));
    assert!(matches!(run(&c).0, RunOutcome::Finished { created: 3, .. }));
    let stamp_file = c.paths.data_dir.join("calculator-globe.stamp");
    assert!(stamp_file.exists());
    // station table unchanged: nothing to do, not even a progress message
    let (r, msgs) = run(&c);
    assert_eq!(r, RunOutcome::Skipped);
    assert!(msgs.is_empty());
    // the station updater touched the table: run again
    f.paths
        .stations
        .set_table_timestamp("Location", stamp() + chrono::Duration::hours(1))
        .unwrap();
    assert_eq!(
        run(&c).0,
        RunOutcome::Finished {
            found: 3,
            created: 0,
            unsaved: 0
        }
    );
}

#[test]
fn calculator_waits_while_ui_busy() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    let c = Arc::new(calc(&f, settings(my())));
    f.paths.ui_busy.store(true, Ordering::SeqCst);
    let done = Arc::new(AtomicBool::new(false));
    let t = {
        let (c, done) = (c.clone(), done.clone());
        std::thread::spawn(move || {
            let r = c.run_once(&mut |_| {}).unwrap();
            done.store(true, Ordering::SeqCst);
            r
        })
    };
    std::thread::sleep(Duration::from_millis(400));
    assert!(!done.load(Ordering::SeqCst), "must wait for the UI");
    assert!(!stored(
        &c,
        &expected(
            &c,
            my(),
            110.0,
            GPoint::new(50.61, 10.71),
            110.0,
            Band::B144M
        )
    ));
    f.paths.ui_busy.store(false, Ordering::SeqCst);
    assert!(matches!(
        t.join().unwrap(),
        RunOutcome::Finished { created: 3, .. }
    ));
}

#[test]
fn calculator_cancel() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    let c = Arc::new(calc(&f, settings(my())));
    // blocked on the busy UI: cancelling must end it at once
    f.paths.ui_busy.store(true, Ordering::SeqCst);
    let t = {
        let c = c.clone();
        std::thread::spawn(move || {
            let mut msgs = Vec::new();
            let r = c.run_once(&mut |m| msgs.push(m)).unwrap();
            (r, msgs)
        })
    };
    std::thread::sleep(Duration::from_millis(100));
    c.cancel.store(true, Ordering::Relaxed);
    let (r, msgs) = t.join().unwrap();
    assert_eq!(r, RunOutcome::Cancelled);
    assert_eq!(msgs.last().unwrap(), "GLOBE cancelled.");
    // a cancelled run must not mark the station table as done
    assert!(!c.paths.data_dir.join("calculator-globe.stamp").exists());
}

#[test]
fn calculator_uses_best_case_position_and_elevation() {
    let f = fixture();
    let centre =
        |loc: &str| maidenhead::gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle).unwrap();
    // at the centre of their squares both stations are only as exact as the locator
    let (my_c, dx_c) = (centre("JO50LL"), centre("JO50NK"));
    station(&f.paths, "DL1AAA", dx_c);
    let mut s = settings(my_c);
    s.path_best_case_elevation = true;
    let c = calc(&f, s);
    run(&c);
    let best = |c: GPoint| {
        let r = maidenhead::bounds_from_loc(
            &maidenhead::loc_from_lat_lon(c.lat, c.lon, false, 3, false)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        GPoint::new(r.min_lat + 0.01, r.min_lon + 0.02)
    };
    // 300 m maximum + 10 m antenna at both ends, positions moved to the maximum
    let p = expected(&c, best(my_c), 310.0, best(dx_c), 310.0, Band::B144M);
    assert!(stored(&c, &p));
    // without the shift the paths would be stored at the original positions
    assert!(!stored(
        &c,
        &expected(&c, my_c, 110.0, dx_c, 110.0, Band::B144M)
    ));
}

#[test]
fn calculator_reads_my_per_path() {
    let f = fixture();
    let d = GPoint::new(50.61, 10.71);
    station(&f.paths, "DL1AAA", d);
    let c = Arc::new(calc(&f, settings(my())));
    let new_my = GPoint::new(50.45, 10.4);
    // My moves in the main window while the calculator runs: after its first path
    let mut changed = false;
    let mut msgs = Vec::new();
    let r = c
        .run_once(&mut |m| {
            if m.contains("calculating path") && !changed {
                changed = true;
                f.paths.set_my_station(MyStation {
                    call: "DL2ALF".into(),
                    lat: new_my.lat,
                    lon: new_my.lon,
                });
            }
            msgs.push(m);
        })
        .unwrap();
    assert!(matches!(r, RunOutcome::Finished { created: 3, .. }));
    // bands run in ascending order: the first path used the old My, the next ones the new My
    assert!(stored(
        &c,
        &expected(&c, my(), 110.0, d, 110.0, Band::B144M)
    ));
    assert!(stored(
        &c,
        &expected(&c, new_my, 110.0, d, 110.0, Band::B432M)
    ));
    assert!(stored(
        &c,
        &expected(&c, new_my, 110.0, d, 110.0, Band::B1_2G)
    ));
    assert!(!stored(
        &c,
        &expected(&c, my(), 110.0, d, 110.0, Band::B432M)
    ));
}

#[test]
fn calculator_reruns_when_the_qrv_table_changed() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    f.paths
        .stations
        .set_table_timestamp("QRV", stamp())
        .unwrap();
    let c = calc(&f, settings(my()));
    assert!(matches!(run(&c).0, RunOutcome::Finished { .. }));
    assert_eq!(run(&c).0, RunOutcome::Skipped);
    // a QRV import: the Location table is untouched
    f.paths
        .stations
        .set_table_timestamp("QRV", stamp() + chrono::Duration::hours(2))
        .unwrap();
    assert!(matches!(run(&c).0, RunOutcome::Finished { found: 3, .. }));
}

#[test]
fn calculator_leaves_the_near_field_suppression_alone() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    f.paths.engine.set_near_field_suppression_m(123.0);
    let mut s = settings(my());
    s.path_near_field_suppression_m = 456.0;
    let c = calc(&f, s);
    assert!(matches!(run(&c).0, RunOutcome::Finished { created: 3, .. }));
    assert_eq!(f.paths.engine.near_field_suppression_m(), 123.0);
}

#[test]
fn calculator_stops_on_incomplete_elevation_data_without_stamp() {
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    // the area reaches into JO60, which has no tiles and is not known as sea
    let mut s = settings(my());
    s.area.max_lon = 12.5;
    let c = calc(&f, s);
    let (r, msgs) = run(&c);
    assert_eq!(r, RunOutcome::Incomplete { missing: 1 });
    assert!(msgs.iter().any(|m| m.contains("incomplete")), "{msgs:?}");
    assert!(!c.paths.data_dir.join("calculator-globe.stamp").exists());
    assert!(!stored(
        &c,
        &expected(
            &c,
            my(),
            110.0,
            GPoint::new(50.61, 10.71),
            110.0,
            Band::B144M
        )
    ));
    // JO60 recorded as sea (as the updater does on a 404): complete, the run calculates
    std::fs::write(c.paths.data_dir.join("globe-sea-squares.txt"), "JO60\n").unwrap();
    assert!(matches!(run(&c).0, RunOutcome::Finished { created: 3, .. }));
    assert!(c.paths.data_dir.join("calculator-globe.stamp").exists());
}

#[test]
fn calculator_writes_no_stamp_while_paths_stay_unstored() {
    let f = fixture();
    // My outside the area: a path with a missing tile ends outside the bounds and is invalid
    // (ElevationDatabase.cs:1485-1498); remove one tile on the way to the station
    f.paths
        .store
        .with_conn(MODEL, |conn| {
            conn.execute("DELETE FROM Elevation WHERE TileIndex = 'JO50HK'", [])?;
            Ok(())
        })
        .unwrap();
    let mut s = settings(GPoint::new(50.45, 10.05));
    s.area.min_lon = 10.4;
    station(&f.paths, "DL1AAA", GPoint::new(50.45, 10.8));
    let c = calc(&f, s);
    let (r, _) = run(&c);
    assert!(
        matches!(r, RunOutcome::Finished { unsaved, .. } if unsaved > 0),
        "{r:?}"
    );
    assert!(!c.paths.data_dir.join("calculator-globe.stamp").exists());
}

#[test]
fn calculator_reruns_when_my_moved() {
    let f = fixture();
    let d = GPoint::new(50.61, 10.71);
    station(&f.paths, "DL1AAA", d);
    let c = calc(&f, settings(my()));
    assert!(matches!(run(&c).0, RunOutcome::Finished { created: 3, .. }));
    assert_eq!(run(&c).0, RunOutcome::Skipped);
    // My moves (Options OK, main window): the paths from the new position are missing
    let new_my = GPoint::new(50.45, 10.4);
    f.paths.set_my_station(MyStation {
        call: "DL2ALF".into(),
        lat: new_my.lat,
        lon: new_my.lon,
    });
    assert!(matches!(run(&c).0, RunOutcome::Finished { created: 3, .. }));
    assert!(stored(
        &c,
        &expected(&c, new_my, 110.0, d, 110.0, Band::B144M)
    ));
}

#[test]
fn user_qrv_write_leaves_the_import_marker_alone() {
    use airscout::app::{RealServices, Services};
    let f = fixture();
    let before = f.paths.stations.table_timestamp("QRV").unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut svc = RealServices::new(f.paths.clone(), tx);
    let q =
        as_stations::Qrv::new("DL2ALF", "JO50MM", Band::B144M, 25.0, 0.0, 0.0, Utc::now()).unwrap();
    assert_eq!(svc.qrv_write(&[q]).unwrap(), 1);
    // "QRV" is the stations updater's last-import marker: a user edit must not make it skip
    // the server's qrv.zip (304 + needs_import false)
    assert_eq!(f.paths.stations.table_timestamp("QRV").unwrap(), before);
    let edit = f
        .paths
        .stations
        .table_timestamp(as_stations::QRV_USER_EDIT_MARKER)
        .unwrap();
    assert!(edit.is_some(), "user edit marker not set");
}

#[test]
fn calculator_reruns_after_a_user_qrv_edit() {
    use airscout::app::{RealServices, Services};
    let f = fixture();
    station(&f.paths, "DL1AAA", GPoint::new(50.61, 10.71));
    let c = calc(&f, settings(my()));
    assert!(matches!(run(&c).0, RunOutcome::Finished { .. }));
    assert_eq!(run(&c).0, RunOutcome::Skipped);
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut svc = RealServices::new(f.paths.clone(), tx);
    let q =
        as_stations::Qrv::new("DL2ALF", "JO50MM", Band::B144M, 25.0, 0.0, 0.0, Utc::now()).unwrap();
    assert_eq!(svc.qrv_write(&[q]).unwrap(), 1);
    assert!(matches!(run(&c).0, RunOutcome::Finished { .. }));
}
