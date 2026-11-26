//! Win-Test UDP receiver: filter, dispatch, prepare functions, bind and retry behaviour.
use airscout::net_addrs::NetAddrs;
use airscout::services::{open_data, Paths, ServiceEvent};
use airscout::wintest_nearest::NearestSettings;
use airscout::wintest_service::{
    prepare_add_watch, prepare_remove_watch, prepare_show_path, prepare_watchlist, show_path_band,
    RemoveWatch, ShowPath, WatchEntry, WinTestCommand, WinTestConfig, WinTestCtx, WinTestService,
};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{gpoint_from_loc, PositionInRectangle};
use as_settings::Settings;
use as_stations::{GeoSource, Location};
use as_wintest::{decode, encode, WtKind, WtMessage};
use chrono::Utc;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

mod common;

struct Rig {
    svc: WinTestService,
    rx: Receiver<ServiceEvent>,
    paths: Paths,
    addr: SocketAddr,
    _dir: tempfile::TempDir,
}

fn centre(loc: &str) -> GPoint {
    gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle).unwrap()
}

fn msg(kind: WtKind, src: &str, dst: &str, data: &str) -> Vec<u8> {
    encode(&WtMessage {
        kind,
        src: src.into(),
        dst: dst.into(),
        data: data.into(),
        extra: String::new(),
    })
    .unwrap()
}

fn wait_addr(svc: &WinTestService) -> Option<SocketAddr> {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if let Some(a) = svc.local_addr() {
            return Some(a);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn config(sync: bool, reply_to: SocketAddr) -> WinTestConfig {
    let mut s = Settings::default();
    s.server.port = 0;
    s.watchlist_sync_with_kst = sync;
    let mut cfg = WinTestConfig::from_settings(&s, NetAddrs::loopback());
    cfg.reply_to = reply_to;
    cfg
}

fn rig_in(dir: tempfile::TempDir, sync: bool, reply_to: SocketAddr) -> Rig {
    let paths = open_data(dir.path()).unwrap();
    paths.set_my_station(airscout::services::MyStation {
        call: "TE9MY".into(),
        lat: 50.0,
        lon: 8.0,
    });
    let (tx, rx) = channel();
    let ctx = WinTestCtx {
        paths: paths.clone(),
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(NearestSettings::from_settings(&Settings {
            path_best_case_elevation: false,
            ..Settings::default()
        }))),
        tx,
        generation: 7,
    };
    let svc = WinTestService::start(config(sync, reply_to), ctx);
    let addr = wait_addr(&svc).expect("bound");
    Rig {
        svc,
        rx,
        paths,
        addr,
        _dir: dir,
    }
}

fn rig(sync: bool) -> Rig {
    rig_in(
        tempfile::tempdir().unwrap(),
        sync,
        "127.0.0.1:9".parse().unwrap(),
    )
}

fn send(r: &Rig, bytes: &[u8]) {
    let c = UdpSocket::bind("127.0.0.1:0").unwrap();
    c.send_to(bytes, r.addr).unwrap();
}

/// The next `WinTest` event within `secs` (returns as soon as it arrives); any other event fails
/// the test.
fn next_cmd(r: &Rig, secs: u64) -> Option<WinTestCommand> {
    match r.rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(ServiceEvent::WinTest { generation, cmd }) => {
            assert_eq!(generation, 7);
            Some(cmd)
        }
        Ok(other) => panic!("unexpected event {other:?}"),
        Err(_) => None,
    }
}

const SHOW: &str = "1440000,TE1ST,JO50AA,TE2ND,JO60AA";

#[test]
fn receiver_dispatches_show_path_to_app() {
    let r = rig(false);
    send(&r, &msg(WtKind::AsShowPath, "KST", "AS", SHOW));
    assert_eq!(
        next_cmd(&r, 10),
        Some(WinTestCommand::ShowPath(ShowPath {
            band: Some(Band::B144M),
            my_call: "TE1ST".into(),
            my: centre("JO50AA"),
            dx_call: "TE2ND".into(),
            dx: centre("JO60AA"),
        }))
    );
}

