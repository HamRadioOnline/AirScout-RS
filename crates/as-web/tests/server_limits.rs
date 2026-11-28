//! The server limits against a hostile LAN peer: the request head timeout, the keep-alive
//! idle timeout, the connection cap, the bounded stop drain, and the closed worker pool
//! after stop.
mod common;

use as_web::query::Query;
use as_web::route::ApiRoute;
use as_web::server::{
    Api, WebConfig, WebServer, DRAIN, HEADER_TIMEOUT, MAX_CONNECTIONS, MAX_WORKERS,
};
use common::{get, http, Conn};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Every wait in these tests returns early and gives up after this long.
const WAIT: Duration = Duration::from_secs(10);

/// The head timeout of the timeout tests.
const SHORT: Duration = Duration::from_millis(300);

/// Answers `"{route:?}"`. `Location` blocks until `release` (at most [`WAIT`]). Every
/// call is recorded.
#[derive(Default)]
struct SlowApi {
    release: AtomicBool,
    started: AtomicUsize,
    calls: Mutex<Vec<ApiRoute>>,
}

impl Api for SlowApi {
    fn answer(&self, route: ApiRoute, _raw_url: &str, _query: &Query) -> String {
        self.calls.lock().unwrap().push(route);
        if route == ApiRoute::Location {
            self.started.fetch_add(1, Ordering::SeqCst);
            let end = Instant::now() + WAIT;
            while !self.release.load(Ordering::SeqCst) && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
        }
        format!("{route:?}")
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !cond() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn start(cfg: WebConfig, api: Arc<SlowApi>) -> (WebServer, SocketAddr) {
    let server = WebServer::start(cfg, api, Box::new(|_| {}));
    let mut addr = None;
    wait_until("the bind", || {
        addr = server.local_addr();
        addr.is_some()
    });
    (server, addr.unwrap())
}

fn cfg() -> WebConfig {
    WebConfig::new("127.0.0.1:0".parse().unwrap())
}

/// Connects with a read timeout of `read`.
fn connect(addr: SocketAddr, read: Duration) -> TcpStream {
    let s = TcpStream::connect_timeout(&addr, WAIT).unwrap();
    s.set_read_timeout(Some(read)).unwrap();
    s
}

/// Reads until the server closes `s`; the time that took, or `None` when the read timed
/// out first (the connection is still open).
fn time_to_close(s: &mut TcpStream) -> Option<Duration> {
    let t = Instant::now();
    let mut b = [0u8; 1024];
    loop {
        match s.read(&mut b) {
            Ok(0) => return Some(t.elapsed()),
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return None
            }
            Err(_) => return Some(t.elapsed()),
        }
    }
}

#[test]
fn limit_defaults() {
    assert_eq!(HEADER_TIMEOUT, Duration::from_secs(10));
    assert_eq!(MAX_CONNECTIONS, 64);
    assert_eq!(DRAIN, Duration::from_secs(1));
    let c = cfg();
    assert_eq!(
        (c.header_timeout, c.max_connections, c.drain),
        (HEADER_TIMEOUT, MAX_CONNECTIONS, DRAIN)
    );
}

#[test]
fn half_sent_head_is_closed_after_the_timeout() {
    let (_s, addr) = start(
        WebConfig {
            header_timeout: SHORT,
            ..cfg()
        },
        Arc::default(),
    );
    let mut s = connect(addr, Duration::from_secs(3));
    s.write_all(b"GET /index.html HTTP/1.1\r\nHost: te")
        .unwrap();
    let closed = time_to_close(&mut s).expect("half-sent head still open after 3 s");
    assert!(closed >= SHORT - Duration::from_millis(50), "{closed:?}");
    // The server goes on.
    assert_eq!(http(addr, &get(b"/index.html")).0, 200);
}

#[test]
fn idle_keepalive_is_closed_after_the_timeout() {
    let (_s, addr) = start(
        WebConfig {
            header_timeout: SHORT,
            ..cfg()
        },
        Arc::default(),
    );
    let mut conn = Conn::open(addr);
    assert_eq!(conn.send(&get(b"/bands.json")).0, 200);
    // A second request within the timeout is served on the same connection.
    thread::sleep(SHORT / 3);
    assert_eq!(conn.send(&get(b"/bands.json")).0, 200);
    let t = Instant::now();
    assert!(conn.is_closed_by_peer(), "idle keep-alive kept open");
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

#[test]
fn stop_with_a_stalled_client_ends_the_thread() {
    let (mut server, addr) = start(cfg(), Arc::default());
    // A half-sent head and a silent connection; the head timeout (10 s) does not help.
    let mut half = connect(addr, Duration::from_secs(3));
    half.write_all(b"GET / HTTP/1.1\r\nHost: te").unwrap();
    let mut silent = connect(addr, Duration::from_secs(3));
    thread::sleep(Duration::from_millis(100));

    let t = Instant::now();
    server.stop();
    assert!(t.elapsed() < Duration::from_millis(250), "stop() blocked");
    // Spec: the drain (1 s); the bound is wider for loaded runners.
    let end = t + Duration::from_secs(4);
    while !server.is_finished() {
        assert!(
            Instant::now() < end,
            "web thread alive {:?} after stop",
            t.elapsed()
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(TcpListener::bind(addr).is_ok(), "port still taken");
    assert!(
        time_to_close(&mut half).is_some(),
        "half-sent head kept open"
    );
    assert!(
        time_to_close(&mut silent).is_some(),
        "silent client kept open"
    );
}

#[test]
fn connections_over_the_cap_wait_then_assets_answer() {
    let cap = 4;
    let (_s, addr) = start(
        WebConfig {
            max_connections: cap,
            ..cfg()
        },
        Arc::default(),
    );
    // `cap` silent connections, then more than the cap.
    let held: Vec<TcpStream> = (0..cap + 3)
        .map(|_| connect(addr, Duration::from_secs(3)))
        .collect();
    thread::sleep(Duration::from_millis(200));

    // With the cap full, a further request is not served.
    let mut waiting = connect(addr, Duration::from_millis(500));
    waiting.write_all(&get(b"/index.html")).unwrap();
    let mut b = [0u8; 16];
    let r = waiting.read(&mut b);
    assert!(
        matches!(&r, Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "served over the cap: {r:?}"
    );

    // The held connections close: the waiting request and new ones are served.
    drop(held);
    waiting.set_read_timeout(Some(WAIT)).unwrap();
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = waiting.read(&mut b).unwrap();
        assert!(n > 0, "closed before an answer");
        head.extend_from_slice(&b[..n]);
    }
    assert!(head.starts_with(b"HTTP/1.1 200"), "{head:?}");
    for _ in 0..cap * 2 {
        assert_eq!(http(addr, &get(b"/index.html")).0, 200);
    }
}

#[test]
fn queued_api_request_never_runs_after_stop() {
    let api = Arc::new(SlowApi::default());
    let (mut server, addr) = start(cfg(), api.clone());
    let busy: Vec<_> = (0..MAX_WORKERS)
        .map(|_| thread::spawn(move || http(addr, &get(b"/location.json?call=te1st"))))
        .collect();
    wait_until("all workers busy", || {
        api.started.load(Ordering::SeqCst) == MAX_WORKERS
    });
    // A request that waits for a worker.
    let queued = thread::spawn(move || {
        let mut s = connect(addr, WAIT);
        s.write_all(&get(b"/qrv.json?call=te1st")).unwrap();
        let mut answer = Vec::new();
        let _ = s.read_to_end(&mut answer);
        answer
    });
    thread::sleep(Duration::from_millis(200));

    let t = Instant::now();
    server.stop();
    let answer = queued.join().unwrap();
    // It returns at once, without a worker: the busy handlers still run.
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    assert!(
        answer.starts_with(b"HTTP/1.1 503"),
        "{:?}",
        String::from_utf8_lossy(&answer)
    );
    assert!(answer.ends_with(b"Error: web server stopped"), "{answer:?}");
    api.release.store(true, Ordering::SeqCst);
    for h in busy {
        let _ = h.join();
    }
    thread::sleep(Duration::from_millis(100));
    assert!(!api.calls.lock().unwrap().contains(&ApiRoute::Qrv));
}
