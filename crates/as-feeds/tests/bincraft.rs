//! binCraft parsers against the C# BinCraftParser / PublicBinCraftParser
//! (`tests/golden/bincraft.json`), and both feeds through mockito with zstd.
use as_feeds::feeds::bincraft_parse::{parse_bincraft, parse_public};
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_golden::{load, num, Case};
use chrono::{DateTime, Duration, Utc};
use mockito::Server;
use serde_json::Value;

fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("bad base64 {c}"),
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c) << (18 - 6 * i);
        }
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

fn data(c: &Case) -> Vec<u8> {
    b64(&c.s("data"))
}

fn case(func: &str, name: &str) -> Case {
    let g = load("bincraft.json");
    let c = g
        .by_fn(func)
        .find(|c| c.args["name"] == name)
        .unwrap_or_else(|| panic!("{func}/{name}"));
    Case {
        func: c.func.clone(),
        args: c.args.clone(),
        result: c.result.clone(),
    }
}

fn f(v: &Value) -> f64 {
    num(v).unwrap_or_else(|| panic!("not a number: {v}"))
}

/// `AddSeconds(-s)` rounded to whole milliseconds.
fn before(t: DateTime<Utc>, s: f64) -> DateTime<Utc> {
    t - Duration::milliseconds((s * 1000.0).round() as i64)
}

#[test]
fn parse_bincraft_golden() {
    let g = load("bincraft.json");
    let mut n = 0;
    for c in g.by_fn("bincraft") {
        let r = parse_bincraft(&data(c));
        if c.is_error() {
            assert!(
                matches!(r, Err(FeedError::Parse(_))),
                "{}: {r:?}",
                c.args["name"]
            );
            continue;
        }
        let planes = r.unwrap_or_else(|e| panic!("{}: {e}", c.args["name"]));
        let acs = c.result["aircraft"].as_array().unwrap();
        assert_eq!(planes.len(), acs.len());
        let now = DateTime::from_timestamp_millis((f(&c.result["now"]) * 1000.0) as i64).unwrap();
        for (p, e) in planes.iter().zip(acs) {
            assert_eq!(p.hex, e["hex"].as_str().unwrap());
            assert_eq!(p.lat, f(&e["lat"]));
            assert_eq!(p.lon, f(&e["lon"]));
            assert_eq!(p.alt_ft, f(&e["alt_baro"]));
            // suspect offsets reproduced: track @24, gs @26, seen_pos @54
            assert_eq!(p.track, f(&e["track"]));
            assert_eq!(p.speed_kts, f(&e["gs"]));
            assert_eq!(p.time, Some(before(now, f(&e["seen_pos"]))));
            assert_eq!(p.call, "");
            assert!(!p.ground);
            n += 1;
        }
    }
    assert_eq!(n, 5); // three_records + partial_tail
}

#[test]
fn parse_public_golden() {
    let g = load("bincraft.json");
    let mut n = 0;
    for c in g.by_fn("public_bincraft") {
        let r = parse_public(&data(c));
        if c.is_error() {
            assert!(
                matches!(r, Err(FeedError::Parse(_))),
                "{}: {r:?}",
                c.args["name"]
            );
            continue;
        }
        let planes = r.unwrap_or_else(|e| panic!("{}: {e}", c.args["name"]));
        let now = DateTime::from_timestamp_millis(c.result["now_ms"].as_i64().unwrap()).unwrap();
        // records without a valid position are skipped
        let acs: Vec<&Value> = c.result["aircraft"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| !a["lat"].is_null() && !a["lon"].is_null())
            .collect();
        assert_eq!(planes.len(), acs.len());
        for (p, e) in planes.iter().zip(acs) {
            let opt = |k: &str| num(&e[k]);
            assert_eq!(p.hex, e["hex"].as_str().unwrap());
            assert_eq!(p.lat, f(&e["lat"]));
            assert_eq!(p.lon, f(&e["lon"]));
            assert_eq!(
                p.alt_ft,
                opt("baro_alt").or(opt("geom_alt")).unwrap_or(f64::MIN)
            );
            assert_eq!(p.track, opt("track").unwrap_or(f64::MIN));
            assert_eq!(p.speed_kts, opt("gs").unwrap_or(f64::MIN));
            assert_eq!(p.call, e["callsign"].as_str().unwrap_or(""));
            assert_eq!(p.reg, e["reg"].as_str().unwrap_or(""));
            assert_eq!(p.type_code, e["type"].as_str().unwrap_or(""));
            assert_eq!(p.vspeed, opt("baro_rate").map_or(0, |v| v as i32));
            assert_eq!(p.ground, e["air_ground"] == 1);
            let seen = opt("seen_pos").or(opt("seen")).unwrap_or(0.0);
            assert_eq!(p.time, Some(before(now, seen)));
            n += 1;
        }
    }
    // three_records: 2 with position; partial_tail: 2 records, both with position
    assert_eq!(n, 4);
}