#[test]
fn other_destination_is_ignored() {
    let r = rig(false);
    send(&r, &msg(WtKind::AsShowPath, "KST", "XX", SHOW));
    assert_eq!(next_cmd(&r, 1), None);
    send(&r, &msg(WtKind::AsShowPath, "KST", "AS", SHOW));
    assert!(next_cmd(&r, 10).is_some());
}

#[test]
fn corrupt_datagram_is_dropped_before_any_handler() {
    let r = rig(false);
    // Encode a valid SHOW path, then corrupt only bit 7 of the trailing checksum byte. The
    // receiver must drop it before any handler runs (#194).
    let mut bytes = msg(WtKind::AsShowPath, "KST", "AS", SHOW);
    *bytes.last_mut().unwrap() &= 0x7F;
    assert!(!decode(&bytes).unwrap().checksum_ok);
    send(&r, &bytes);
    assert_eq!(next_cmd(&r, 1), None);
    // The correct datagram is handled.
    send(&r, &msg(WtKind::AsShowPath, "KST", "AS", SHOW));
    assert!(matches!(
        next_cmd(&r, 10),
        Some(WinTestCommand::ShowPath(_))
    ));
}

#[test]
fn malformed_datagrams_do_not_stop_receiver() {
    let r = rig(false);
    send(&r, b"");
    send(&r, b"garbage");
    let mut x: u32 = 12345;
    for _ in 0..200 {
        let n = {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            (x >> 24) as usize % 120
        };
        let bytes: Vec<u8> = (0..n)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 16) as u8
            })
            .collect();
        send(&r, &bytes);
    }
    // macOS refuses datagrams above 9216 bytes (net.inet.udp.maxdgram): fall back to 9000
    let c = UdpSocket::bind("127.0.0.1:0").unwrap();
    if c.send_to(&vec![b'A'; 65_000], r.addr).is_err() {
        c.send_to(&vec![b'A'; 9_000], r.addr).unwrap();
    }
    // the garbage produces nothing (an Error/Status event would panic in next_cmd). Under
    // load the burst can overflow the localhost UDP receive buffer and drop the valid
    // datagram, so resend it until the receiver answers.
    let mut got = None;
    for _ in 0..10 {
        send(&r, &msg(WtKind::AsShowPath, "KST", "AS", SHOW));
        got = next_cmd(&r, 1);
        if got.is_some() {
            break;
        }
    }
    assert!(matches!(got, Some(WinTestCommand::ShowPath(_))));
}

fn stored(call: &str, loc: &str, lat: f64, lon: f64) -> Location {
    Location::new(call, loc, lat, lon, GeoSource::FromUser, 0, Utc::now())
}

#[test]
fn show_path_uses_stored_location_when_locator_matches() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    // inside JO50AA, off its centre
    paths
        .stations
        .location_insert_or_update_if_newer(&stored("TE1ST", "JO50AA", 50.01, 10.01))
        .unwrap();
    // another square
    paths
        .stations
        .location_insert_or_update_if_newer(&stored("TE2ND", "JO70AA", 52.01, 8.01))
        .unwrap();
    let p = prepare_show_path(SHOW, &paths.stations).unwrap();
    assert_eq!(p.my, GPoint::new(50.01, 10.01));
    assert_eq!(p.dx, centre("JO60AA"));
    let l = paths
        .stations
        .location_find("TE2ND", Some("JO60AA"))
        .unwrap()
        .expect("stored");
    assert_eq!(l.source, GeoSource::FromLoc);
    // the match path writes nothing
    assert!(paths
        .stations
        .location_find("TE1ST", Some("JO50AA"))
        .unwrap()
        .is_some());
}

#[test]
fn show_path_short_locator_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    assert_eq!(
        prepare_show_path("1440000,TE1ST,JO50,TE2ND,JO60AA", &paths.stations),
        None
    );
    assert_eq!(
        prepare_show_path("1440000,TE1ST,JO50AA,TE2ND", &paths.stations),
        None
    );
    assert_eq!(
        prepare_show_path("1440000,1,JO50AA,TE2ND,JO60AA", &paths.stations),
        None
    );
    assert_eq!(paths.stations.location_count().unwrap(), 0);
}

