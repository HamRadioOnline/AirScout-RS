//! The router hook of `WebConfig`.
mod common;

use as_web::query::Query;
use as_web::route::{route, ApiRoute, Route};
use as_web::server::{Api, Router, WebConfig, WebServer};
use common::{get, http};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

struct Tiny;
impl Api for Tiny {
    fn answer(&self, _route: ApiRoute, _raw: &str, _q: &Query) -> String {
        "{}".into()
    }
}

fn loopback0() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn nothing(_raw: &str) -> Route {
    Route::NotFound
}

fn start(cfg: WebConfig) -> (WebServer, SocketAddr) {
    let server = WebServer::start(cfg, Arc::new(Tiny), Box::new(|_| {}));
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(a) = server.local_addr() {
            return (server, a);
        }
        assert!(Instant::now() < end, "no bind within 10 s");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn default_router_is_route() {
    let cfg = WebConfig::new(loopback0());
    for p in [
        "/",
        "/planes.json",
        "/Planes.JSON",
        "/version.json",
        "/nope",
    ] {
        assert_eq!((cfg.router)(p), route(p), "{p}");
    }
    assert_eq!(cfg, WebConfig::new(loopback0()));
    assert_ne!(
        cfg,
        WebConfig::new(loopback0()).with_router(nothing as Router)
    );
}

#[test]
fn custom_router_used() {
    let (_s, addr) = start(WebConfig::new(loopback0()).with_router(nothing));
    let (status, _, _) = http(addr, &get(b"/planes.json"));
    assert_eq!(status, 404);
    let (_s2, addr2) = start(WebConfig::new(loopback0()));
    let (status, _, body) = http(addr2, &get(b"/planes.json"));
    assert_eq!(status, 200);
    assert_eq!(body, b"{}");
}

#[test]
fn join_frees_the_port() {
    let (s, addr) = start(WebConfig::new(loopback0()));
    s.join();
    assert!(
        std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_err(),
        "port still accepts after join"
    );
    std::net::TcpListener::bind(addr).expect("port is free after join");
}
