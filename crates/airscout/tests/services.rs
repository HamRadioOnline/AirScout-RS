//! Background services: path computation thread and feed worker wrapper.
use airscout::services::{
    open_data, FeedService, PathRequest, PathService, PathTarget, ServiceEvent,
};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_elevation::ElevationModel;
use as_feeds::worker::FeedEvent;
use as_geo::latlon::GPoint;
use as_settings::{ElevationEnabled, Settings};
use as_stations::{default_antenna_gain, default_antenna_height, Qrv};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

mod common;

const WAIT: Duration = Duration::from_secs(20);

fn settings() -> Settings {
    // the default enables GLOBE only
    let mut s = Settings::default();
    s.gui.elevation_download = false;
    s
}

fn request(dx: GPoint) -> PathRequest {
    PathRequest {
        id: 1,
        my: GPoint::new(50.2, 10.3),
        my_call: "DL2ALF".into(),
        targets: vec![PathTarget {
            call: "GB3MHZ".into(),
            pos: dx,
        }],
        settings: settings(),
    }
}

fn next_path(rx: &Receiver<ServiceEvent>) -> (as_propagation::PropagationPath, Vec<f64>) {
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::PathReady { path, terrain, .. } => return (path, terrain),
            ServiceEvent::PathFailed { error: e, .. } => panic!("path failed: {e}"),
            _ => {}
        }
    }
}

#[test]
fn path_request_produces_path() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    svc.request(request(GPoint::new(50.7, 11.6)));
    let (path, terrain) = next_path(&rx);
    assert!(path.valid);
    assert!(path.distance() > 50.0);
    let info = path.info_points().unwrap();
    // R1: one ground elevation per info point
    assert_eq!(terrain.len(), info.len());
    assert!(terrain.iter().any(|&e| e > 0.0));
    // antenna heights are the default 10 m above ground
    assert!(path.params.h1 >= 10.0);
    // no stored local obstruction
    assert_eq!(path.params.local_obstruction, f64::MIN);
    svc.stop();
}

#[test]
fn newest_request_wins() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    let dxs = [
        GPoint::new(50.5, 11.1),
        GPoint::new(50.6, 11.3),
        GPoint::new(50.7, 11.6),
    ];
    // worker blocked while all three are queued: only the newest may be computed
    svc.pause();
    for dx in dxs {
        svc.request(request(dx));
    }
    svc.resume();
    let (path, _) = next_path(&rx);
    assert_eq!(
        (path.params.lat2, path.params.lon2),
        (dxs[2].lat, dxs[2].lon)
    );
    // only the end marker of the newest request follows
    assert!(matches!(
        rx.recv_timeout(WAIT),
        Ok(ServiceEvent::PathsDone { id: 1 })
    ));
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    svc.stop();
}

#[test]
fn path_without_elevation_is_invalid_not_error() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    svc.request(request(GPoint::new(50.7, 12.5)));
    let mut status = Vec::new();
    let path = loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::Status(s) => status.push(s),
            ServiceEvent::PathNote { text, .. } => status.push(text),
            ServiceEvent::PathReady { path, .. } => break path,
            other => panic!("unexpected {other:?}"),
        }
    };
    assert!(!path.valid);
    let text = status.join("\n");
    assert!(text.contains("JO50") && text.contains("JO60"), "{text}");
    svc.stop();
}

#[test]
fn model_none_is_a_path_failure() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    let mut req = request(GPoint::new(50.7, 11.6));
    req.id = 7;
    req.settings.elevation_enabled = ElevationEnabled {
        globe: false,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    svc.request(req);
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::PathFailed {
                id,
                index,
                error: e,
            } => {
                assert_eq!((id, index), (7, 0));
                assert_eq!(e, "no elevation model enabled");
                break;
            }
            ServiceEvent::PathReady { .. } => panic!("no path expected"),
            _ => {}
        }
    }
    svc.stop();
}

