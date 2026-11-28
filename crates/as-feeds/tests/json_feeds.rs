//! JSON web feeds against recorded/synthetic fixtures served by mockito
//! (`tests/fixtures/feeds/`, see its README).
use as_aircraft::AltitudeProvenance;
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_golden::{deltas::DeltaManifest, load};
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
    assert_eq!(p.altitude_provenance, AltitudeProvenance::PressureEstimate);
    assert_eq!(p.track, 245.71);
    assert_eq!(p.speed_kts, 482.3);
    assert_eq!(p.time, Some(ms(1_795_435_200_000 - 1200)));
    assert_eq!(p.category, 4); // A5, but A388 is superheavy
    assert!(!p.ground);
    let g = find(&planes, "3C66B3");
    assert_eq!(g.alt_ft, f64::from(i32::MIN));
    assert!(g.ground);
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
fn finding_52_readsb_and_dump1090_golden_deltas() {
    use serde_json::json;

    let fixtures = load("feed_json.json");
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area3/golden-deltas.tsv", "feed_json");
    for (index, case) in fixtures.cases.iter().enumerate() {
        let body = fixture(&case.s("fixture"));
        let (feed_name, altitude_key) = match case.func.as_str() {
            "readsb_deserialize" => ("Readsb", "alt_baro"),
            "dump1090_deserialize" => ("Dump1090", "altitude"),
            other => panic!("unhandled golden case {other}"),
        };
        let filtered = case.args.get("without_ground").is_some();
        let body = if filtered {
            without_ground(&body, altitude_key)
        } else {
            body
        };
        let mut server = Server::new();
        let mock = serve(&mut server, "/aircraft", &body);
        let batch = feed(feed_name, &url_setting(&server, "/aircraft"))
            .get_batch(&args())
            .unwrap();
        mock.assert();
        let ground = batch.planes.iter().filter(|plane| plane.ground).count();
        // Every full fixture has seven wire objects. One has no position age,
        // and one of the remaining six explicitly reports ground.
        let (actual, reference) = if filtered {
            assert_eq!((batch.planes.len(), ground), (5, 0));
            (json!(batch.decoded), case.result.clone())
        } else {
            assert_eq!(batch.planes.len() - ground, 5);
            (
                json!({"decoded":batch.decoded,"mapped":batch.planes.len(),"ground":ground}),
                json!({"decoded":7,"mapped":6,"ground":1}),
            )
        };
        assert_eq!(
            actual,
            deltas.expected(index, "result", &case.result, reference, 0.0, "#52")
        );
    }
    deltas.finish();
}

fn check_companion_wire_fields(body: &str, planes: &[FeedPlane]) {
    let root: serde_json::Value = serde_json::from_str(body).unwrap();
    for row in root["aircraft"].as_array().unwrap() {
        let Some(age) = row["seen_pos"].as_f64() else {
            continue;
        };
        let hex = row["hex"].as_str().unwrap().to_ascii_uppercase();
        let plane = find(planes, &hex);
        assert_eq!(plane.call, row["flight"].as_str().unwrap_or("").trim());
        let category = match row["category"].as_str() {
            Some("A1" | "A2") => 1,
            Some("A3" | "A4") => 2,
            Some("A5") => 3,
            _ => 0,
        };
        assert_eq!(plane.category, category, "{hex}");
        assert_eq!(
            plane.time,
            Some(ms(1_795_435_200_700 - (age * 1000.0).round() as i64))
        );
    }
}

#[test]
fn readsb_maps_fixture() {
    let body = without_ground(&fixture("readsb_aircraft.json"), "alt_baro");
    let mut server = Server::new();
    serve(&mut server, "/tar1090/data/aircraft.json", &body);
    let planes = get(
        "Readsb",
        &url_setting(&server, "/tar1090/data/aircraft.json"),
    )
    .unwrap();
    assert_eq!(planes.len(), 5);
    check_companion_wire_fields(&body, &planes);
    let p = find(&planes, "3C6589");
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.speed_kts, 482.3);
    assert_eq!(p.track, 245.71);
    assert_eq!(p.lat, 50.612345);
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
fn dump1090_maps_fixture() {
    let body = without_ground(&fixture("dump1090_aircraft.json"), "altitude");
    let mut server = Server::new();
    serve(&mut server, "/data/aircraft.json", &body);
    let planes = get("Dump1090", &url_setting(&server, "/data/aircraft.json")).unwrap();
    assert_eq!(planes.len(), 5);
    let p = find(&planes, "3C6589");
    check_companion_wire_fields(&body, &planes);
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.speed_kts, 482.0);
    assert_eq!(p.track, 245.0);
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
    assert_eq!(p.reg, "D-AIMA");
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
    assert_eq!(planes.len(), 3);
    assert_eq!(find(&planes, "3C4DD4").reg, "D-AEWM");
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
    assert_eq!(
        find(&planes, "4ca7b2").altitude_provenance,
        AltitudeProvenance::Geometric
    );
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

