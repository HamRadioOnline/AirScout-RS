//! The web service after stop and around a failed database open: a stopped API writes
//! nothing, a stopped service reports nothing, the aircraft database is opened again on
//! request, and an unchanged config keeps a server that is still starting.
#[path = "../../as-web/tests/common/mod.rs"]
mod http;

use airscout::services::{open_data, ServiceEvent};
use airscout::web_api::{WebApi, WebCtx};
use airscout::web_service::{keeps_server, WebService, WebState};
use as_aircraft::db::{airports_from_json, AircraftDb};
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use as_web::query::Query;
use as_web::route::{route, Route};
use as_web::server::{Api, WebConfig};
use http::get;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

/// Every wait returns early and gives up after this long.
const WAIT: Duration = Duration::from_secs(10);

fn ctx(dir: &Path) -> WebCtx {
    WebCtx {
        paths: open_data(dir).unwrap(),
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(Settings {
            my_call: "TE1ST".into(),
            ..Settings::default()
        })),
        aircraft: None,
        tmp_dir: dir.join("tmp"),
        stop: Arc::new(AtomicBool::new(false)),
    }
}

/// The body `api` answers for the raw request target `raw`.
fn answer(api: &WebApi, raw: &str) -> String {
    match route(raw) {
        Route::Api(r) => api.answer(r, raw, &Query::from_raw_url(raw)),
        other => panic!("{raw} routes to {other:?}"),
    }
}

/// The `Error` events on `rx` within `d`.
fn errors_within(rx: &Receiver<ServiceEvent>, d: Duration) -> Vec<String> {
    let end = Instant::now() + d;
    let mut out = Vec::new();
    while let Ok(ev) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
        if let ServiceEvent::Error(t) = ev {
            out.push(t);
        }
    }
    out
}

fn wait_addr(svc: &WebService) -> SocketAddr {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(a) = svc.local_addr() {
            return a;
        }
        assert!(Instant::now() < end, "no bind within 10 s");
        thread::sleep(Duration::from_millis(10));
    }
}

fn body(addr: SocketAddr, target: &str) -> String {
    let (status, _, body) = http::http(addr, &get(target.as_bytes()));
    assert_eq!(status, 200);
    String::from_utf8(body).unwrap()
}

/// A path request between two unknown stations: it stores both locations (as in C#).
const PATH: &str = "/propagationpath.json?mycall=TE1ST&myloc=JO50AA&dxcall=TE2ND&dxloc=JO60AA";

#[test]
fn stopped_api_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let api = WebApi::new(ctx(dir.path()));
    let locations = |a: &WebApi| a.ctx.paths.stations.location_count().unwrap();
    api.ctx.stop.store(true, Ordering::SeqCst);
    assert_eq!(answer(&api, PATH), "Error: web server stopped");
    assert_eq!(locations(&api), 0);
    // The same request on a running server stores the locations.
    api.ctx.stop.store(false, Ordering::SeqCst);
    answer(&api, PATH);
    assert_eq!(locations(&api), 2);
}

#[test]
fn stopped_service_reports_no_error() {
    // A held port: a running server would report the bind error at once.
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut cfg = WebConfig::new(held.local_addr().unwrap());
    cfg.retry = Duration::from_millis(100);
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(dir.path());
    c.stop.store(true, Ordering::SeqCst);
    let (tx, rx) = channel();
    let _svc = WebService::start(cfg, c, tx);
    let errs = errors_within(&rx, Duration::from_millis(500));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn aircraft_db_opened_again_on_request() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the database file belongs: the open fails.
    let file = dir.path().join("aircrafts.db3");
    std::fs::create_dir(&file).unwrap();
    let (tx, rx) = channel();
    let svc = WebService::start(
        WebConfig::new("127.0.0.1:0".parse().unwrap()),
        ctx(dir.path()),
        tx,
    );
    let addr = wait_addr(&svc);
    assert_eq!(
        body(addr, "/airports.json"),
        "Error: airports: no aircraft database"
    );
    assert_eq!(
        body(addr, "/airports.json"),
        "Error: airports: no aircraft database"
    );
    // The open error is reported once.
    let errs = errors_within(&rx, Duration::from_millis(300));
    let db_errs: Vec<_> = errs
        .iter()
        .filter(|e| e.starts_with("Web server: aircraft database: "))
        .collect();
    assert_eq!(db_errs.len(), 1, "{errs:?}");

    // The database becomes available: the next request opens it.
    std::fs::remove_dir(&file).unwrap();
    let db = AircraftDb::open(dir.path()).unwrap();
    let json = "[{\"ICAO\":\"XXAA\",\"IATA\":\"XAA\",\"Lat\":50.5,\"Lon\":10.25,\"Alt\":300.0,\
                \"Airport\":\"Test Field\",\"Country\":\"Testland\",\
                \"LastUpdated\":\"2026-11-24T12:00:00Z\"}]";
    db.airport_bulk_insert_or_update_if_newer(&airports_from_json(json).unwrap())
        .unwrap();
    drop(db);
    let text = body(addr, "/airports.json");
    assert!(text.starts_with("[{\"ICAO\":\"XXAA\""), "{text}");
}

#[test]
fn unchanged_config_keeps_a_starting_server() {
    let a = WebConfig::new("127.0.0.1:9880".parse().unwrap());
    let b = WebConfig::new("127.0.0.1:9881".parse().unwrap());
    let failed = WebState::NotListening("Web server: cannot listen".into());
    // Unchanged: kept while starting or bound, restarted after a bind error.
    assert!(keeps_server(&a, &WebState::Starting, Some(&a)));
    assert!(keeps_server(&a, &WebState::Listening(9880), Some(&a)));
    assert!(!keeps_server(&a, &failed, Some(&a)));
    // Changed or off: restarted or stopped.
    assert!(!keeps_server(&a, &WebState::Listening(9880), Some(&b)));
    assert!(!keeps_server(&a, &WebState::Starting, None));
}