#[test]
fn record_count_exceeds_buffer_is_err() {
    // stride 32: the second record's fields reach past the end of the buffer
    let c = case("bincraft", "stride_too_small");
    assert!(c.is_error());
    assert!(matches!(
        parse_bincraft(&data(&c)),
        Err(FeedError::Parse(_))
    ));
    assert!(matches!(parse_public(&data(&c)), Err(FeedError::Parse(_))));
    // stride 0 loops forever in the C# BinCraft parser; here it is an error
    let mut d = data(&case("bincraft", "three_records"));
    d[8..12].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(parse_bincraft(&d), Err(FeedError::Parse(_))));
    // the public parser returns no aircraft for stride 0, as in C#
    assert_eq!(parse_public(&d).unwrap().len(), 0);
}

fn args() -> FeedArgs {
    FeedArgs {
        min_lat: 50.0,
        max_lat: 51.0,
        min_lon: 10.0,
        max_lon: 12.0,
        min_alt_m: 0,
        max_alt_m: 13000,
        my_lat: 50.5,
        my_lon: 11.0,
        ..FeedArgs::default()
    }
}

fn feed(name: &str, settings: &str) -> Box<dyn PlaneFeed> {
    let mut f = feed_by_name(name).unwrap_or_else(|| panic!("{name} not registered"));
    f.load_settings(&settings.parse().unwrap()).unwrap();
    f.start(&args()).unwrap();
    f
}

fn three_records() -> Vec<u8> {
    data(&case("bincraft", "three_records"))
}

fn get(name: &str, settings: &str) -> Result<Vec<FeedPlane>, FeedError> {
    feed(name, settings).get_planes(&args())
}

#[test]
fn zstd_roundtrip_through_feed() {
    let body = zstd::encode_all(three_records().as_slice(), 3).unwrap();
    let mut server = Server::new();
    let m1 = server
        .mock("GET", "/tar1090/data/aircraft.binCraft.zst")
        .with_body(&body)
        .create();
    let s = format!(
        "url = \"{}/tar1090/data/aircraft.binCraft.zst\"",
        server.url()
    );
    let planes = get("BinCraft", &s).unwrap();
    m1.assert();
    assert_eq!(planes.len(), 3);
    assert_eq!(planes[0].hex, "3C6589");

    // the box comes from the covered area, the path is fixed
    let m2 = server
        .mock("GET", "/re-api/?binCraft&zstd&box=50,51,10,12")
        .match_header("user-agent", as_net::user_agent().as_str())
        .with_body(&body)
        .create();
    let s = format!("host = \"{}/ignored/path\"", server.url());
    let planes = get("PublicBinCraft", &s).unwrap();
    m2.assert();
    assert_eq!(planes.len(), 2);
    assert_eq!(planes[0].call, "DLH4AB");
}

#[test]
fn uncompressed_setting() {
    let mut server = Server::new();
    server
        .mock("GET", "/raw")
        .with_body(three_records())
        .create();
    let s = format!("url = \"{}/raw\"\ncompressed = false", server.url());
    assert_eq!(get("BinCraft", &s).unwrap().len(), 3);
}

