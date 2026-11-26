//! `BackgroundService`: when the calculators start (no network: the URLs refuse connections).
use airscout::background::BackgroundService;
use airscout::services::{open_data, ServiceEvent};
use as_settings::{ElevationEnabled, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn settings(on_startup: bool, periodically: bool, calculations: bool) -> Settings {
    let mut s = Settings::default();
    // port 1 refuses at once
    let dead = "http://127.0.0.1:1/x";
    s.background.update_on_startup = on_startup;
    s.background.update_periodically = periodically;
    s.background.calculations_enable = calculations;
    s.background.stations_url = dead.into();
    s.background.aircraft_url = dead.into();
    s.gui.elevation_base_url = dead.into();
    // one GLOBE square: Windows reports each refused connect only after ~2 s, so the default
    // area (about 1100 squares, each one refused download) keeps the updater busy for over
    // half an hour before the calculator may start
    s.area.min_lat = 50.0;
    s.area.max_lat = 51.0;
    s.area.min_lon = 10.0;
    s.area.max_lon = 11.0;
    s.elevation_enabled = ElevationEnabled {
        globe: true,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    s
}

/// Events within `wait`, as the texts of the `Calculations` events and the generations seen.
fn calculations(
    rx: &Receiver<ServiceEvent>,
    wait: Duration,
    until_finished: bool,
) -> Vec<(u64, String)> {
    let end = Instant::now() + wait;
    let mut v = Vec::new();
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok(ServiceEvent::Calculations { generation, text }) = rx.recv_timeout(left) {
            let done = text.ends_with("finished.") || text.contains("incomplete");
            v.push((generation, text));
            if until_finished && done {
                break;
            }
        }
    }
    v
}

fn start(
    s: &Settings,
) -> (
    tempfile::TempDir,
    BackgroundService,
    Receiver<ServiceEvent>,
    Arc<AtomicBool>,
) {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let svc = BackgroundService::start(paths, s, tx, cancel.clone(), 7);
    (dir, svc, rx, cancel)
}

#[test]
fn no_calculator_when_updates_are_off() {
    // C# starts the calculators only inside the OnStartup / Periodically branches
    let (_d, svc, rx, cancel) = start(&settings(false, false, true));
    let got = calculations(&rx, Duration::from_millis(1500), false);
    cancel.store(true, Ordering::Relaxed);
    drop(svc);
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn calculator_starts_after_the_updaters_with_the_generation() {
    let (_d, svc, rx, cancel) = start(&settings(true, false, true));
    // the loop returns at the first finished/incomplete event; the bound covers slow runners
    let got = calculations(&rx, Duration::from_secs(60), true);
    cancel.store(true, Ordering::Relaxed);
    drop(svc);
    assert_eq!(
        got.first().map(|g| g.1.as_str()),
        Some("GLOBE started."),
        "{got:?}"
    );
    assert!(got.iter().all(|g| g.0 == 7));
    // the elevation download failed: the calculator stops on the incomplete data
    assert!(got.last().unwrap().1.contains("incomplete"), "{got:?}");
}

#[test]
fn no_calculator_when_calculations_are_off() {
    let (_d, svc, rx, cancel) = start(&settings(true, false, false));
    let got = calculations(&rx, Duration::from_millis(1500), false);
    cancel.store(true, Ordering::Relaxed);
    drop(svc);
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn background_calculator_stores_paths_after_the_download() {
    use airscout::calculator::{Calculator, RunOutcome, Timing};
    use airscout::services::MyStation;
    use as_elevation::ElevationModel;
    use as_stations::{GeoSource, Location};
    // the real flow: the elevation job downloads GLOBE JO50, then the calculator runs; no status
    // bit is set by hand
    let mut server = mockito::Server::new();
    let zip = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/elevation/GLOBE/JO/JO50.zip"),
    )
    .unwrap();
    let _m = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let mut s = settings(true, false, true);
    s.gui.elevation_base_url = server.url();
    s.area.min_lat = 50.2;
    s.area.max_lat = 50.8;
    s.area.min_lon = 10.2;
    s.area.max_lon = 10.8;
    s.my_call = "DL2ALF".into();
    s.my_lat = 50.5;
    s.my_lon = 10.5;
    s.band_settings
        .retain(|b, _| *b == as_geo::band::Band::B144M);
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_insert_or_update_if_newer(&Location::new(
            "DL1AAA",
            "JO50MO",
            50.6,
            10.7,
            GeoSource::FromUser,
            0,
            chrono::Utc::now(),
        ))
        .unwrap();
    paths.set_my_station(MyStation {
        call: s.my_call.clone(),
        lat: s.my_lat,
        lon: s.my_lon,
    });
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let svc = BackgroundService::start(paths.clone(), &s, tx, cancel.clone(), 7);
    let got = calculations(&rx, Duration::from_secs(30), true);
    cancel.store(true, Ordering::Relaxed);
    drop(svc);
    assert!(got.iter().any(|g| g.1.ends_with("finished.")), "{got:?}");
    // the background run stored its path: a fresh run finds it
    std::fs::remove_file(dir.path().join("calculator-globe.stamp")).unwrap();
    let c = Calculator {
        paths,
        settings: s,
        model: ElevationModel::Globe,
        cancel: Arc::new(AtomicBool::new(false)),
        timing: Timing::default(),
    };
    assert_eq!(
        c.run_once(&mut |_| {}).unwrap(),
        RunOutcome::Finished {
            found: 1,
            created: 0,
            unsaved: 0
        }
    );
}
