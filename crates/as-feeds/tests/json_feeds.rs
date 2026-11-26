//! JSON web feeds against recorded/synthetic fixtures served by mockito
//! (`tests/fixtures/feeds/`, see its README).
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_golden::load;
use chrono::{DateTime, TimeZone, Utc};
use mockito::{Matcher, Server};

fn fixture(name: &str) -> String {
    let p = as_golden::repo_root()
        .join("tests/fixtures/feeds")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"))
}

/// JO50 area; my position is the locator centre (no user data).
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
    let mut f = feed_by_name(name).unwrap_or_else(|| panic!("feed {name} not registered"));
    let t: toml::Table = settings.parse().unwrap();
    f.load_settings(&t).unwrap();
    f
}

fn ms(v: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(v).unwrap()
}

fn find<'a>(planes: &'a [FeedPlane], hex: &str) -> &'a FeedPlane {
    planes
        .iter()
        .find(|p| p.hex == hex)
        .unwrap_or_else(|| panic!("{hex} missing in {planes:?}"))
}

/// The fixture without aircraft whose altitude field is a string ("ground").
fn without_ground(json: &str, key: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
    v["aircraft"]
        .as_array_mut()
        .unwrap()
        .retain(|a| !a[key].is_string());
    v.to_string()
}

fn serve(server: &mut Server, path: &str, body: &str) -> mockito::Mock {
    server
        .mock("GET", path)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create()
}

fn get(name: &str, settings: &str) -> Result<Vec<FeedPlane>, FeedError> {
    let mut f = feed(name, settings);
    f.start(&args())?;
    f.get_planes(&args())
}

fn url_setting(server: &Server, path: &str) -> String {
    format!("url = \"{}{}\"", server.url(), path)
}

fn http_500_is_err(name: &str) {
    let mut server = Server::new();
    let m = server.mock("GET", "/x").with_status(500).create();
    let r = get(name, &url_setting(&server, "/x"));
    m.assert();
    assert!(matches!(r, Err(FeedError::Http(_))), "{r:?}");
}

fn truncated_json_is_err(name: &str, fixture_name: &str) {
    let body = fixture(fixture_name);
    let mut server = Server::new();
    serve(&mut server, "/x", &body[..body.len() / 2]);
    let r = get(name, &url_setting(&server, "/x"));
    assert!(matches!(r, Err(FeedError::Parse(_))), "{r:?}");
}

// ---------------------------------------------------------------- ADSBOne

#[test]
fn adsbone_maps_fixture() {
    let mut server = Server::new();
    // %MYLAT%/%MYLON%/%MAXDISTKM% are substituted into the path
    let m = server
        .mock(
            "GET",
            Matcher::Regex(r"^/v2/point/50\.5/11/[0-9.]+$".into()),
        )
        .with_status(200)
        .with_body(fixture("adsbone.json"))
        .create();
    let url = format!(
        "url = \"{}/v2/point/%MYLAT%/%MYLON%/%MAXDISTKM%\"",
        server.url()
    );
    let planes = get("ADSBOne", &url).unwrap();
    m.assert();
    // 8 aircraft, one without seen_pos
    assert_eq!(planes.len(), 7);
    let p = find(&planes, "3C6589");
    assert_eq!(p.call, "DLH4AB");
    assert_eq!(p.reg, "D-AIMA");
    assert_eq!(p.type_code, "A388");
    assert_eq!(p.manufacturer, "Airbus");
    assert_eq!(p.model, "A-380-800");
    assert_eq!(p.lat, 50.612345);
    assert_eq!(p.lon, 10.987654);
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.track, 245.71);
    assert_eq!(p.speed_kts, 482.3);
    assert_eq!(p.time, Some(ms(1_795_435_200_000 - 1200)));
    assert_eq!(p.category, 4); // A5, but A388 is superheavy
    assert!(!p.ground);
    // "ground" altitude becomes 0 and the plane is not flagged as on ground
    let g = find(&planes, "3C66B3");
    assert_eq!(g.alt_ft, 0.0);
    assert!(!g.ground);
    // desc without a space goes to Model only
    let b = find(&planes, "44CD71");
    assert_eq!(b.model, "A330-300");
    assert_eq!(b.manufacturer, "");
    assert_eq!(b.reg, "");
    // manufacturer keeps the first letter and lowercases the rest of the first word
    let c = find(&planes, "3E1C40");
    assert_eq!(c.manufacturer, "Cessna");
    assert_eq!(c.model, "172 Skyhawk");
    assert_eq!(c.category, 1);
    assert_eq!(find(&planes, "3C4DD4").category, 1);
    assert_eq!(find(&planes, "4CA7B2").category, 2);
    // TIS-B hex is passed through upper-cased; the pipeline rejects it later
    let t = find(&planes, "~2AD3F1");
    assert_eq!(t.call, "");
    assert!(t.track == 180.0);
}