#[test]
fn show_path_band_mapping() {
    for (s, b) in [
        ("500000", Band::B50M),
        ("700000", Band::B70M),
        ("1440000", Band::B144M),
        ("4320000", Band::B432M),
        ("12960000", Band::B1_2G),
        ("23200000", Band::B2_3G),
        ("34000000", Band::B3_4G),
        ("57600000", Band::B5_7G),
        ("103680000", Band::B10G),
        ("240480000", Band::B24G),
        ("470880000", Band::B47G),
        ("760320000", Band::B76G),
    ] {
        assert_eq!(show_path_band(s), Some(b), "{s}");
    }
    assert_eq!(show_path_band("1440001"), None);
    assert_eq!(show_path_band(""), None);
}

#[test]
fn set_path_reply_received() {
    let reply = UdpSocket::bind("127.0.0.1:0").unwrap();
    reply
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let r = rig_in(
        common::data_dir_with_globe(),
        false,
        reply.local_addr().unwrap(),
    );
    send(
        &r,
        &msg(
            WtKind::AsSetPath,
            "KST",
            "AS",
            "1440000,TE1ST,JO50AA,TE2ND,JO60AA",
        ),
    );
    let mut buf = [0u8; 65_535];
    let (n, _) = reply.recv_from(&mut buf).expect("reply");
    let d = decode(&buf[..n]).unwrap();
    assert_eq!(d.msg.kind, WtKind::AsNearest);
    assert_eq!(d.msg.src, "AS");
    assert_eq!(d.msg.dst, "KST");
    assert!(
        d.msg.data.ends_with(",TE1ST,JO50AA,TE2ND,JO60AA,0"),
        "{}",
        d.msg.data
    );
    // read-only ASSETPATH (#265): the answer uses the locator centres and stores nothing
    assert_eq!(r.paths.stations.location_count().unwrap(), 0);
}

#[test]
fn watchlist_only_when_sync_on() {
    let data = "144,TE1ST,JO50AA";
    let on = rig(true);
    send(&on, &msg(WtKind::AsWatchlist, "KST", "AS", data));
    assert!(matches!(
        next_cmd(&on, 10),
        Some(WinTestCommand::Watchlist(v)) if v.len() == 1
    ));
    let off = rig(false);
    send(&off, &msg(WtKind::AsWatchlist, "KST", "AS", data));
    assert_eq!(next_cmd(&off, 1), None);
}

#[test]
fn add_watch_only_when_sync_off() {
    let data = "144,TE1ST,JO50AA,1";
    let off = rig(false);
    send(&off, &msg(WtKind::AsAddWatch, "KST", "AS", data));
    assert!(matches!(
        next_cmd(&off, 10),
        Some(WinTestCommand::AddWatch(v)) if v.len() == 1
    ));
    send(&off, &msg(WtKind::AsRemoveWatch, "KST", "AS", "144"));
    assert_eq!(
        next_cmd(&off, 10),
        Some(WinTestCommand::RemoveWatch(RemoveWatch::All))
    );
    let on = rig(true);
    send(&on, &msg(WtKind::AsAddWatch, "KST", "AS", data));
    send(&on, &msg(WtKind::AsRemoveWatch, "KST", "AS", "144"));
    assert_eq!(next_cmd(&on, 1), None);
}

fn entry(call: &str, loc: &str, checked: bool) -> WatchEntry {
    WatchEntry {
        call: call.into(),
        loc: loc.into(),
        pos: centre(loc),
        checked,
    }
}

