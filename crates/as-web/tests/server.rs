//! The HTTP server over the wire (`Webserver.cs:74-193`, `:1113-1235`): status, headers,
//! the C# route rules, malformed requests, the worker pool, stop and the bind retry.
mod common;

use as_web::assets::asset;
use as_web::query::Query;
use as_web::route::ApiRoute;
use as_web::server::{Api, WebConfig, WebServer, MAX_WORKERS, RETRY};
use common::{get, header, http, Conn};
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Every wait in these tests returns early and gives up after this long.
const WAIT: Duration = Duration::from_secs(10);

/// Echoes `"{route:?} {CALL:?}"`. One route can block until `release`, one can panic.
/// Every call is recorded with the number of blocked calls finished before it.
#[derive(Default)]
struct FakeApi {
    block: Option<ApiRoute>,
    panic: Option<ApiRoute>,
    release: AtomicBool,
    started: AtomicUsize,
    finished: AtomicUsize,
    calls: Mutex<Vec<(ApiRoute, usize)>>,
}

impl FakeApi {
    fn blocking(route: ApiRoute) -> Arc<FakeApi> {
        Arc::new(FakeApi {
            block: Some(route),
            ..FakeApi::default()
        })
    }

    fn set_release(&self, on: bool) {
        self.release.store(on, Ordering::SeqCst);
    }

    fn calls_of(&self, route: ApiRoute) -> Vec<usize> {
        let calls = self.calls.lock().unwrap();
        calls
            .iter()
            .filter(|(r, _)| *r == route)
            .map(|(_, f)| *f)
            .collect()
    }
}