#[test]
fn adsbone_http_500_is_err() {
    http_500_is_err("ADSBOne");
}

#[test]
fn adsbone_truncated_json_is_err() {
    truncated_json_is_err("ADSBOne", "adsbone.json");
}

#[test]
#[ignore = "live network: api.adsb.one"]
fn adsbone_live() {
    let mut f = feed_by_name("ADSBOne").unwrap();
    f.start(&args()).unwrap();
    let planes = f.get_planes(&args()).unwrap();
    assert!(!planes.is_empty());
}

// ---------------------------------------------------------------- Readsb

#[test]
fn readsb_ground_matches_golden() {
    let g = load("feed_json.json");
    let full = g
        .by_fn("readsb_deserialize")
        .find(|c| c.args.get("without_ground").is_none())
        .unwrap();
    let mut server = Server::new();
    serve(&mut server, "/x", &fixture("readsb_aircraft.json"));
    let r = get("Readsb", &url_setting(&server, "/x"));
    // C# Newtonsoft cannot put "ground" into a double?: the whole response is lost
    assert!(full.is_error());
    assert!(matches!(r, Err(FeedError::Parse(_))), "{r:?}");
}

#[test]
fn readsb_maps_fixture() {
    let g = load("feed_json.json");
    let numeric = g
        .by_fn("readsb_deserialize")
        .find(|c| c.args.get("without_ground").is_some())
        .unwrap();
    let body = without_ground(&fixture("readsb_aircraft.json"), "alt_baro");
    let mut server = Server::new();
    serve(&mut server, "/tar1090/data/aircraft.json", &body);
    let planes = get(
        "Readsb",
        &url_setting(&server, "/tar1090/data/aircraft.json"),
    )
    .unwrap();
    // C# deserialises 6; the one without seen_pos is skipped
    assert_eq!(numeric.result.as_i64(), Some(6));
    assert_eq!(planes.len(), 5);
    let p = find(&planes, "3C6589");
    // callsign and category are not mapped
    assert_eq!(p.call, "");
    assert_eq!(p.category, 0);
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.speed_kts, 482.3);
    assert_eq!(p.track, 245.71);
    assert_eq!(p.lat, 50.612345);
    // now = 1795435200.7 s is truncated to whole seconds before use
    assert_eq!(p.time, Some(ms(1_795_435_200_000 - 1200)));
    assert_eq!(
        find(&planes, "3D0A11").time,
        Some(ms(1_795_435_200_000 - 250))
    );
}

#[test]
fn readsb_http_500_is_err() {
    http_500_is_err("Readsb");
}

#[test]
fn readsb_truncated_json_is_err() {
    truncated_json_is_err("Readsb", "readsb_aircraft.json");
}

// ---------------------------------------------------------------- Dump1090

#[test]
fn dump1090_ground_matches_golden() {
    let g = load("feed_json.json");
    let full = g
        .by_fn("dump1090_deserialize")
        .find(|c| c.args.get("without_ground").is_none())
        .unwrap();
    let mut server = Server::new();
    serve(&mut server, "/x", &fixture("dump1090_aircraft.json"));
    let r = get("Dump1090", &url_setting(&server, "/x"));
    assert!(full.is_error());
    assert!(matches!(r, Err(FeedError::Parse(_))), "{r:?}");
}