#[test]
fn prepare_add_watch_rules() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let db = &paths.stations;
    db.location_insert_or_update_if_newer(&Location::new(
        "TE2ND",
        "JO60AA",
        centre("JO60AA").lat,
        centre("JO60AA").lon,
        GeoSource::FromLoc,
        0,
        Utc::now(),
    ))
    .unwrap();
    assert_eq!(
        prepare_add_watch("144,te1st , jo50aa,1,TE2ND,,0", "TE9MY", db, 1000),
        Some(vec![
            entry("TE1ST", "JO50AA", true),
            entry("TE2ND", "JO60AA", false)
        ])
    );
    assert_eq!(
        prepare_add_watch("144,TE1ST,JO50AA", "TE9MY", db, 1000),
        None
    );
    assert_eq!(prepare_add_watch("nocomma", "TE9MY", db, 1000), None);
    assert_eq!(
        prepare_add_watch("144,TE9MY,JO50AA,1", "TE9MY", db, 1000),
        Some(vec![])
    );
    assert_eq!(
        prepare_add_watch("144,1,JO50AA,1", "TE9MY", db, 1000),
        Some(vec![])
    );
}

#[test]
fn prepare_watchlist_pairs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    assert_eq!(
        prepare_watchlist("144,te1st , jo50aa,TE2ND", "TE9MY", &paths.stations, 1000),
        vec![entry("TE1ST", "JO50AA", false)]
    );
    assert_eq!(
        prepare_watchlist("144", "TE9MY", &paths.stations, 1000),
        vec![]
    );
}

#[test]
fn prepare_remove_watch_forms() {
    assert_eq!(prepare_remove_watch("144"), RemoveWatch::All);
    assert_eq!(
        prepare_remove_watch("144,te1st,TE2ND"),
        RemoveWatch::Calls(vec!["TE1ST".into(), "TE2ND".into()])
    );
}

#[test]
fn bind_failure_reports_error_once_and_retries() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let ctx = WinTestCtx {
        paths,
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(NearestSettings::from_settings(
            &Settings::default(),
        ))),
        tx,
        generation: 1,
    };
    let mut cfg = config(false, "127.0.0.1:9".parse().unwrap());
    // Named exception to "bind 127.0.0.1 only": 192.0.2.1 is TEST-NET-1, never a local address,
    // so the bind fails with EADDRNOTAVAIL and nothing is ever sent.
    cfg.bind = "192.0.2.1:0".parse().unwrap();
    let svc = WinTestService::start(cfg, ctx);
    match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
        ServiceEvent::Error(e) => assert!(e.contains("cannot listen"), "{e}"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
    assert_eq!(svc.local_addr(), None);
    svc.stop();
}

#[test]
fn stop_releases_port() {
    let r = rig(false);
    let port = r.addr.port();
    r.svc.stop();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        match UdpSocket::bind(("127.0.0.1", port)) {
            Ok(_) => break,
            Err(e) => {
                assert!(Instant::now() < end, "port still held: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[test]
fn two_receivers_share_a_port() {
    let r = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, _rx) = channel();
    let ctx = WinTestCtx {
        paths,
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(NearestSettings::from_settings(
            &Settings::default(),
        ))),
        tx,
        generation: 1,
    };
    let mut cfg = config(false, "127.0.0.1:9".parse().unwrap());
    cfg.bind = r.addr;
    let second = WinTestService::start(cfg, ctx);
    assert_eq!(wait_addr(&second), Some(r.addr));
}

#[test]
fn watch_messages_are_capped_before_db_writes() {
    let calls = ["TE1ST", "TE2ND", "TE3RD", "TE4TH"];
    let wl: String = calls.iter().map(|c| format!(",{c},JO50AA")).collect();
    let add: String = calls.iter().map(|c| format!(",{c},JO50AA,0")).collect();
    for kind in 0..2 {
        let dir = tempfile::tempdir().unwrap();
        let paths = open_data(dir.path()).unwrap();
        let out = if kind == 0 {
            prepare_watchlist(&format!("144{wl}"), "TE9MY", &paths.stations, 3)
        } else {
            prepare_add_watch(&format!("144{add}"), "TE9MY", &paths.stations, 3).unwrap()
        };
        assert_eq!(out.len(), 3);
        assert_eq!(paths.stations.location_count().unwrap(), 3);
    }
    let cfg = WinTestConfig::from_settings(&Settings::default(), NetAddrs::loopback());
    assert_eq!(cfg.watchlist_max, 1000);
}