impl Api for FakeApi {
    fn answer(&self, route: ApiRoute, _raw_url: &str, query: &Query) -> String {
        let done = self.finished.load(Ordering::SeqCst);
        self.calls.lock().unwrap().push((route, done));
        if self.panic == Some(route) {
            panic!("FakeApi panics on {route:?}");
        }
        if self.block == Some(route) {
            self.started.fetch_add(1, Ordering::SeqCst);
            let end = Instant::now() + WAIT;
            while !self.release.load(Ordering::SeqCst) && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
            self.finished.fetch_add(1, Ordering::SeqCst);
        }
        format!("{route:?} {:?}", query.get("CALL"))
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !cond() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_addr(server: &WebServer) -> SocketAddr {
    let mut addr = None;
    wait_until("the bind", || {
        addr = server.local_addr();
        addr.is_some()
    });
    addr.unwrap()
}

fn loopback0() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn start(api: Arc<FakeApi>) -> (WebServer, SocketAddr) {
    let server = WebServer::start(WebConfig::new(loopback0()), api, Box::new(|_| {}));
    let addr = wait_addr(&server);
    (server, addr)
}

#[test]
fn config_defaults() {
    assert_eq!(RETRY, Duration::from_secs(10));
    assert_eq!(MAX_WORKERS, 4);
    let cfg = WebConfig::new(loopback0());
    assert_eq!(cfg.retry, RETRY);
    assert_eq!(cfg.bind, loopback0());
}

#[test]
fn serves_index_redirect_page() {
    let (_s, addr) = start(Arc::default());
    let (status, headers, body) = http(addr, &get(b"/"));
    assert_eq!(status, 200);
    assert_eq!(header(&headers, "content-type"), Some("text/html"));
    assert_eq!(body, asset("index.html").unwrap().bytes);
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    assert_eq!(
        header(&headers, "content-length"),
        Some(body.len().to_string().as_str())
    );
    let lengths = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"));
    assert_eq!(lengths.count(), 1);
}

#[test]
fn api_route_answers_text_json() {
    let (_s, addr) = start(Arc::default());
    let (status, headers, body) = http(addr, &get(b"/location.json?call=te1st"));
    assert_eq!(status, 200);
    assert_eq!(header(&headers, "content-type"), Some("text/json"));
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    assert_eq!(body, b"Location Some(\"TE1ST\")");
    assert_eq!(header(&headers, "content-length"), Some("22"));
}

#[test]
fn not_found_is_404_text_html() {
    let (_s, addr) = start(Arc::default());
    let (status, headers, body) = http(addr, &get(b"/nothing.here"));
    assert_eq!(status, 404);
    assert_eq!(header(&headers, "content-type"), Some("text/html"));
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    assert_eq!(header(&headers, "content-length"), Some("10"));
    assert_eq!(body, b"Not found!");
}

#[test]
fn post_treated_as_get() {
    let (_s, addr) = start(Arc::default());
    let g = http(addr, &get(b"/bands.json"));
    let p = http(
        addr,
        b"POST /bands.json HTTP/1.1\r\nHost: test\r\nContent-Length: 3\r\n\r\nx=1",
    );
    assert_eq!(g.0, 200);
    assert_eq!((p.0, &p.2), (g.0, &g.2));
    assert_eq!(header(&p.1, "content-type"), header(&g.1, "content-type"));
    // Any other method too.
    let d = http(addr, b"DELETE /bands.json HTTP/1.1\r\nHost: test\r\n\r\n");
    assert_eq!((d.0, &d.2), (g.0, &g.2));
}

#[test]
fn traversal_over_the_wire_is_404() {
    let (_s, addr) = start(Arc::default());
    let targets: [&[u8]; 7] = [
        b"/../Cargo.toml",
        b"/%2e%2e/%2e%2e/etc/passwd",
        b"//etc/passwd",
        b"/C:/Windows/win.ini",
        b"/dist/..%2f..%2fCargo.toml",
        b"/..\\..\\Cargo.toml",
        b"/%00",
    ];
    for t in targets {
        let (status, _, body) = http(addr, &get(t));
        let shown = String::from_utf8_lossy(t);
        assert_eq!(status, 404, "{shown}");
        assert_eq!(body, b"Not found!", "{shown}");
        assert!(
            !String::from_utf8_lossy(&body).contains("[workspace]"),
            "{shown}"
        );
    }
}

#[test]
fn bad_urls_do_not_panic() {
    let (_s, addr) = start(Arc::default());
    let mut long = b"/".to_vec();
    long.extend(std::iter::repeat_n(b'a', 100_000));
    let targets: [&[u8]; 4] = [&long, b"/%", b"/%zz", b"/\xFF\xFEx"];
    for t in targets {
        let (status, _, _) = http(addr, &get(t));
        let shown: String = String::from_utf8_lossy(t).chars().take(20).collect();
        assert!((400..500).contains(&status), "{shown}: {status}");
    }
    let (status, _, body) = http(addr, &get(b"/bands.json"));
    assert_eq!(status, 200);
    assert_eq!(body, b"Bands None");
}

/// Sends `n` blocking `Location` requests, each on its own thread.
fn slow(addr: SocketAddr, n: usize) -> Vec<thread::JoinHandle<common::Answer>> {
    (0..n)
        .map(|_| thread::spawn(move || http(addr, &get(b"/location.json?call=te1st"))))
        .collect()
}

fn join_ok(handles: Vec<thread::JoinHandle<common::Answer>>) {
    for h in handles {
        assert_eq!(h.join().unwrap().0, 200);
    }
}

#[test]
fn slow_answer_does_not_block_others() {
    let api = FakeApi::blocking(ApiRoute::Location);
    let (_s, addr) = start(api.clone());

    // One slow request: another API route answers at once.
    let first = slow(addr, 1);
    wait_until("the slow request", || {
        api.started.load(Ordering::SeqCst) == 1
    });
    let t = Instant::now();
    assert_eq!(http(addr, &get(b"/bands.json")).0, 200);
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );
    api.set_release(true);
    join_ok(first);
    api.set_release(false);

    // MAX_WORKERS slow requests: assets and 404 still answer at once.
    let busy = slow(addr, MAX_WORKERS);
    wait_until("all workers busy", || {
        api.started.load(Ordering::SeqCst) == 1 + MAX_WORKERS
    });
    let t = Instant::now();
    assert_eq!(http(addr, &get(b"/index.html")).0, 200);
    assert_eq!(http(addr, &get(b"/nothing.here")).0, 404);
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );

    // A further API request waits for a free worker.
    let fifth = thread::spawn(move || http(addr, &get(b"/bands.json")));
    thread::sleep(Duration::from_millis(200));
    assert!(!fifth.is_finished(), "answered with all workers busy");
    api.set_release(true);
    let (status, _, body) = fifth.join().unwrap();
    assert_eq!((status, body.as_slice()), (200, &b"Bands None"[..]));
    // Its handler ran only after one of the busy handlers finished.
    let bands = api.calls_of(ApiRoute::Bands);
    assert_eq!(bands.len(), 2, "{bands:?}");
    assert!(bands[1] > 1, "{bands:?}");
    join_ok(busy);
}

