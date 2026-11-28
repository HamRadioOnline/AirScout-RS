//! RTL1090 feed against a fake receiver on 127.0.0.1 (Beast and AVR).
use as_adsb::crc::parity;
use as_aircraft::db::AircraftDb;
use as_aircraft::{AltitudeProvenance, PlaneInfoCache};
use as_feeds::feeds::rtl1090::Rtl1090;
use as_feeds::pipeline::{process, PipelineConfig};
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedPlane, PlaneFeed};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const ICAO: [u8; 3] = [0x40, 0x62, 0x1D];
/// Identification KLM1023, airborne position even/odd (38000 ft, about 52.26 N 3.94 E),
/// velocity over ground (159 kts, 183°, geo-minus-baro +550 ft): the Task 7 vectors. The
/// even position is sent again at the end: only a local position fix updates the
/// decoder's timestamp (logic-findings #45), and `GetPlanes` reports updated planes only.
const MES: [[u8; 7]; 5] = [
    [0x20, 0x2C, 0xC3, 0x71, 0xC3, 0x2C, 0xE0],
    [0x58, 0xC3, 0x82, 0xD6, 0x90, 0xC8, 0xAC],
    [0x58, 0xC3, 0x86, 0x43, 0x5C, 0xC4, 0x12],
    [0x99, 0x44, 0x09, 0x94, 0x08, 0x38, 0x17],
    [0x58, 0xC3, 0x82, 0xD6, 0x90, 0xC8, 0xAC],
];

/// DF17 frame for `ICAO` with a valid parity.
fn frame(me: &[u8; 7]) -> Vec<u8> {
    let mut b = vec![0x8D];
    b.extend_from_slice(&ICAO);
    b.extend_from_slice(me);
    let p = parity(&b);
    b.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    b
}

fn beast_stream() -> Vec<u8> {
    let mut out = Vec::new();
    for me in &MES {
        let f = frame(me);
        assert!(!f.contains(&0x1A), "test frame needs Beast escaping");
        out.extend_from_slice(&[0x1A, 0x33, 0, 0, 0, 0, 0, 0, 0x80]);
        out.extend_from_slice(&f);
    }
    out
}

fn avr_stream() -> Vec<u8> {
    let mut s = String::new();
    for me in &MES {
        let hex: String = frame(me).iter().map(|b| format!("{b:02X}")).collect();
        s.push_str(&format!("*{hex};\r\n"));
    }
    s.into_bytes()
}

/// Write `data` in 3-byte pieces, then keep the connection open until `hold` has passed.
fn send_slowly(mut s: TcpStream, data: &[u8], hold: Duration) {
    for chunk in data.chunks(3) {
        s.write_all(chunk).unwrap();
        s.flush().unwrap();
        thread::sleep(Duration::from_millis(2));
    }
    thread::sleep(hold);
}

fn args() -> FeedArgs {
    FeedArgs {
        min_lat: -90.0,
        max_lat: 90.0,
        min_lon: -180.0,
        max_lon: 180.0,
        min_alt_m: 0,
        max_alt_m: 20_000,
        ..FeedArgs::default()
    }
}

fn rtl(port: u16, extra: &str) -> Rtl1090 {
    let mut f = Rtl1090::with_backoff(Duration::from_millis(100));
    let t: toml::Table = format!("server = \"127.0.0.1\"\nport = {port}\n{extra}")
        .parse()
        .unwrap();
    f.load_settings(&t).unwrap();
    f
}