#[test]
fn dump1090_maps_fixture() {
    let body = without_ground(&fixture("dump1090_aircraft.json"), "altitude");
    let mut server = Server::new();
    serve(&mut server, "/data/aircraft.json", &body);
    let planes = get("Dump1090", &url_setting(&server, "/data/aircraft.json")).unwrap();
    assert_eq!(planes.len(), 5);
    let p = find(&planes, "3C6589");
    assert_eq!(p.call, ""); // "flight" is not mapped
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.speed_kts, 482.0);
    assert_eq!(p.track, 245.0);
    assert_eq!(p.time, Some(ms(1_795_435_200_000 - 1200)));
    // the if-chain without else only keeps A5 (#32 of the survey)
    assert_eq!(p.category, 3);
    for hex in ["4CA7B2", "3C4DD4", "3E1C40", "3D0A11"] {
        assert_eq!(find(&planes, hex).category, 0, "{hex}");
    }
}

#[test]
fn dump1090_http_500_is_err() {
    http_500_is_err("Dump1090");
}

#[test]
fn dump1090_truncated_json_is_err() {
    truncated_json_is_err("Dump1090", "dump1090_aircraft.json");
}

// ---------------------------------------------------------------- AirScoutServer

#[test]
fn airscoutserver_maps_fixture() {
    let mut server = Server::new();
    serve(
        &mut server,
        "/planes.json",
        &fixture("airscoutserver_planes.json"),
    );
    let planes = get("AirScoutServer", &url_setting(&server, "/planes.json")).unwrap();
    assert_eq!(planes.len(), 4);
    let p = find(&planes, "3C6589");
    assert_eq!(p.call, "DLH4AB");
    assert_eq!(p.lat, 50.612345);
    assert_eq!(p.lon, 10.987654);
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.track, 245.71);
    assert_eq!(p.speed_kts, 482.3);
    assert_eq!(p.type_code, "A388");
    assert_eq!(p.category, 4);
    assert_eq!(p.manufacturer, "AIRBUS");
    assert_eq!(p.model, "A-380-800");
    assert_eq!(p.reg, ""); // Reg is not mapped
    assert_eq!(p.time, Some(ms(1_795_435_199_000)));
    // explicit nulls: NaN position, no time
    let n = find(&planes, "3C4DD4");
    assert!(n.lat.is_nan() && n.lon.is_nan());
    assert_eq!(n.time, None);
    // missing keys keep the PlaneJSON constructor defaults (0, category NONE)
    let d = find(&planes, "3E1C40");
    assert_eq!((d.lat, d.lon, d.category), (0.0, 0.0, 0));
}

#[test]
fn airscoutserver_fr24_branch() {
    let mut server = Server::new();
    serve(
        &mut server,
        "/planes.json",
        &fixture("airscoutserver_fr24.json"),
    );
    let planes = get("AirScoutServer", &url_setting(&server, "/planes.json")).unwrap();
    // only keys starting with "0" are read
    assert_eq!(planes.len(), 2);
    let p = find(&planes, "3C6589");
    assert_eq!(p.call, "DLH400");
    assert_eq!((p.lat, p.lon), (50.612345, 10.987654));
    assert_eq!((p.track, p.alt_ft, p.speed_kts), (245.0, 37000.0, 482.0));
    assert_eq!(p.type_code, "A388");
    assert_eq!(p.category, 0);
    assert_eq!(p.time, Some(ms(1_795_435_199_000)));
}

#[test]
fn airscoutserver_short_response_is_empty() {
    let mut server = Server::new();
    serve(&mut server, "/planes.json", "[]");
    let planes = get("AirScoutServer", &url_setting(&server, "/planes.json")).unwrap();
    assert!(planes.is_empty());
}

#[test]
fn airscoutserver_http_500_is_err() {
    http_500_is_err("AirScoutServer");
}

