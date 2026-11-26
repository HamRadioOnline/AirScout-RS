//! `RealServices::background_jobs`: the running-worker counter is cumulative across generations
//! and only decreases when a worker actually exits (no network responses: the blocked GETs are
//! held open by a test server).
use airscout::app::{RealServices, Services};
use airscout::services::{open_data, ServiceEvent};
use as_settings::{ElevationEnabled, Settings};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

/// A TCP server that accepts `n` connections and holds each open without responding, so a worker's
/// blocking GET stays in flight until the connections are closed (when `release` is dropped).
fn holding_server(n: usize) -> (String, Sender<()>, Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}/", listener.local_addr().unwrap());
    let (accepted_tx, accepted_rx) = channel();
    let (release_tx, release_rx) = channel::<()>();
    std::thread::spawn(move || {
        let mut conns = Vec::new();
        for _ in 0..n {
            match listener.accept() {
                Ok((sock, _)) => {
                    accepted_tx.send(()).unwrap();
                    conns.push(sock);
                }
                Err(_) => break,
            }
        }
        // hold the sockets open until the test drops `release_tx`
        let _ = release_rx.recv();
        drop(conns);
    });
    (addr, release_tx, accepted_rx)
}

/// A `Settings` whose stations updater blocks forever on `stations_url` while every other worker
/// (aircraft) fails at once. Elevation is fully disabled so no `bg-*` model updater (a real HTTP
/// download) or calculator runs, and the map preloader is gated off by the disabled models.
fn settings(stations_url: String) -> Settings {
    let mut s = Settings::default();
    s.background.update_on_startup = true;
    s.background.update_periodically = false;
    s.background.calculations_enable = false;
    s.background.stations_url = stations_url;
    s.background.aircraft_url = "http://127.0.0.1:1/x".into();
    s.elevation_enabled = ElevationEnabled {
        globe: false,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    s
}

/// The workers started by `settings`: `bg-stations` (blocked in the GET), `bg-aircraft` (fails at
/// once) and `bg-positions` (the always-on maintainer). The stuck stations worker keeps the count
/// above zero until the server releases it (#155).
#[test]
fn background_jobs_is_cumulative_across_generations() {
    let (blocked, release, accepted) = holding_server(1);
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, _rx) = channel::<ServiceEvent>();
    let mut svc = RealServices::new(paths, tx);

    // generation 1: its stations worker connects and blocks in the GET
    svc.start_background(&settings(blocked.clone()), 1);
    accepted
        .recv_timeout(Duration::from_secs(10))
        .expect("gen-1 stations worker connected");
    assert!(svc.background_jobs() >= 1, "a worker is running");

    // stop is signal-only: the stuck worker is still counted (it cannot see the cancel)
    svc.stop_background();
    assert!(
        svc.background_jobs() >= 1,
        "stop must not zero the count while a worker is still in flight"
    );

    // generation 2: its stations worker fails at once, but the stuck gen-1 worker must stay
    // counted alongside gen-2's maintainer
    svc.start_background(&settings("http://127.0.0.1:1/x".into()), 2);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let n = svc.background_jobs();
        if n <= 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "gen-2 workers never settled: {n}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    // gen-1 stuck worker (1) + gen-2 maintainer (1): the older generation was not overwritten
    assert_eq!(
        svc.background_jobs(),
        2,
        "the stuck gen-1 worker must stay counted across the restart"
    );

    // release the blocked GET so the stuck worker exits and the count reaches zero
    drop(release);
    svc.stop_background();
    let deadline = Instant::now() + Duration::from_secs(10);
    while svc.background_jobs() > 0 {
        assert!(Instant::now() < deadline, "workers never exited");
        std::thread::sleep(Duration::from_millis(5));
    }
}