/// Poll `get_planes` until a plane arrives (5 s at most).
fn wait_plane(f: &mut Rtl1090) -> FeedPlane {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(p) = f.get_planes(&args()).unwrap().into_iter().next() {
            return p;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("no plane within 5 s");
}

#[test]
fn beast_stream_yields_plane() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        send_slowly(s, &beast_stream(), Duration::from_millis(500));
    });
    let mut f = rtl(port, "");
    f.start(&args()).unwrap();
    let p = wait_plane(&mut f);
    server.join().unwrap();
    f.stop(&args()).unwrap();
    assert_eq!(p.hex, "40621D");
    // the callsign keeps its padding; the pipeline checks it later
    assert_eq!(p.call, "KLM1023 ");
    assert_eq!(p.alt_ft, 38000.0);
    assert_eq!(p.altitude_provenance, AltitudeProvenance::PressureEstimate);
    let admitted = process(
        vec![p.clone()],
        &args(),
        &PipelineConfig {
            extended_plausibility: false,
            max_error_dist: 30.0,
            log_errors: false,
        },
        &PlaneInfoCache::new(),
        &AircraftDb::open_in_memory().unwrap(),
    );
    assert_eq!(admitted.planes.len(), 1);
    assert_eq!(
        admitted.planes[0].altitude_provenance,
        AltitudeProvenance::PressureEstimate
    );
    assert!(
        (p.lat - 52.26).abs() < 0.02 && (p.lon - 3.93).abs() < 0.03,
        "{p:?}"
    );
    assert_eq!(p.speed_kts, 159.0);
    assert_eq!(p.track, 183.0);
    assert_eq!(p.reg, "[unknown]");
    assert_eq!(p.type_code, "[unknown]");
    assert_eq!(p.manufacturer, "[unknown]");
    assert_eq!(p.model, "[unknown]");
    assert_eq!(p.category, 0);
    assert!(p.time.is_some());
}

#[test]
fn avr_stream_yields_plane() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        send_slowly(s, &avr_stream(), Duration::from_millis(500));
    });
    let mut f = rtl(
        port,
        "binary = false\nuse_geometric_alt_only = true\nmark_local = true",
    );
    f.start(&args()).unwrap();
    let p = wait_plane(&mut f);
    // the finite sender must finish before the feed closes the socket (BrokenPipe race)
    server.join().unwrap();
    f.stop(&args()).unwrap();
    assert_eq!(p.call, "@KLM1023 ");
    // baro + geo-minus-baro
    assert_eq!(p.alt_ft, 38550.0);
}

#[test]
fn server_closes_then_reconnects() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        // first connection: closed at once
        let (s, _) = l.accept().unwrap();
        drop(s);
        let (s, _) = l.accept().unwrap();
        send_slowly(s, &beast_stream(), Duration::from_millis(500));
    });
    let mut f = rtl(port, "");
    f.start(&args()).unwrap();
    let p = wait_plane(&mut f);
    server.join().unwrap();
    f.stop(&args()).unwrap();
    assert_eq!(p.hex, "40621D");
}

#[test]
fn stop_is_prompt() {
    // connected to a silent receiver
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (_s, _) = l.accept().unwrap();
        thread::sleep(Duration::from_secs(3));
    });
    let mut f = rtl(port, "");
    f.start(&args()).unwrap();
    thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    f.stop(&args()).unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    server.join().unwrap();

    // waiting in the default 10 s back-off after a refused connection
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut f = Rtl1090::default();
    let t: toml::Table = format!("server = \"127.0.0.1\"\nport = {port}")
        .parse()
        .unwrap();
    f.load_settings(&t).unwrap();
    f.start(&args()).unwrap();
    thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    f.stop(&args()).unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn rtl1090_defaults() {
    let f = feed_by_name("RTL1090").expect("RTL1090 registered");
    let t = f.settings();
    assert_eq!(t["server"].as_str(), Some("localhost"));
    assert_eq!(t["port"].as_integer(), Some(31001));
    assert_eq!(t["binary"].as_bool(), Some(true));
    assert_eq!(t["use_geometric_alt_only"].as_bool(), Some(false));
    assert_eq!(t["mark_local"].as_bool(), Some(false));
}

#[test]
fn silent_receiver_reconnects() {
    // C# gives up a connection after 10 s without a complete frame and reconnects; a dead
    // link without FIN must not leave the feed silent forever (final review #3)
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        // first connection: accepted, then silent and kept open
        let (_silent, _) = l.accept().unwrap();
        let (s, _) = l.accept().unwrap();
        send_slowly(s, &beast_stream(), Duration::from_millis(500));
    });
    let mut f = Rtl1090::with_timeouts(Duration::from_millis(100), Duration::from_millis(300));
    let t: toml::Table = format!("server = \"127.0.0.1\"\nport = {port}")
        .parse()
        .unwrap();
    f.load_settings(&t).unwrap();
    f.start(&args()).unwrap();
    let p = wait_plane(&mut f);
    server.join().unwrap();
    f.stop(&args()).unwrap();
    assert_eq!(p.hex, "40621D");
}
