//! The planeserver `Service`: feeds into one cache, `/planes.json` and the welcome page on
//! the LAN port, the status, and a prompt stop. Servers bind `127.0.0.1:0`; no test goes to
//! the internet.
use airscout_planeserver::config::{PlaneServerConfig, WELCOME};
use airscout_planeserver::service::{make_feeds, router, Service, Status};
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_web::route::{ApiRoute, Route};
use as_web::server::WebConfig;
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// Every poll loop gives up after this long (loaded CI).
const WAIT: Duration = Duration::from_secs(10);

/// What a [`FakeFeed`] does on each `get_planes` once its script is used up.
#[derive(Clone, Copy)]
enum Then {
    Empty,
    Fail,
    Hang(Duration),
}

/// A scripted feed (as in `crates/as-feeds/tests/worker.rs`).
struct FakeFeed {
    name: &'static str,
    script: VecDeque<Result<Vec<FeedPlane>, FeedError>>,
    then: Then,
}

impl FakeFeed {
    fn boxed(name: &'static str, planes: Vec<FeedPlane>, then: Then) -> Box<dyn PlaneFeed> {
        Box::new(FakeFeed {
            name,
            script: VecDeque::from([Ok(planes)]),
            then,
        })
    }

    fn failing(name: &'static str) -> Box<dyn PlaneFeed> {
        Box::new(FakeFeed {
            name,
            script: VecDeque::new(),
            then: Then::Fail,
        })
    }
}

