//! Background updaters and status LEDs (mockito, no network).
use airscout::background::{led_color, strip_query, BackgroundService, Led, LedState, LEDS};
use airscout::services::{open_data, ServiceEvent};
use as_elevation::ElevationModel;
use as_settings::{ElevationEnabled, Settings};
use eframe::egui::Color32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(20);

fn none_enabled() -> ElevationEnabled {
    ElevationEnabled {
        globe: false,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    }
}

/// Settings with all URLs on `url`, an area inside square JO50 and only GLOBE enabled.
fn settings(url: &str) -> Settings {
    let mut s = Settings::default();
    s.background.stations_url = format!("{url}/stations/");
    s.background.aircraft_url = format!("{url}/aircraft/");
    s.gui.elevation_base_url = url.to_string();
    s.area.min_lat = 50.2;
    s.area.max_lat = 50.8;
    s.area.min_lon = 10.2;
    s.area.max_lon = 10.8;
    s.elevation_enabled = none_enabled();
    s.elevation_enabled.globe = true;
    s
}

/// Wait for `BackgroundIdle(led)`; returns the last LED state seen for it.
fn wait_idle(rx: &Receiver<ServiceEvent>, led: Led) -> (LedState, String) {
    let end = Instant::now() + WAIT;
    let mut last = (LedState::Unknown, String::new());
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left).expect("no BackgroundIdle in time") {
            ServiceEvent::Led {
                led: l,
                state,
                text,
                ..
            } if l == led => last = (state, text),
            ServiceEvent::BackgroundIdle { led: l, .. } if l == led => return last,
            _ => {}
        }
    }
}

type Started = (tempfile::TempDir, BackgroundService, Receiver<ServiceEvent>);

/// Wait until all of `leds` reported idle; their last LED states.
fn wait_idle_all(
    rx: &Receiver<ServiceEvent>,
    leds: &[Led],
) -> std::collections::HashMap<Led, LedState> {
    let end = Instant::now() + WAIT;
    let mut states = std::collections::HashMap::new();
    let mut idle = std::collections::HashSet::new();
    while !leds.iter().all(|l| idle.contains(l)) {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::Led { led, state, .. }) => {
                states.insert(led, state);
            }
            Ok(ServiceEvent::BackgroundIdle { led, .. }) => {
                idle.insert(led);
            }
            Ok(_) => {}
            Err(e) => panic!("jobs not idle: {e}"),
        }
    }
    states
}

fn start_with_store(s: &Settings) -> (Started, Arc<as_elevation::ElevationStore>) {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let store = paths.store.clone();
    let (tx, rx) = channel();
    let svc = BackgroundService::start(
        paths,
        s,
        tx,
        Arc::new(AtomicBool::new(false)),
        7,
        Arc::new(AtomicUsize::new(0)),
    );
    ((dir, svc, rx), store)
}

fn start(s: &Settings) -> Started {
    start_with_store(s).0
}

fn globe_zip() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/elevation/GLOBE/JO/JO50.zip"),
    )
    .unwrap()
}

#[test]
fn led_color_mapping() {
    assert_eq!(led_color(LedState::Error), Color32::from_rgb(255, 0, 0));
    assert_eq!(led_color(LedState::UpToDate), Color32::from_rgb(0, 128, 0));
    assert_eq!(led_color(LedState::Complete), Color32::from_rgb(0, 0, 255));
    assert_eq!(
        led_color(LedState::Updating),
        Color32::from_rgb(255, 215, 0)
    );
    assert_eq!(led_color(LedState::Empty), Color32::BLACK);
    assert_eq!(
        led_color(LedState::Unknown),
        Color32::from_rgb(221, 160, 221)
    );
    assert_eq!(LEDS.len(), 7);
    assert_eq!(LEDS[0], Led::Aircraft);
    assert_eq!(LEDS[6], Led::Elevation(ElevationModel::Aster1));
}

#[test]
fn strip_query_removes_credentials() {
    assert_eq!(
        strip_query("GET http://h/x?key=SECRET failed (code 5)"),
        "GET http://h/x failed (code 5)"
    );
    assert_eq!(
        strip_query("GET https://u:secret@h/x?k=S failed; or https://v@h2/y"),
        "GET https://h/x failed; or https://h2/y"
    );
}

#[test]
fn strip_query_redacts_passwords_with_unescaped_delimiters() {
    for (url, rest) in [
        ("https://user:pa/ss@host/x", "host/x"),
        ("https://u:p?q@host/", "host/"),
        ("https://u:p#f@host/", "host/"),
        ("https://u:p)x@host/y", "host/y"),
        ("https://u:p@w@host/y", "host/y"),
    ] {
        let out = strip_query(&format!("Downloading history from {url} failed"));
        assert_eq!(
            out,
            format!("Downloading history from https://{rest} failed"),
            "{url}"
        );
        for secret in ["pa/ss", "p?q", "p#f", "p)x", "user", "u:"] {
            assert!(!out.contains(secret), "{url}: {out}");
        }
    }
    // normal URLs and texts stay as they are
    for text in [
        "GET https://host/x/y.zip failed (code 5)",
        "see (http://host:8080/a#frag) and https://h2/",
        "no url here, mail me@example",
    ] {
        assert_eq!(strip_query(text), text);
    }
}