#[test]
fn empty_body_is_err() {
    let mut server = Server::new();
    server.mock("GET", "/e").with_body("").create();
    let s = format!("url = \"{}/e\"", server.url());
    assert!(matches!(get("BinCraft", &s), Err(FeedError::Parse(_))));
    assert!(matches!(parse_bincraft(&[]), Err(FeedError::Parse(_))));
    assert!(matches!(parse_public(&[]), Err(FeedError::Parse(_))));
}

#[test]
fn bad_zstd_magic_is_err() {
    let mut server = Server::new();
    // uncompressed data while `compressed` is on
    server.mock("GET", "/z").with_body(three_records()).create();
    let s = format!("url = \"{}/z\"", server.url());
    assert!(matches!(get("BinCraft", &s), Err(FeedError::Parse(_))));
    server
        .mock("GET", "/re-api/?binCraft&zstd&box=50,51,10,12")
        .with_body(three_records())
        .create();
    let s = format!("host = \"{}\"", server.url());
    assert!(matches!(
        get("PublicBinCraft", &s),
        Err(FeedError::Parse(_))
    ));
}

#[test]
fn http_500_is_err() {
    let mut server = Server::new();
    server.mock("GET", "/x").with_status(500).create();
    let s = format!("url = \"{}/x\"", server.url());
    assert!(matches!(get("BinCraft", &s), Err(FeedError::Http(_))));
}

#[test]
fn bincraft_disabled_by_default() {
    let b = feed_by_name("BinCraft").unwrap();
    assert!(!b.enabled_by_default());
    assert_eq!(
        b.settings()["url"].as_str(),
        Some("http://44.149.131.185/tar1090/data/aircraft.binCraft.zst")
    );
    assert_eq!(b.settings()["compressed"].as_bool(), Some(true));
    let p = feed_by_name("PublicBinCraft").unwrap();
    assert!(!p.enabled_by_default());
    assert_eq!(p.settings()["host"].as_str(), Some("http://localhost"));
    // no user-agent setting: the honest one is always used
    assert!(p.settings().get("user_agent").is_none());
}

#[test]
fn publicbincraft_defaults() {
    assert_eq!(
        as_feeds::feeds::publicbincraft::DEFAULT_HOST,
        "http://localhost"
    );
    let p = feed_by_name("PublicBinCraft").unwrap();
    assert!(!p.enabled_by_default());
    // PublicBinCraft.cs at fbadef0: six C# string pieces joined with +; \n and \" are C# escapes
    let upstream = [
        "This plane feed might fetch data from an Internet server via the binCraft endpoint.\n",
        "The use is probably not intended by the website owners and could be changed in URL and data format frequently and without further notice.\n",
        "Furthermore, it might cause legal issues in some countries.\n",
        "By clicking on \"Accept\" you understand that you are\n\n",
        "DOING THAT ON YOUR OWN RISK\n\n",
        "The auhor of this software will not be responsible in any case.",
    ]
    .concat();
    assert_eq!(p.disclaimer(), Some(upstream.as_str()));
}

#[test]
fn publicbincraft_default_host_nothing_listening_is_err() {
    // the default host builds the expected URL
    let p = as_feeds::feeds::publicbincraft::PublicBinCraft::default();
    assert_eq!(
        p.url(&args()).unwrap(),
        "http://localhost/re-api/?binCraft&zstd&box=50,51,10,12"
    );
    // a closed port on the same kind of host: an error, no credential, the feed stays usable
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let s = format!("host = \"http://localhost:{port}\"");
    let mut f = feed("PublicBinCraft", &s);
    for _ in 0..2 {
        let e = f.get_planes(&args()).unwrap_err();
        assert!(
            matches!(e, FeedError::Http(_) | FeedError::Timeout),
            "{e:?}"
        );
        let text = format!("{e} {e:?}");
        assert!(
            !text.contains("password") && !text.contains("token"),
            "{text}"
        );
    }
}