impl PlaneFeed for FakeFeed {
    fn name(&self) -> &'static str {
        self.name
    }
    fn info(&self) -> &'static str {
        "scripted"
    }
    fn disclaimer(&self) -> Option<&'static str> {
        None
    }
    fn enabled_by_default(&self) -> bool {
        true
    }
    fn settings(&self) -> toml::Table {
        toml::Table::new()
    }
    fn load_settings(&mut self, _: &toml::Table) -> Result<(), FeedError> {
        Ok(())
    }
    fn start(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
    fn get_planes(&mut self, _: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        if let Some(r) = self.script.pop_front() {
            return r;
        }
        match self.then {
            Then::Empty => Ok(Vec::new()),
            Then::Fail => Err(FeedError::Http("500".into())),
            Then::Hang(d) => {
                thread::sleep(d);
                Ok(Vec::new())
            }
        }
    }
    fn stop(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}

/// A synthetic plane inside the default area and altitude band.
fn plane(hex: &str, call: &str) -> FeedPlane {
    FeedPlane {
        time: Some(chrono::Utc::now()),
        hex: hex.into(),
        call: call.into(),
        reg: "D-AIMA".into(),
        lat: 50.0,
        lon: 8.0,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 450.0,
        ..FeedPlane::default()
    }
}

fn localhost() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// The default config on `127.0.0.1:0` with its data dir in `dir`.
fn cfg(dir: &Path) -> PlaneServerConfig {
    PlaneServerConfig {
        bind: localhost(),
        port: 0,
        interval_s: 1,
        data_dir: Some(dir.to_path_buf()),
        ..PlaneServerConfig::default()
    }
}

fn wait_until(what: &str, limit: Duration, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !cond() {
        assert!(
            Instant::now() < end,
            "timed out after {limit:?} waiting for {what}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// The bound address, once the server listens.
fn addr_of(svc: &Service) -> SocketAddr {
    let mut addr = None;
    wait_until("the bind", WAIT, || {
        addr = svc.local_addr();
        addr.is_some()
    });
    addr.unwrap()
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(WAIT)
        .build()
        .unwrap()
}

fn get(addr: SocketAddr, path: &str) -> reqwest::blocking::Response {
    client().get(format!("http://{addr}{path}")).send().unwrap()
}

/// Sends `raw` on a new connection and reads up to EOF. `None` when the server closed or
/// reset the connection without an answer.
fn raw_http(addr: SocketAddr, raw: &[u8]) -> Option<(u16, String)> {
    let mut s = TcpStream::connect_timeout(&addr, WAIT).unwrap();
    s.set_read_timeout(Some(WAIT)).unwrap();
    s.set_write_timeout(Some(WAIT)).unwrap();
    // The server may answer an oversized head and close before it read all of it.
    let _ = s.write_all(raw);
    let mut out = Vec::new();
    let mut b = [0u8; 4096];
    loop {
        match s.read(&mut b) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&b[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                assert!(
                    !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
                    "no answer within {WAIT:?}"
                );
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&out).into_owned();
    let status = text.split(' ').nth(1)?.parse().ok()?;
    Some((status, text))
}

/// The planes of a `/planes.json` text, read back by the AirScoutServer feed.
fn read_back(body: &str) -> Vec<String> {
    let mut hex: Vec<String> = as_feeds::feeds::airscoutserver::parse(body, chrono::Utc::now())
        .unwrap()
        .into_iter()
        .map(|p| p.hex)
        .collect();
    hex.sort();
    hex
}

/// A web config like `Service::start`'s, on `127.0.0.1:0`.
fn web() -> WebConfig {
    WebConfig::new(SocketAddr::new(localhost(), 0))
}

fn all_status_text(s: &Status) -> String {
    let mut t = format!("{s:?}\n{}\n", s.server);
    for f in &s.feeds {
        t.push_str(&format!(
            "{} {} {:?}\n",
            f.name,
            f.state,
            f.last_error.as_deref()
        ));
    }
    if let Some(e) = &s.db_error {
        t.push_str(e);
    }
    t
}

#[test]
fn planeserver_router_only_two_routes() {
    for p in ["/planes.json", "/PLANES.JSON", "/planes.json?minlat=1"] {
        assert_eq!(router(p), Route::Api(ApiRoute::Planes), "{p}");
    }
    match router("/") {
        Route::Asset(a) => {
            assert_eq!(a.mime, "text/html");
            assert_eq!(a.bytes, WELCOME.as_bytes());
        }
        r => panic!("/ routed to {r:?}"),
    }
    for p in [
        "/index.html",
        "/../planes.json",
        "/settings.json",
        "/planes.json/x",
        "/version.json",
        "/AircraftList.json",
    ] {
        assert_eq!(router(p), Route::NotFound, "{p}");
    }
}

#[test]
fn serves_planes_from_fake_feed() {
    let tmp = tempfile::tempdir().unwrap();
    let feeds = vec![Ok(FakeFeed::boxed(
        "Fake",
        vec![plane("3C0001", "TE1ST"), plane("3C0002", "TE2ST")],
        Then::Empty,
    ))];
    let svc = Service::start(&cfg(tmp.path()), feeds).unwrap();
    wait_until("the first planes", WAIT, || {
        svc.status().feeds[0].last_ok.is_some()
    });
    let addr = addr_of(&svc);
    let r = get(addr, "/planes.json");
    assert_eq!(r.status(), 200);
    assert_eq!(read_back(&r.text().unwrap()), ["3C0001", "3C0002"]);
    let st = svc.status();
    assert_eq!(st.planes, 2);
    assert_eq!(st.feeds.len(), 1);
    assert_eq!(st.feeds[0].name, "Fake");
    assert_eq!(st.server, format!("listening on {addr}"));
    assert_eq!(st.db_error, None);
    assert!(tmp.path().join("aircrafts.db3").exists());
    for d in ["tmp", "log", "database"] {
        assert!(tmp.path().join(d).is_dir(), "{d}");
    }
    svc.stop();
}

#[test]
fn serves_only_planes_and_welcome() {
    let tmp = tempfile::tempdir().unwrap();
    let svc = Service::start(&cfg(tmp.path()), Vec::new()).unwrap();
    let addr = addr_of(&svc);

    let r = get(addr, "/");
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "text/html");
    assert!(r
        .text()
        .unwrap()
        .contains("Welcome to AirScout PlaneServer"));
    for p in ["/settings.json", "/index.html"] {
        assert_eq!(get(addr, p).status(), 404, "{p}");
    }

    // POST answers like GET, as the 4d server does (#266).
    let r = client()
        .post(format!("http://{addr}/planes.json"))
        .body("x")
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r
        .text()
        .unwrap()
        .starts_with("{\"full_count\":0,\"version\":1"));

    // A raw target, not normalised by a client.
    let (status, _) = raw_http(
        addr,
        b"GET /../planes.json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .expect("an answer to /../planes.json");
    assert_eq!(status, 404);

    // A 1 MB header: closed or refused with 4xx; the server stays up.
    let mut big = b"GET /planes.json HTTP/1.1\r\nHost: localhost\r\nX-Big: ".to_vec();
    big.extend(std::iter::repeat_n(b'a', 1 << 20));
    big.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
    if let Some((status, _)) = raw_http(addr, &big) {
        assert!(
            (400..500).contains(&status),
            "1 MB header answered {status}"
        );
    }
    assert_eq!(get(addr, "/planes.json").status(), 200);
    svc.stop();
}

#[test]
fn survives_many_connections() {
    let tmp = tempfile::tempdir().unwrap();
    let web = WebConfig {
        header_timeout: Duration::from_millis(500),
        max_connections: 64,
        ..web()
    };
    let svc = Service::start_with_web(&cfg(tmp.path()), Vec::new(), web).unwrap();
    let addr = addr_of(&svc);
    let silent: Vec<TcpStream> = (0..100)
        .map(|_| TcpStream::connect_timeout(&addr, WAIT).unwrap())
        .collect();
    let t = Instant::now();
    let r = client()
        .get(format!("http://{addr}/planes.json"))
        .timeout(Duration::from_secs(5))
        .send()
        .expect("a fresh GET answers within 5 s");
    assert_eq!(r.status(), 200);
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    drop(silent);
    svc.stop();
}

#[test]
fn failing_feed_does_not_stop_others() {
    let tmp = tempfile::tempdir().unwrap();
    let feeds = vec![
        Ok(FakeFeed::failing("Bad")),
        Ok(FakeFeed::boxed(
            "Good",
            vec![plane("3C0001", "TE1ST")],
            Then::Empty,
        )),
    ];
    let svc = Service::start(&cfg(tmp.path()), feeds).unwrap();
    wait_until("both feeds report", WAIT, || {
        let s = svc.status();
        s.feeds[0].last_error.is_some() && s.feeds[1].last_ok.is_some()
    });
    let s = svc.status();
    assert_eq!(s.feeds[0].name, "Bad");
    assert!(s.feeds[0].last_error.as_deref().unwrap().contains("500"));
    assert_eq!(s.feeds[0].last_ok, None);
    // the error carries its time
    assert!(s.feeds[0].last_error_at.is_some());
    assert_eq!(s.feeds[1].name, "Good");
    assert_eq!(s.feeds[1].last_error, None);
    assert_eq!(s.feeds[1].last_error_at, None);
    let addr = addr_of(&svc);
    assert_eq!(
        read_back(&get(addr, "/planes.json").text().unwrap()),
        ["3C0001"]
    );
    let t = Instant::now();
    svc.stop();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn restricted_feed_needs_disclaimer() {
    const PASSWORD: &str = "pw-TE1ST-5ecret";
    let tmp = tempfile::tempdir().unwrap();
    let mut c = PlaneServerConfig {
        server_active: false,
        feeds: vec!["ADSBSupport".into()],
        ..cfg(tmp.path())
    };
    let mut t = toml::Table::new();
    t.insert("password".into(), PASSWORD.into());
    c.feed_settings.insert("ADSBSupport".into(), t.into());

    let feeds = make_feeds(&c);
    assert_eq!(feeds.len(), 1);
    let Err(e) = &feeds[0] else {
        panic!("ADSBSupport made without acceptance")
    };
    assert_eq!(e, "disclaimer not accepted (ADSBSupport)");

    let svc = Service::start(&c, feeds).unwrap();
    let s = svc.status();
    assert_eq!(s.feeds.len(), 1);
    assert_eq!(s.feeds[0].name, "ADSBSupport");
    assert_eq!(s.feeds[0].state, "disclaimer not accepted (ADSBSupport)");
    assert_eq!(s.server, "off");
    assert_eq!(svc.local_addr(), None);
    assert!(!all_status_text(&s).contains(PASSWORD));
    svc.stop();
}

#[test]
fn make_feeds_slot_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = PlaneServerConfig {
        feeds: vec![
            "[none]".into(),
            String::new(),
            "NoSuchFeed".into(),
            "ADSBSupport".into(),
        ],
        feeds_disclaimer_accepted: vec!["ADSBSupport".into()],
        ..cfg(tmp.path())
    };
    // A credential of the wrong type: the error names the key, never the value.
    let mut t = toml::Table::new();
    t.insert("password".into(), 987_654_321.into());
    c.feed_settings.insert("ADSBSupport".into(), t.into());
    let feeds: Vec<Result<String, String>> = make_feeds(&c)
        .into_iter()
        .map(|f| f.map(|f| f.name().to_owned()))
        .collect();
    assert_eq!(feeds.len(), 2, "[none] and empty slots are skipped");
    assert_eq!(feeds[0], Err("unknown feed (NoSuchFeed)".to_owned()));
    let e = feeds[1].clone().unwrap_err();
    assert!(e.contains("password"), "{e}");
    assert!(!e.contains("987654321"), "{e}");

    // Accepted and with valid settings, the restricted feed is made.
    let mut t = toml::Table::new();
    t.insert("password".into(), "pw".into());
    c.feed_settings.insert("ADSBSupport".into(), t.into());
    let made = make_feeds(&c);
    assert!(matches!(&made[1], Ok(f) if f.name() == "ADSBSupport"));
}

#[test]
fn port_in_use_reports_and_retries() {
    let tmp = tempfile::tempdir().unwrap();
    let taken = TcpListener::bind((localhost(), 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let c = PlaneServerConfig {
        port,
        ..cfg(tmp.path())
    };
    let web = WebConfig {
        retry: Duration::from_millis(200),
        ..WebConfig::new(SocketAddr::new(localhost(), port))
    };
    let feeds = vec![Ok(FakeFeed::boxed(
        "Fake",
        vec![plane("3C0001", "TE1ST")],
        Then::Empty,
    ))];
    let svc = Service::start_with_web(&c, feeds, web).unwrap();
    let want = format!("127.0.0.1:{port}");
    wait_until("the bind error", Duration::from_secs(2), || {
        let s = svc.status().server;
        s.contains(&want) && s.contains("cannot listen")
    });
    // The feeds keep collecting while the port is taken.
    wait_until("planes while the port is taken", WAIT, || {
        svc.status().feeds[0].last_ok.is_some()
    });
    assert_eq!(svc.local_addr(), None);
    drop(taken);
    wait_until(
        "the retried bind",
        Duration::from_millis(200) + Duration::from_secs(2),
        || svc.local_addr().is_some(),
    );
    assert_eq!(svc.status().server, format!("listening on {want}"));
    svc.stop();
}

#[test]
fn stop_is_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let feeds = vec![Ok(FakeFeed::boxed(
        "Fake",
        vec![plane("3C0001", "TE1ST")],
        Then::Empty,
    ))];
    let svc = Service::start(&cfg(tmp.path()), feeds).unwrap();
    addr_of(&svc);
    wait_until("the first planes", WAIT, || {
        svc.status().feeds[0].last_ok.is_some()
    });
    let t = Instant::now();
    svc.stop();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn stop_is_prompt_with_hanging_feed() {
    let tmp = tempfile::tempdir().unwrap();
    let feeds = vec![Ok(FakeFeed::boxed(
        "Hang",
        Vec::new(),
        Then::Hang(Duration::from_secs(20)),
    ))];
    let svc = Service::start(&cfg(tmp.path()), feeds).unwrap();
    addr_of(&svc);
    wait_until("the first cycle", WAIT, || {
        svc.status().feeds[0].last_ok.is_some()
    });
    // The second `get_planes` hangs; give the worker time to enter it.
    thread::sleep(Duration::from_millis(1500));
    let t = Instant::now();
    svc.stop();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn broken_data_dir_falls_back_to_memory() {
    let tmp = tempfile::tempdir().unwrap();
    // A file where the data dir should be.
    let file = tmp.path().join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    let c = PlaneServerConfig {
        server_active: false,
        ..cfg(&file)
    };
    let feeds = vec![Ok(FakeFeed::boxed(
        "Fake",
        vec![plane("3C0001", "TE1ST")],
        Then::Empty,
    ))];
    let svc = Service::start(&c, feeds).unwrap();
    assert!(svc.status().db_error.is_some());
    wait_until("planes with the in-memory db", WAIT, || {
        svc.status().planes == 1
    });
    svc.stop();
}

/// Apply in the window stops the service and starts a new one on the same port: the port is
/// free again for the new one at once.
#[test]
fn restart_on_same_port() {
    let tmp = tempfile::tempdir().unwrap();
    let first = Service::start(&cfg(tmp.path()), Vec::new()).unwrap();
    let port = addr_of(&first).port();
    first.stop();
    let again = PlaneServerConfig {
        port,
        ..cfg(tmp.path())
    };
    let second = Service::start(&again, Vec::new()).unwrap();
    wait_until("the second bind", Duration::from_secs(2), || {
        second.local_addr().is_some()
    });
    assert_eq!(second.local_addr().unwrap().port(), port);
    assert_eq!(get(second.local_addr().unwrap(), "/").status(), 200);
    second.stop();
}