#[test]
fn feed_error_reaches_status_and_keeps_cache() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/aircraft.json")
        .with_status(500)
        .expect_at_least(1)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let cache = Arc::new(PlaneInfoCache::new());
    cache.insert_or_update_if_newer(&PlaneInfo::new(
        chrono::Utc::now(),
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    ));
    let mut s = settings();
    s.feed = "Readsb".into();
    s.planes_interval_s = 1;
    let mut t = toml::Table::new();
    t.insert(
        "url".into(),
        format!("{}/aircraft.json", server.url()).into(),
    );
    s.feed_settings.insert("Readsb".into(), t.into());
    let (tx, rx) = channel();
    let svc = FeedService::start(&paths, &s, cache.clone(), tx, 1).unwrap();
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::Feed {
                event: FeedEvent::Error(e),
                ..
            } => {
                assert!(e.contains("500"), "{e}");
                break;
            }
            ServiceEvent::Feed { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(cache.latest("3C0001").is_some());
    svc.stop();
}

#[test]
fn two_feeds_run_in_parallel_and_share_the_cache() {
    let mut readsb = mockito::Server::new();
    let _m1 = readsb
        .mock("GET", "/aircraft.json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"now":1700000000.0,"aircraft":[{"hex":"3c0001","flight":"TEST1","lat":50.5,"lon":11.0,"alt_baro":37000,"gs":450.0,"track":245.0,"seen_pos":0.4}]}"#,
        )
        .expect_at_least(1)
        .create();
    let mut dump = mockito::Server::new();
    let _m2 = dump
        .mock("GET", "/data/aircraft.json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"now":1700000000.0,"aircraft":[{"hex":"4b0002","flight":"TEST2","lat":50.6,"lon":11.1,"altitude":37000,"speed":450.0,"track":245.0,"seen_pos":0.4}]}"#,
        )
        .expect_at_least(1)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let cache = Arc::new(PlaneInfoCache::new());
    let mut s = settings();
    s.feed = "Readsb".into();
    s.feed2 = "Dump1090".into();
    s.planes_interval_s = 1;
    let mut r = toml::Table::new();
    r.insert(
        "url".into(),
        format!("{}/aircraft.json", readsb.url()).into(),
    );
    s.feed_settings.insert("Readsb".into(), r.into());
    let mut d = toml::Table::new();
    d.insert(
        "url".into(),
        format!("{}/data/aircraft.json", dump.url()).into(),
    );
    s.feed_settings.insert("Dump1090".into(), d.into());
    let (tx, rx) = channel();
    let svc = FeedService::start(&paths, &s, cache.clone(), tx, 1).unwrap();
    let mut seen = std::collections::HashSet::new();
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::Feed {
                name,
                event: FeedEvent::Planes(_),
                ..
            } => {
                seen.insert(name);
                if seen.contains("Readsb") && seen.contains("Dump1090") {
                    break;
                }
            }
            ServiceEvent::Feed {
                name,
                event: FeedEvent::Error(e),
                ..
            } => panic!("{name}: {e}"),
            ServiceEvent::Feed { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(cache.latest("3C0001").is_some(), "Readsb plane missing");
    assert!(cache.latest("4B0002").is_some(), "Dump1090 plane missing");
    svc.stop();
}

#[test]
fn unknown_feed_reports_an_error_event() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let mut s = settings();
    s.feed = "NoSuchFeed".into();
    let (tx, rx) = channel();
    let svc = FeedService::start(&paths, &s, Arc::new(PlaneInfoCache::new()), tx, 1).unwrap();
    match rx.recv_timeout(WAIT).expect("event") {
        ServiceEvent::Feed {
            name,
            event: FeedEvent::Error(e),
            ..
        } => {
            assert_eq!(name, "NoSuchFeed");
            assert!(e.contains("unknown feed"), "{e}");
        }
        other => panic!("unexpected {other:?}"),
    }
    svc.stop();
}