#[test]
fn startup_run_sets_uptodate_led() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(globe_zip())
        .create();
    let ((_dir, svc, rx), store) = start_with_store(&settings(&server.url()));
    let (state, text) = wait_idle(&rx, Led::Elevation(ElevationModel::Globe));
    assert_eq!(state, LedState::UpToDate, "{text}");
    // the updater only downloads: validity of paths stays with the path service
    assert!(!store.is_complete(ElevationModel::Globe));
    svc.stop();
}

#[test]
fn updater_error_sets_error_led() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/stations/locations.zip")
        .with_status(500)
        .create();
    let (_dir, svc, rx) = start(&settings(&server.url()));
    let (state, text) = wait_idle(&rx, Led::Stations);
    assert_eq!(state, LedState::Error, "{text}");
    assert!(text.contains("Database has errors."));
    svc.stop();
}

#[test]
fn disabled_model_not_updated() {
    let mut server = mockito::Server::new();
    let globe = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(globe_zip())
        .expect(1)
        .create();
    let srtm3 = server
        .mock("GET", mockito::Matcher::Regex("SRTM3".into()))
        .expect(0)
        .create();
    let (_dir, svc, rx) = start(&settings(&server.url()));
    // every job of the run must be done before the mocks are checked
    let mut idle = std::collections::BTreeSet::new();
    let end = Instant::now() + WAIT;
    while idle.len() < 3 {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::BackgroundIdle { led: l, .. }) => {
                idle.insert(l);
            }
            Ok(_) => {}
            Err(e) => panic!("jobs not idle: {e}"),
        }
    }
    globe.assert();
    srtm3.assert();
    svc.stop();
}

#[test]
fn cancel_stops_updaters_quickly() {
    // periodic job with a long period: the thread sleeps and must end soon after the cancel
    let mut s = settings("http://127.0.0.1:1");
    s.background.update_on_startup = false;
    s.background.update_periodically = true;
    let (_dir, svc, rx) = start(&s);
    wait_idle(&rx, Led::Stations);
    let running = svc.running_jobs();
    assert!(running.load(Ordering::SeqCst) > 0);
    svc.stop();
    // the period is long, so ending at all proves the cancel; the bound leaves room for a
    // loaded CI runner finishing a refused request first
    let end = Instant::now() + Duration::from_secs(5);
    while running.load(Ordering::SeqCst) > 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(running.load(Ordering::SeqCst), 0, "threads still alive");
}

#[test]
fn cancel_during_slow_request_silences_service() {
    // A blocking request cannot be interrupted: the thread is detached and reports nothing.
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/stations/locations.zip")
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_secs(3));
            w.write_all(b"x")
        })
        .create();
    let mut s = settings(&server.url());
    s.elevation_enabled = none_enabled();
    let (_dir, svc, rx) = start(&s);
    // Stations job is in its request
    let end = Instant::now() + WAIT;
    loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::Led {
                led: Led::Stations,
                state: LedState::Updating,
                text,
                ..
            }) if text.contains("Updating locations") => break,
            Ok(_) => {}
            Err(e) => panic!("no stations progress: {e}"),
        }
    }
    svc.stop();
    while rx.try_recv().is_ok() {}
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "event after cancel"
        );
    }
}

#[test]
fn events_carry_the_generation() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(globe_zip())
        .create();
    let (_dir, svc, rx) = start(&settings(&server.url()));
    let end = Instant::now() + WAIT;
    loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::Led { generation, .. }) => {
                assert_eq!(generation, 7);
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("no LED event: {e}"),
        }
    }
    svc.stop();
}

#[test]
fn startup_and_periodic_together_run_once_and_maintainer_always_runs() {
    // upstream `if OnStartup {RUNONCE} else if Periodically {..}`: both set means once
    let server = mockito::Server::new();
    let mut s = settings(&server.url());
    s.background.update_on_startup = true;
    s.background.update_periodically = true;
    s.planes_keep_history = false;
    let (dir, svc, rx) = start(&s);
    wait_idle_all(
        &rx,
        &[
            Led::Aircraft,
            Led::Stations,
            Led::Elevation(ElevationModel::Globe),
        ],
    );
    // only the periodic position maintainer is left
    let running = svc.running_jobs();
    let end = Instant::now() + Duration::from_secs(5);
    while running.load(Ordering::SeqCst) != 1 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(running.load(Ordering::SeqCst), 1);
    // the maintainer ran although the history is off
    let end = Instant::now() + Duration::from_secs(5);
    let db = dir.path().join("aircraftpositions.db3");
    while !db.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(db.exists());
    svc.stop();
}

