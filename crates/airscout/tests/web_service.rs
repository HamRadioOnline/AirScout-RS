//! Desktop aircraft routes (`Webserver.cs:429-449`) and the server lifecycle in the Options cycle
//! (`MapDlg.cs:2402-2414`, `:2477-2503`).
#[path = "../../as-web/tests/common/mod.rs"]
mod http;

use airscout::app::{RealServices, Services};
use airscout::net_addrs::NetAddrs;
use airscout::services::{open_data, ServiceEvent};
use airscout::web_api::{WebApi, WebCtx};
use airscout::web_service::{web_config, WebService, WebState};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_settings::Settings;
use as_web::planes::planes_json;
use as_web::query::Query;
use as_web::route::{route, Route};
use as_web::server::{Api, WebConfig};
use chrono::{DateTime, TimeZone, Utc};
use http::{get, Conn};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

/// Every wait returns early and gives up after this long.
const WAIT: Duration = Duration::from_secs(10);

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap()
}

fn plane(time: DateTime<Utc>, hex: &str, call: &str) -> PlaneInfo {
    PlaneInfo::new(
        time,
        call,
        "TE-ST1",
        hex,
        50.12345,
        8.5,
        89.5,
        35000.4,
        450.5,
        "B738",
        "",
        "",
        PlaneCategory::Medium,
    )
}

fn settings() -> Settings {
    Settings {
        my_call: "TE1ST".into(),
        dx_call: "TE2ND".into(),
        ..Settings::default()
    }
}