#[test]
fn airscoutserver_truncated_json_is_err() {
    truncated_json_is_err("AirScoutServer", "airscoutserver_planes.json");
}

// ---------------------------------------------------------------- VRS

fn basic(user: &str, pw: &str) -> String {
    use std::io::Write;
    // tiny base64 encoder, so the test does not need a crate for it
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = format!("{user}:{pw}").into_bytes();
    let mut out = Vec::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                out.write_all(&[T[(n >> (18 - 6 * i) & 63) as usize]])
                    .unwrap();
            } else {
                out.push(b'=');
            }
        }
    }
    format!("Basic {}", String::from_utf8(out).unwrap())
}

#[test]
fn vrs_maps_fixture() {
    let mut server = Server::new();
    serve(&mut server, "/vrs.json", &fixture("vrs_aircraftlist.json"));
    let planes = get("VRSWebServer", &url_setting(&server, "/vrs.json")).unwrap();
    // 5 aircraft, one without PosTime
    assert_eq!(planes.len(), 4);
    let p = find(&planes, "3c6589"); // not upper-cased
    assert_eq!(p.call, "DLH4AB");
    assert_eq!(p.reg, "D-AIMA");
    assert_eq!(p.alt_ft, 37000.0);
    // Spd/Trak are float in the DTO
    assert_eq!(p.speed_kts, f64::from(482.3_f32));
    assert_eq!(p.track, f64::from(245.71_f32));
    assert_eq!(p.time, Some(ms(1_795_435_199_300)));
    assert_eq!(p.type_code, "A388");
    assert_eq!(p.manufacturer, "Airbus");
    assert_eq!(p.model, "Airbus A380 842");
    assert_eq!(p.category, 4);
    assert_eq!(p.country, "Germany");
    assert_eq!(p.from, "EDDF Frankfurt");
    assert_eq!(p.to, "KJFK New York");
    assert_eq!(p.vspeed, 64);
    assert_eq!(find(&planes, "4ca7b2").category, 2);
    assert_eq!(find(&planes, "3e1c40").category, 1);
    assert_eq!(find(&planes, "4b1a2c").category, 0);
}

#[test]
fn vrs_geo_altitude() {
    let mut server = Server::new();
    serve(&mut server, "/vrs.json", &fixture("vrs_aircraftlist.json"));
    let s = format!("{}\nuse_geo_alt = true", url_setting(&server, "/vrs.json"));
    let planes = get("VRSWebServer", &s).unwrap();
    assert_eq!(find(&planes, "4ca7b2").alt_ft, 36500.0);
    // no GAlt: int.MinValue
    assert_eq!(find(&planes, "3e1c40").alt_ft, f64::from(i32::MIN));
}

#[test]
fn vrs_lastdv_feeds_next_url() {
    let mut server = Server::new();
    let first = serve(
        &mut server,
        "/vrs.json?ldv=",
        &fixture("vrs_aircraftlist.json"),
    );
    let second = serve(
        &mut server,
        "/vrs.json?ldv=638676336000000000",
        &fixture("vrs_aircraftlist.json"),
    );
    let mut f = feed(
        "VRSWebServer",
        &url_setting(&server, "/vrs.json?ldv=%LASTDV%"),
    );
    f.start(&args()).unwrap();
    f.get_planes(&args()).unwrap();
    f.get_planes(&args()).unwrap();
    first.assert();
    second.assert();
}

#[test]
fn vrs_basic_auth_header() {
    let mut server = Server::new();
    let with = server
        .mock("GET", "/auth.json")
        .match_header("authorization", basic("user1", "pw1").as_str())
        .with_body(fixture("vrs_aircraftlist.json"))
        .create();
    let s = format!(
        "{}\nusername = \"user1\"\npassword = \"pw1\"",
        url_setting(&server, "/auth.json")
    );
    assert_eq!(get("VRSWebServer", &s).unwrap().len(), 4);
    with.assert();
    // a username without password sends no header
    let without = server
        .mock("GET", "/noauth.json")
        .match_header("authorization", Matcher::Missing)
        .with_body(fixture("vrs_aircraftlist.json"))
        .create();
    let s = format!(
        "{}\nusername = \"user1\"",
        url_setting(&server, "/noauth.json")
    );
    get("VRSWebServer", &s).unwrap();
    without.assert();
}