#[test]
fn nothing_runs_without_startup_or_periodic_but_idle_is_reported() {
    let server = mockito::Server::new();
    let mut s = settings(&server.url());
    s.background.update_on_startup = false;
    s.background.update_periodically = false;
    let (_dir, svc, rx) = start(&s);
    let states = wait_idle_all(
        &rx,
        &[
            Led::Aircraft,
            Led::Stations,
            Led::Elevation(ElevationModel::Globe),
        ],
    );
    assert!(states.is_empty(), "no job may report a state");
    svc.stop();
}

#[test]
fn cancel_stops_elevation_download_after_current_square() {
    // area JO50..JO60: JO50 is slow; cancel during it, JO60 must never be requested
    let mut server = mockito::Server::new();
    let slow = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(1500));
            w.write_all(b"x")
        })
        .expect(1)
        .create();
    let next = server
        .mock("GET", "/GLOBE/JO/JO60.zip")
        .with_status(404)
        .expect(0)
        .create();
    let mut s = settings(&server.url());
    s.area.max_lon = 13.0;
    let (_dir, svc, rx) = start(&s);
    let end = Instant::now() + WAIT;
    loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::Led {
                text,
                led: Led::Elevation(_),
                ..
            }) if text.contains("JO50") => break,
            Ok(_) => {}
            Err(e) => panic!("no elevation progress: {e}"),
        }
    }
    let running = svc.running_jobs();
    svc.stop();
    // the thread leaves after the request in flight and goes no further
    let end = Instant::now() + Duration::from_secs(5);
    while running.load(Ordering::SeqCst) > 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(running.load(Ordering::SeqCst), 0);
    slow.assert();
    next.assert();
}

#[test]
fn elevation_download_off_downloads_nothing() {
    let mut server = mockito::Server::new();
    let globe = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(globe_zip())
        .expect(0)
        .create();
    let mut s = settings(&server.url());
    s.gui.elevation_download = false;
    let (_dir, svc, rx) = start(&s);
    let (state, text) = wait_idle(&rx, Led::Elevation(ElevationModel::Globe));
    // nothing stored and nothing downloaded: the LED says what is missing
    assert_eq!(state, LedState::Empty, "{text}");
    assert!(text.contains("missing squares: JO50"), "{text}");
    globe.assert();
    svc.stop();
}

/// Wait for a `Calculations` text of the map preloader that satisfies `done`; all its texts.
fn wait_preloader(rx: &Receiver<ServiceEvent>, done: impl Fn(&str) -> bool) -> Vec<String> {
    let end = Instant::now() + WAIT;
    let mut texts = Vec::new();
    loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::Calculations { text, generation }) if text.contains("Preload") => {
                assert_eq!(generation, 7);
                let stop = done(&text);
                texts.push(text);
                if stop {
                    return texts;
                }
            }
            Ok(_) => {}
            Err(e) => panic!("no preloader text in time: {e}; got {texts:?}"),
        }
    }
}

#[test]
fn map_preloader_fills_store_from_mirror() {
    let mut server = mockito::Server::new();
    let tiles = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/osm/5/\d+/\d+\.png$".into()),
        )
        .with_body(b"\x89PNG\r\n\x1a\nTEST-TILE")
        .expect_at_least(1)
        .create();
    let mut s = settings(&server.url());
    s.gui.map_preloader_enabled = true;
    s.gui.map_preloader_max_zoom = 5;
    s.background.map_mirror_url = format!("{}/osm/", server.url());
    s.background.calculations_enable = false;
    let a = &s.area;
    let area = as_geo::latlon::GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
    let total = as_maps::preload_tile_count(&area, 5, 5);
    assert!(total >= 1);
    let (dir, svc, rx) = start(&s);
    let texts = wait_preloader(&rx, |t| t.starts_with("MapPreloader finished"));
    svc.stop();
    tiles.assert();
    assert_eq!(texts[0], "MapPreloader started.");
    assert_eq!(texts[1], "MapPreloader getting tiles from database.");
    assert!(texts[2].starts_with("Preloading /5/"), "{texts:?}");
    assert_eq!(
        texts.last().unwrap(),
        &format!("MapPreloader finished, total  {total} tile(s), 0 left.")
    );
    let store = as_maps::TileStore::open(&dir.path().join("maptiles.gmdb")).unwrap();
    assert_eq!(store.count().unwrap(), total);
}

#[test]
fn map_preloader_other_provider_gets_notice_only() {
    let mut server = mockito::Server::new();
    let none = server
        .mock("GET", mockito::Matcher::Regex(r"\.png$".into()))
        .expect(0)
        .create();
    let mut s = settings(&server.url());
    s.gui.map_preloader_enabled = true;
    s.gui.map_provider = "OpenTopoMap".into();
    s.background.map_mirror_url = format!("{}/osm/", server.url());
    s.background.calculations_enable = false;
    let (_dir, svc, rx) = start(&s);
    let texts = wait_preloader(&rx, |t| t.contains("no mirror"));
    assert_eq!(
        texts,
        ["MapPreloader: no mirror for provider OpenTopoMap, nothing preloaded."]
    );
    // the elevation updater has run meanwhile; no tile was requested
    wait_idle(&rx, Led::Elevation(ElevationModel::Globe));
    svc.stop();
    none.assert();
}