fn ctx(dir: &Path, cache: Arc<PlaneInfoCache>) -> WebCtx {
    WebCtx {
        paths: open_data(dir).unwrap(),
        cache,
        live: Arc::new(RwLock::new(settings())),
        aircraft: None,
        tmp_dir: dir.join("tmp"),
        stop: Arc::new(AtomicBool::new(false)),
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !cond() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn planes_json_layout_matches_csharp() {
    let planes = [plane(t0(), "AAAAAA", "TST1"), plane(t0(), "BBBBBB", "TST2")];
    let row = |i: &str, hex: &str, call: &str| {
        format!(
            ",\"{i}\":[\"{hex}\",50.1235,8.5000,90,35000,451,\"\",\"\",\"B738\",\"TE-ST1\",\
             1795521600,\"\",\"\",\"\",0,0,\"{call}\",0]\n"
        )
    };
    let want = format!(
        "{{\"full_count\":2,\"version\":7{}{}}}\n",
        row("00000000", "AAAAAA", "TST1"),
        row("00000001", "BBBBBB", "TST2")
    );
    assert_eq!(planes_json(&planes, 7), want);
    assert_eq!(planes_json(&[], 1), "{\"full_count\":0,\"version\":1}\n");
}

#[test]
fn planes_json_escapes_quotes() {
    let mut p = plane(t0(), "AA\"AA", "TS\"1");
    p.type_code = "B7\\8".into();
    p.reg = "TE\nST".into();
    let text = planes_json(&[p], 1);
    assert!(text.contains(r#"["AA\"AA","#), "{text}");
    assert!(text.contains(r#","B7\\8","TE\nST","#), "{text}");
    assert!(text.contains(r#","TS\"1",0]"#), "{text}");
}

/// The body `api` answers for the raw request target `raw`.
fn answer(api: &WebApi, raw: &str) -> String {
    match route(raw) {
        Route::Api(r) => api.answer(r, raw, &Query::from_raw_url(raw)),
        other => panic!("{raw} routes to {other:?}"),
    }
}

#[test]
fn finding_370_legacy_forecast_is_ignored_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let legacy = tmp.join("planes.json");
    let snapshot = planes_json(&[plane(t0(), "3C0001", "TE1ST")], 1);
    std::fs::write(&legacy, &snapshot).unwrap();

    let api = WebApi::new(ctx(dir.path(), Arc::new(PlaneInfoCache::new())));
    let response = answer(&api, "/planes.json");
    assert!(as_feeds::feeds::airscoutserver::parse(&response, t0())
        .unwrap()
        .is_empty());
    assert_eq!(std::fs::read_to_string(legacy).unwrap(), snapshot);
}

#[test]
fn missing_files_answer() {
    let dir = tempfile::tempdir().unwrap();
    let api = WebApi::new(ctx(dir.path(), Arc::new(PlaneInfoCache::new())));
    assert_eq!(
        answer(&api, "/planes.json"),
        "{\"full_count\":0,\"version\":1}\n"
    );
    assert_eq!(
        answer(&api, "/AircraftList.json"),
        "Error: vrsplanes.json not found!"
    );
    let tmp = dir.path().join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("vrsplanes.json"), "{\"acList\":[]}").unwrap();
    assert_eq!(answer(&api, "/AircraftList.json"), "{\"acList\":[]}");
}

#[test]
fn web_config_follows_activation_and_port() {
    let mut s = settings();
    s.web.port = 9880;
    assert_eq!(web_config(&s, NetAddrs::loopback()), None);
    s.server.activate = true;
    assert_eq!(
        web_config(&s, NetAddrs::loopback()),
        Some(WebConfig::new("127.0.0.1:9880".parse().unwrap()))
    );
    assert_eq!(
        web_config(&s, NetAddrs::default()).map(|c| c.bind),
        Some("0.0.0.0:9880".parse().unwrap())
    );
}

/// A TCP port that was free a moment ago (bind and release).
fn free_tcp_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A UDP port that was free a moment ago and differs from `not`.
fn free_udp_port(not: u16) -> u16 {
    loop {
        let p = UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if p != not {
            return p;
        }
    }
}

/// `server.activate` with the web server on `web_port` and the Win-Test receiver on
/// another free port (C17).
fn active(web_port: u16) -> Settings {
    let mut s = settings();
    s.server.activate = true;
    s.web.port = web_port;
    s.server.port = free_udp_port(web_port);
    s
}

struct Real {
    svc: RealServices,
    rx: Receiver<ServiceEvent>,
    _dir: tempfile::TempDir,
}

fn real() -> Real {
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = channel();
    let svc = RealServices::new(open_data(dir.path()).unwrap(), tx).with_net(NetAddrs::loopback());
    Real { svc, rx, _dir: dir }
}

fn start(r: &mut Real, s: &Settings) {
    r.svc
        .start_integrations(s, 1, Arc::new(PlaneInfoCache::new()));
}

fn wait_web(r: &Real) -> SocketAddr {
    let mut addr = None;
    wait_until("the web bind", || {
        addr = r.svc.web_local_addr();
        addr.is_some()
    });
    addr.unwrap()
}

fn body(addr: SocketAddr, target: &str) -> String {
    let (status, _, body) = http::http(addr, &get(target.as_bytes()));
    assert_eq!(status, 200);
    String::from_utf8(body).unwrap()
}

/// The old server refuses connections within 1 s.
fn assert_refuses(addr: SocketAddr) {
    let end = Instant::now() + Duration::from_secs(1);
    while TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
        assert!(Instant::now() < end, "{addr} still accepts after 1 s");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_services_serve_settings_json() {
    let mut r = real();
    start(&mut r, &active(free_tcp_port()));
    let addr = wait_web(&r);
    let text = body(addr, "/settings.json");
    assert!(text.contains("\"MyCall\": \"TE1ST\""), "{text}");
    // the web thread opened the aircraft database before the bind (C20)
    assert_eq!(body(addr, "/airports.json"), "[]");
}

#[test]
fn options_port_change_restarts_web() {
    let mut r = real();
    let a = active(free_tcp_port());
    start(&mut r, &a);
    let addr_a = wait_web(&r);
    let mut b = a.clone();
    loop {
        b.web.port = free_tcp_port();
        if b.web.port != a.web.port && b.web.port != a.server.port {
            break;
        }
    }
    start(&mut r, &b);
    let addr_b = wait_web(&r);
    assert_eq!(addr_b.port(), b.web.port);
    assert!(body(addr_b, "/settings.json").contains("\"MyCall\": \"TE1ST\""));
    assert_refuses(addr_a);
}

#[test]
fn unchanged_config_keeps_server() {
    let mut r = real();
    let s = active(free_tcp_port());
    start(&mut r, &s);
    let addr = wait_web(&r);
    let mut conn = Conn::open(addr);
    assert_eq!(conn.send(&get(b"/version.json")).0, 200);
    start(&mut r, &s);
    assert_eq!(r.svc.web_local_addr(), Some(addr));
    let (status, _, body) = conn.send(&get(b"/settings.json"));
    assert_eq!(status, 200);
    assert!(String::from_utf8(body).unwrap().contains("TE1ST"));
}

#[test]
fn stop_integrations_keeps_web() {
    let mut r = real();
    start(&mut r, &active(free_tcp_port()));
    let addr = wait_web(&r);
    r.svc.stop_integrations();
    assert_eq!(r.svc.web_local_addr(), Some(addr));
    assert!(body(addr, "/settings.json").contains("TE1ST"));
}

#[test]
fn activate_off_stops_web() {
    let mut r = real();
    let mut s = active(free_tcp_port());
    start(&mut r, &s);
    let addr = wait_web(&r);
    s.server.activate = false;
    start(&mut r, &s);
    assert_eq!(r.svc.web_local_addr(), None);
    assert_refuses(addr);
}

#[test]
fn dropping_services_stops_web() {
    let mut r = real();
    start(&mut r, &active(free_tcp_port()));
    let addr = wait_web(&r);
    drop(r);
    assert_refuses(addr);
}

#[test]
fn web_settings_seen_by_next_request() {
    let mut r = real();
    let s = active(free_tcp_port());
    start(&mut r, &s);
    let addr = wait_web(&r);
    let mut changed = s.clone();
    changed.my_call = "TE2ND".into();
    r.svc.set_web_settings(&changed);
    let text = body(addr, "/settings.json");
    assert!(text.contains("\"MyCall\": \"TE2ND\""), "{text}");
}

/// The next web server error on `rx` within 10 s; other events (Win-Test, C17) are skipped.
fn next_web_error(rx: &Receiver<ServiceEvent>) -> String {
    let end = Instant::now() + WAIT;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(ServiceEvent::Error(t)) if t.starts_with("Web server") => return t,
            Ok(_) => {}
            Err(e) => panic!("no web server error within 10 s: {e}"),
        }
    }
}

#[test]
fn unbound_server_restarts_on_options_close() {
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut r = real();
    let s = active(held.local_addr().unwrap().port());
    start(&mut r, &s);
    let first = next_web_error(&r.rx);
    assert!(first.contains("cannot listen"), "{first}");
    assert!(matches!(r.svc.web_state(), WebState::NotListening(t) if t == first));
    // Options closed with the same settings: a fresh bind attempt, reported again
    start(&mut r, &s);
    assert_eq!(next_web_error(&r.rx), first);
    drop(held);
    start(&mut r, &s);
    let addr = wait_web(&r);
    assert_eq!(r.svc.web_state(), WebState::Listening(addr.port()));
}

#[test]
fn web_state_follows_the_bind() {
    let mut r = real();
    assert_eq!(r.svc.web_state(), WebState::Off);
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let mut cfg = WebConfig::new(held.local_addr().unwrap());
    cfg.retry = Duration::from_millis(100);
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = channel();
    let svc = WebService::start(cfg, ctx(dir.path(), Arc::new(PlaneInfoCache::new())), tx);
    wait_until(
        "NotListening",
        || matches!(svc.state(), WebState::NotListening(t) if t.contains("cannot listen")),
    );
    drop(held);
    wait_until("Listening", || svc.state() == WebState::Listening(port));
    start(&mut r, &active(free_tcp_port()));
    let addr = wait_web(&r);
    assert_eq!(r.svc.web_state(), WebState::Listening(addr.port()));
    r.svc
        .start_integrations(&settings(), 2, Arc::new(PlaneInfoCache::new()));
    assert_eq!(r.svc.web_state(), WebState::Off);
}