#[test]
fn finding_329_rejects_direct_and_redirected_self_feeds() {
    use as_feeds::feeds::airscoutserver::AirScoutServer;
    use std::net::{Ipv4Addr, SocketAddr};

    let mut server = mockito::Server::new();
    let port = server.socket_address().port();
    let relay = server
        .mock("GET", "/relay")
        .with_status(302)
        .with_header("Location", "/planes.json")
        .create();
    let self_endpoint = server.mock("GET", "/planes.json").expect(0).create();
    let mut feed = AirScoutServer {
        url: format!("http://localhost:{port}/planes.json"),
        ..Default::default()
    };
    let mut args = FeedArgs {
        airscout_listener: Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))),
        ..Default::default()
    };
    let error = feed.start(&args).unwrap_err();
    assert!(error.to_string().contains("this process"));
    feed.url = format!("{}/relay", server.url());
    feed.start(&args).unwrap();
    let error = feed.get_planes(&args).unwrap_err();
    assert!(error.to_string().contains("this process"));
    relay.assert();
    self_endpoint.assert();

    args.airscout_listener = Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    feed.url = format!("http://127.0.0.1:{port}/PLANES.JSON");
    assert!(feed.start(&args).is_err());
    args.airscout_listener = None;
    assert!(feed.start(&args).is_ok());
}

#[test]
fn finding_52_maps_ground_without_losing_airborne_neighbors() {
    for (parse, field) in [
        (
            as_feeds::feeds::readsb::parse as fn(&str) -> Result<Vec<FeedPlane>, FeedError>,
            "alt_baro",
        ),
        (
            as_feeds::feeds::dump1090::parse as fn(&str) -> Result<Vec<FeedPlane>, FeedError>,
            "altitude",
        ),
    ] {
        let body = format!(
            r#"{{"now":100.7,"aircraft":[
            {{"hex":"3C0001","seen_pos":0,"{field}":"ground"}},
            {{"hex":"3C0002","seen_pos":0,"{field}":20000}}
        ]}}"#
        );
        let planes: Vec<FeedPlane> = parse(&body).unwrap();
        assert_eq!(planes.len(), 2);
        assert!(planes[0].ground);
        assert!(!planes[1].ground);
        assert_eq!(planes[1].alt_ft, 20000.0);
    }
}

#[test]
fn finding_54_subtracts_age_from_fractional_epoch() {
    let json = r#"{"now":100.700,"aircraft":[{"hex":"3C0001","seen_pos":1.200}]}"#;
    let readsb = as_feeds::feeds::readsb::parse(json).unwrap();
    let dump1090 = as_feeds::feeds::dump1090::parse(json).unwrap();
    for planes in [&readsb, &dump1090] {
        assert_eq!(planes[0].time, Some(ms(99_500)));
    }
}

#[test]
fn finding_55_callsign_survives_readsb_and_dump1090_mapping() {
    let body = r#"{"now":100,"aircraft":[
        {"hex":"3C0001","seen_pos":0,"flight":" TE1ST  ","type":"adsb_icao"},
        {"hex":"3C0002","seen_pos":0,"flight":null},
        {"hex":"3C0003","seen_pos":0,"flight":"  "}
    ]}"#;
    for planes in [
        as_feeds::feeds::readsb::parse(body).unwrap(),
        as_feeds::feeds::dump1090::parse(body).unwrap(),
    ] {
        assert_eq!(planes[0].call, "TE1ST");
        assert!(planes[0].type_code.is_empty());
        assert!(planes[1].call.is_empty() && planes[2].call.is_empty());
    }
}

#[test]
fn finding_59_distinguishes_ground_unknown_and_zero() {
    let body = r#"{"now":100000,"ac":[
        {"hex":"3C0001","seen_pos":0,"alt_baro":"ground"},
        {"hex":"3C0002","seen_pos":0,"alt_baro":"bad"},
        {"hex":"3C0003","seen_pos":0,"alt_baro":0}
    ]}"#;
    let planes = as_feeds::feeds::adsbone::parse(body).unwrap();
    assert!(planes[0].ground);
    assert!(!planes[1].ground && !planes[2].ground);
    assert_eq!(planes[0].alt_ft, f64::from(i32::MIN));
    assert_eq!(planes[1].alt_ft, f64::from(i32::MIN));
    assert_eq!(planes[2].alt_ft, 0.0);
}

#[test]
fn finding_60_keeps_registration_in_lists_and_legacy_records() {
    use as_feeds::feeds::airscoutserver::parse;
    let list = r#"[{"Hex":"3C0001","Reg":" G-TEST "},{"Hex":"3C0002","Reg":null}]"#;
    let list = parse(list, ms(100_000)).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].reg, "G-TEST");
    assert!(list[1].reg.is_empty());

    let legacy = r#"{
        "version" : 1,
        "full_count" : 1,
        "custom-id" : ["3C0001",50,10,45,20000,450,"","","A388","G-TEST",100,"","","",0,0,"TE1ST"]
    }"#;
    let legacy = parse(legacy, ms(100_000)).unwrap();
    assert_eq!(legacy.len(), 1);
    assert_eq!(legacy[0].reg, "G-TEST");
}

