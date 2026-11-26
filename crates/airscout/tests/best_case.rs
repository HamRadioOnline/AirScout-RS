//! `PathService`: best-case position and the near-field suppression (no network).
use airscout::services::{open_data, PathRequest, PathService, PathTarget, ServiceEvent};
use as_elevation::{ElevationModel, ElevationTile, LocalObstruction};
use as_geo::latlon::{bearing, GPoint};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::{ElevationEnabled, Settings};
use as_stations::default_antenna_height;
use chrono::{TimeZone, Utc};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// Tiles of JO50 with 100 m everywhere except the grid cell at the south-west corner, which is
/// 300 m and holds the tile maximum (0.01 deg north and 0.02 deg east of the corner).
fn tiles() -> Vec<ElevationTile> {
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
                    vec![300, 100, 100, 100],
                    Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap(),
                )
                .unwrap(),
            );
        }
    }
    v
}

fn settings(best_case: bool, nfs: f64) -> Settings {
    let mut s = Settings {
        path_best_case_elevation: best_case,
        path_near_field_suppression_m: nfs,
        ..Settings::default()
    };
    s.gui.elevation_download = false;
    s.elevation_enabled = ElevationEnabled {
        globe: true,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    s
}

fn request(s: Settings, my: GPoint, dx: GPoint) -> PathRequest {
    PathRequest {
        id: 1,
        my,
        my_call: "DL2ALF".into(),
        targets: vec![PathTarget {
            call: "DL1AAA".into(),
            pos: dx,
        }],
        settings: s,
    }
}

fn first_path(rx: &Receiver<ServiceEvent>) -> as_propagation::PropagationPath {
    let mut found = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(30)).expect("event") {
            ServiceEvent::PathReady { path, .. } => found = Some(path),
            ServiceEvent::PathFailed { error, .. } => panic!("path failed: {error}"),
            ServiceEvent::PathsDone { .. } => return found.expect("a path"),
            _ => {}
        }
    }
}

fn run(
    best_case: bool,
    nfs: f64,
    engine_nfs: f64,
) -> (as_propagation::PropagationPath, f64, GPoint) {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .store
        .insert_tiles_atomic(ElevationModel::Globe, &tiles())
        .unwrap();
    paths.engine.set_near_field_suppression_m(engine_nfs);
    let centre =
        |l: &str| maidenhead::gpoint_from_loc(l, PositionInRectangle::MiddleMiddle).unwrap();
    let my = centre("JO50LL");
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths.clone(), tx);
    svc.request(request(
        settings(best_case, nfs),
        my,
        GPoint::new(50.61, 10.71),
    ));
    let p = first_path(&rx);
    svc.stop();
    (p, paths.engine.near_field_suppression_m(), my)
}

#[test]
fn compute_applies_the_best_case_shift() {
    let (p, _, my) = run(true, 0.0, 0.0);
    let r = maidenhead::bounds_from_loc("JO50LL").unwrap();
    // My moves to the tile maximum; the height is the ground there (300 m) plus the 10 m default
    assert_eq!(p.params.lat1, r.min_lat + 0.01);
    assert_eq!(p.params.lon1, r.min_lon + 0.02);
    assert_eq!(p.params.h1, 310.0);
    // DX is not at a locator centre: it stays
    assert_eq!((p.params.lat2, p.params.lon2), (50.61, 10.71));
    // setting off: the stored position
    let (p, _, _) = run(false, 0.0, 0.0);
    assert_eq!((p.params.lat1, p.params.lon1), (my.lat, my.lon));
}

#[test]
fn compute_leaves_the_near_field_suppression_alone() {
    let (_, engine_nfs, _) = run(false, 456.0, 123.0);
    assert_eq!(engine_nfs, 123.0);
}

#[test]
fn compute_finds_the_obstruction_at_the_unshifted_position() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .store
        .insert_tiles_atomic(ElevationModel::Globe, &tiles())
        .unwrap();
    let centre =
        |l: &str| maidenhead::gpoint_from_loc(l, PositionInRectangle::MiddleMiddle).unwrap();
    let my = centre("JO50LL");
    let dx = GPoint::new(50.61, 10.71);
    // a 300 m obstacle 5 km out, only in the sector from the unshifted position to DX: a lookup
    // at the best-case shifted position would find nothing (#105)
    let i = bearing(my.lat, my.lon, dx.lat, dx.lon)
        .rem_euclid(360.0)
        .round() as usize
        % 360;
    let mut dist = [0.0; 360];
    let mut height = [0.0; 360];
    dist[i] = 5000.0;
    height[i] = 300.0;
    let o = LocalObstruction::new(my.lat, my.lon, Some(dist), Some(height));
    paths
        .store
        .local_obstruction_insert_or_update_if_newer(ElevationModel::Globe, &o, Utc::now())
        .unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths.clone(), tx);
    svc.request(request(settings(true, 0.0), my, dx));
    let p = first_path(&rx);
    svc.stop();
    // best case moved My to the tile maximum ...
    let r = maidenhead::bounds_from_loc("JO50LL").unwrap();
    assert_eq!(p.params.lat1, r.min_lat + 0.01);
    assert_eq!(p.params.lon1, r.min_lon + 0.02);
    // ... but the obstruction was found at the unshifted settings position, against the
    // unshifted bearing, so the path carries the entered obstacle (#105)
    let h = default_antenna_height(Settings::default().band);
    let expected = ((300.0 - h) / 5000.0f64).atan();
    assert!((p.params.local_obstruction - expected).abs() < 1e-12);
}