#[test]
fn incomplete_request_after_complete_one_is_invalid_and_not_saved() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    // A: inside JO50, complete
    svc.request(request(GPoint::new(50.7, 11.6)));
    assert!(next_path(&rx).0.valid);
    // B: into JO60, which has no tiles, download off
    svc.request(request(GPoint::new(50.7, 12.5)));
    assert!(!next_path(&rx).0.valid);
    // an invalid path must not have been stored: the same request is still invalid
    svc.request(request(GPoint::new(50.7, 12.5)));
    assert!(!next_path(&rx).0.valid);
    svc.stop();
}

fn my_loc() -> String {
    as_geo::maidenhead::loc_from_lat_lon(50.2, 10.3, false, 3, true)
        .unwrap()
        .unwrap()
}

#[test]
fn stored_obstruction_enters_path_params() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    // 1000 m obstacle in 10 km in every direction, stored for My and the selected model
    let o =
        as_elevation::LocalObstruction::new(50.2, 10.3, Some([10_000.0; 360]), Some([1000.0; 360]));
    paths
        .store
        .local_obstruction_insert_or_update_if_newer(ElevationModel::Globe, &o, chrono::Utc::now())
        .unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    svc.request(request(GPoint::new(50.7, 11.6)));
    let (path, _) = next_path(&rx);
    // C#: o.GetObstruction(myqrv.AntennaHeight, bearing): the antenna height, not ground + antenna
    let h = default_antenna_height(Settings::default().band);
    let expected = ((1000.0 - h) / 10_000.0f64).atan();
    assert!((path.params.local_obstruction - expected).abs() < 1e-12);
    svc.stop();
}

#[test]
fn qrv_gain_reported_in_path_ready() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let band = Settings::default().band;
    let q = Qrv::new(
        "DL2ALF",
        &my_loc(),
        band,
        0.0,
        11.5,
        0.0,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(paths.stations.qrv_insert_or_update_if_newer(&q).unwrap());
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    svc.request(request(GPoint::new(50.7, 11.6)));
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::PathReady {
                my_gain_dbi,
                dx_gain_dbi,
                ..
            } => {
                // stored gain for My; the DX station is new, so the band default
                assert_eq!(my_gain_dbi, 11.5);
                assert_eq!(dx_gain_dbi, default_antenna_gain(band));
                break;
            }
            ServiceEvent::PathFailed { error, .. } => panic!("path failed: {error}"),
            _ => {}
        }
    }
    svc.stop();
}

#[test]
fn multi_targets_report_index_and_done() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let svc = PathService::spawn(paths, tx);
    let mut req = request(GPoint::new(50.5, 11.1));
    req.id = 5;
    req.targets.push(PathTarget {
        call: "DL0XYZ".into(),
        pos: GPoint::new(50.7, 11.6),
    });
    svc.request(req);
    let mut got = Vec::new();
    loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::PathReady {
                id,
                index,
                call,
                path,
                ..
            } => {
                assert_eq!(id, 5);
                got.push((index, call, path.params.lat2));
            }
            ServiceEvent::PathFailed { error, .. } => panic!("path failed: {error}"),
            ServiceEvent::PathsDone { id } => {
                assert_eq!(id, 5);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(
        got,
        [
            (0, "GB3MHZ".to_string(), 50.5),
            (1, "DL0XYZ".to_string(), 50.7)
        ]
    );
    svc.stop();
}

#[test]
fn finding_329_desktop_start_rejects_its_active_aircraft_listener() {
    let dir = tempfile::tempdir().unwrap();
    let paths = airscout::services::open_data(dir.path()).unwrap();
    let mut settings = Settings {
        feed: "AirScoutServer".into(),
        ..Default::default()
    };
    settings.server.activate = true;
    settings.web.port = 9872;
    let (tx, rx) = std::sync::mpsc::channel();
    let svc =
        FeedService::start(&paths, &settings, Arc::new(PlaneInfoCache::new()), tx, 1).unwrap();
    match rx.recv_timeout(WAIT).expect("event") {
        ServiceEvent::Feed {
            event: FeedEvent::Error(e),
            ..
        } => assert!(e.contains("this process"), "{e}"),
        other => panic!("unexpected {other:?}"),
    }
    svc.stop();
}