#[test]
fn vrs_failover_to_url2() {
    let mut server = Server::new();
    let down = server.mock("GET", "/one.json").with_status(503).create();
    // C# tries the second URL with the FIRST URL's credentials (#57); not reproduced: that
    // would send server 1's password to server 2
    let two = server
        .mock("GET", "/two.json")
        .match_header("authorization", basic("user2", "pw2").as_str())
        .with_body(fixture("vrs_aircraftlist.json"))
        .create();
    let s = format!(
        "url = \"{0}/one.json\"\nurl2 = \"{0}/two.json\"\nusername = \"user1\"\npassword = \"pw1\"\n\
         username2 = \"user2\"\npassword2 = \"pw2\"\nload_share = false",
        server.url()
    );
    assert_eq!(get("VRSWebServer", &s).unwrap().len(), 4);
    down.assert();
    two.assert();
}

#[test]
fn vrs_load_share_failover_uses_url2_credentials() {
    let mut server = Server::new();
    server.mock("GET", "/one.json").with_status(503).create();
    let two = server
        .mock("GET", "/two.json")
        .match_header("authorization", basic("user2", "pw2").as_str())
        .with_body(fixture("vrs_aircraftlist.json"))
        .create();
    let s = format!(
        "url = \"{0}/one.json\"\nurl2 = \"{0}/two.json\"\nusername = \"user1\"\npassword = \"pw1\"\n\
         username2 = \"user2\"\npassword2 = \"pw2\"",
        server.url()
    );
    assert_eq!(get("VRSWebServer", &s).unwrap().len(), 4);
    two.assert();
}

#[test]
fn vrs_http_500_is_err() {
    http_500_is_err("VRSWebServer");
}

#[test]
fn vrs_truncated_json_is_err() {
    truncated_json_is_err("VRSWebServer", "vrs_aircraftlist.json");
}

// ---------------------------------------------------------------- settings

#[test]
fn defaults_and_settings_roundtrip() {
    let f = feed_by_name("VRSWebServer").unwrap();
    let t = f.settings();
    assert_eq!(
        t["url"].as_str(),
        Some("http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%")
    );
    assert_eq!(t["load_share"].as_bool(), Some(true));
    assert_eq!(t["use_geo_alt"].as_bool(), Some(false));
    assert_eq!(t["timeout_s"].as_integer(), Some(30));
    for (name, url) in [
        (
            "ADSBOne",
            "https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/250",
        ),
        ("Readsb", "http://localhost/tar1090/data/aircraft.json"),
        ("Dump1090", "http://localhost:8080/data/aircraft.json"),
        ("AirScoutServer", "http://localhost:9880/planes.json"),
    ] {
        let f = feed_by_name(name).unwrap();
        assert_eq!(f.settings()["url"].as_str(), Some(url), "{name}");
        // a URL without '?' survives a reload (C# drops it, survey #35)
        let mut g = feed_by_name(name).unwrap();
        let t: toml::Table = "url = \"http://example.invalid/a.json\"".parse().unwrap();
        g.load_settings(&t).unwrap();
        assert_eq!(
            g.settings()["url"].as_str(),
            Some("http://example.invalid/a.json")
        );
    }
    let mut v = feed_by_name("VRSWebServer").unwrap();
    let bad: toml::Table = "timeout_s = \"x\"".parse().unwrap();
    assert!(matches!(v.load_settings(&bad), Err(FeedError::Config(_))));
}

#[test]
fn adsbone_default_url_is_250() {
    assert_eq!(
        as_feeds::feeds::adsbone::DEFAULT_URL,
        "https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/250"
    );
}