#[test]
fn finding_60_validates_empty_and_malformed_envelopes() {
    use as_feeds::feeds::airscoutserver::parse;
    for empty in ["[]", r#"{"version":1,"full_count":0,"stats":{}}"#] {
        assert!(parse(empty, ms(100_000)).unwrap().is_empty());
    }
    for bad in [
        "{",
        "[null]",
        "{}",
        r#"{"version":2,"full_count":0}"#,
        r#"{"version":1,"full_count":-1}"#,
    ] {
        assert!(parse(bad, ms(100_000)).is_err(), "{bad}");
    }
}

#[test]
fn finding_380_maps_full_width_utc_seconds_in_all_three_layouts() {
    for epoch in [2_147_483_647_i64, 2_147_483_648] {
        let expected = Some(ms(epoch * 1000));
        let modern = format!(r#"[{{"Hex":"3C0001","Time":{epoch}}}]"#);
        let legacy = format!(
            r#"{{"version":1,"full_count":1,"0001":["3C0001",50,10,45,20000,450,"","","A388","G-TEST",{epoch},"","","",0,0,"TE1ST"]}}"#
        );
        let open = format!(
            r#"{{"states":[["3C0001","TE1ST","", {epoch},{epoch},10,50,7000,false,220,45,0,null,7000,null,false,0]]}}"#
        );
        for body in [modern, legacy] {
            assert_eq!(
                as_feeds::feeds::airscoutserver::parse(&body, ms(100_000)).unwrap()[0].time,
                expected
            );
        }
        assert_eq!(
            as_feeds::feeds::opensky::parse(&open).unwrap()[0].time,
            expected
        );
    }
}

#[test]
fn finding_378_isolates_decode_and_mapping_failures_in_typed_feeds() {
    use as_feeds::feeds::{adsbsupport, vrs};
    let vrs_json = r#"{"acList":[
        {"Icao":"3C0001","PosTime":100000,"Lat":50,"Long":10,"Alt":20000},
        null,{"Icao":[]},{"Icao":"3C0002","Lat":1e400},
        {"Icao":"3C0003","PosTime":100000,"Alt":2147483648}
    ]}"#;
    let support_json = r#"{"planes":[
        {"hex":"3C0001","receivedAt":100000,"latitude":50,"longitude":10,"altitude_barometric":20000},
        null,{"hex":[]},{"hex":"3C0002","latitude":1e400},
        {"hex":"3C0003","receivedAt":100000,"altitude_barometric":2147483648}
    ]}"#;
    let mut server = Server::new();
    let request = serve(&mut server, "/mixed", vrs_json);
    let batch = feed("VRSWebServer", &url_setting(&server, "/mixed"))
        .get_batch(&args())
        .unwrap();
    request.assert();
    assert_eq!(
        (batch.planes.len(), batch.decoded, batch.rejected),
        (1, 2, 4)
    );
    assert_eq!(vrs::parse(vrs_json, false).unwrap().0.len(), 1);
    assert_eq!(adsbsupport::parse(support_json, false).unwrap().len(), 1);
    assert!(vrs::parse(r#"{"acList":[null]}"#, false).is_err());
    assert!(adsbsupport::parse(r#"{"planes":[null]}"#, false).is_err());
    assert!(vrs::parse(r#"{"acList":[]}"#, false).unwrap().0.is_empty());
    assert!(adsbsupport::parse(r#"{"planes":[]}"#, false)
        .unwrap()
        .is_empty());
    assert!(vrs::parse(r#"{"acList":[{}"#, false).is_err());
    assert!(adsbsupport::parse(r#"{"planes":[{}"#, false).is_err());
}

#[test]
fn finding_384_negative_json_age_does_not_turn_into_a_future_observation() {
    use as_feeds::feeds::{adsbone, dump1090, readsb};

    let records = r#"[
        {"hex":"3C0001","seen_pos":0.250},
        {"hex":"3C0002","seen_pos":-0.250},
        {"hex":"3C0003","seen_pos":-0.00001}
    ]"#;
    for reports in [
        adsbone::parse(&format!(r#"{{"now":100500,"ac":{records}}}"#)).unwrap(),
        dump1090::parse(&format!(r#"{{"now":100.500,"aircraft":{records}}}"#)).unwrap(),
        readsb::parse(&format!(r#"{{"now":100.500,"aircraft":{records}}}"#)).unwrap(),
    ] {
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].hex, "3C0001");
        assert_eq!(reports[0].time, Some(ms(100_250)));
    }
}