#[test]
fn disconnected_waiter_never_runs() {
    let api = FakeApi::blocking(ApiRoute::Location);
    let (_s, addr) = start(api.clone());
    let busy = slow(addr, MAX_WORKERS);
    wait_until("all workers busy", || {
        api.started.load(Ordering::SeqCst) == MAX_WORKERS
    });

    // An API request that waits for a worker, then the client goes away.
    let mut gone = std::net::TcpStream::connect(addr).unwrap();
    gone.write_all(&get(b"/qrv.json")).unwrap();
    thread::sleep(Duration::from_millis(300));
    drop(gone);
    thread::sleep(Duration::from_millis(300));

    api.set_release(true);
    join_ok(busy);
    // One request after the busy ones: the pool has drained.
    assert_eq!(http(addr, &get(b"/bands.json")).0, 200);
    assert_eq!(api.calls_of(ApiRoute::Qrv), Vec::<usize>::new());
}

#[test]
fn panic_in_api_answers_500() {
    let api = Arc::new(FakeApi {
        panic: Some(ApiRoute::Version),
        ..FakeApi::default()
    });
    let (_s, addr) = start(api);
    let (status, headers, body) = http(addr, &get(b"/version.json"));
    assert_eq!(status, 500);
    assert_eq!(header(&headers, "content-type"), Some("text/json"));
    assert_eq!(body, b"Error: internal error");
    let (status, _, _) = http(addr, &get(b"/bands.json"));
    assert_eq!(status, 200);
}

#[test]
fn stop_frees_port_with_idle_keepalive() {
    let (mut server, addr) = start(Arc::default());
    let mut conn = Conn::open(addr);
    assert_eq!(conn.send(&get(b"/bands.json")).0, 200);
    // The same connection carries a second request.
    assert_eq!(conn.send(&get(b"/version.json")).0, 200);

    let t = Instant::now();
    server.stop();
    assert!(t.elapsed() < Duration::from_millis(250), "stop() blocked");
    // Spec: within 1 s; the bound is wider for loaded runners.
    let end = t + Duration::from_secs(5);
    loop {
        if TcpListener::bind(addr).is_ok() {
            break;
        }
        assert!(Instant::now() < end, "port still taken");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(conn.is_closed_by_peer(), "idle keep-alive kept open");
}

#[test]
fn bind_conflict_reports_once_and_retries() {
    let holder = TcpListener::bind(loopback0()).unwrap();
    let addr = holder.local_addr().unwrap();
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = errors.clone();
    let cfg = WebConfig {
        retry: Duration::from_millis(200),
        ..WebConfig::new(addr)
    };
    let server = WebServer::start(
        cfg,
        Arc::new(FakeApi::default()),
        Box::new(move |e| sink.lock().unwrap().push(e)),
    );
    wait_until("the first bind error", || {
        !errors.lock().unwrap().is_empty()
    });
    // Several retries later, still one report.
    thread::sleep(Duration::from_millis(700));
    {
        let errs = errors.lock().unwrap();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("cannot listen"), "{}", errs[0]);
        assert!(errs[0].contains(&addr.to_string()), "{}", errs[0]);
    }
    assert_eq!(server.local_addr(), None);

    drop(holder);
    // Spec: within 2 s; the bound is wider for loaded runners.
    assert_eq!(wait_addr(&server), addr);
    assert_eq!(http(addr, &get(b"/bands.json")).0, 200);
}

#[test]
fn start_with_builds_the_api_on_the_web_thread_before_the_bind() {
    let seen: Arc<Mutex<Option<Option<String>>>> = Arc::default();
    let slot = seen.clone();
    // a listener on the port: the builder must run before any bind attempt succeeds
    let held = TcpListener::bind(loopback0()).unwrap();
    let mut cfg = WebConfig::new(held.local_addr().unwrap());
    cfg.retry = Duration::from_millis(100);
    let server = WebServer::start_with(
        cfg,
        Box::new(move || {
            let name = thread::current().name().map(str::to_string);
            *slot.lock().unwrap() = Some(name);
            Arc::new(FakeApi::default()) as Arc<dyn Api>
        }),
        Box::new(|_| {}),
    );
    wait_until("the builder", || seen.lock().unwrap().is_some());
    assert_eq!(seen.lock().unwrap().clone(), Some(Some("web".to_string())));
    assert_eq!(server.local_addr(), None);
    drop(held);
    let addr = wait_addr(&server);
    let (status, _, body) = http(addr, &get(b"/version.json"));
    assert_eq!((status, body), (200, b"Version None".to_vec()));
}
