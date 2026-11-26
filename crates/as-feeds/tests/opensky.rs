//! OpenSky feed against a recorded response (`tests/fixtures/feeds/opensky_states.json`).
use as_aircraft::AltitudeProvenance;
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::DateTime;
use mockito::{Matcher, Server};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        as_golden::repo_root()
            .join("tests/fixtures/feeds")
            .join(name),
    )
    .unwrap()
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

fn feed(settings: &str) -> Box<dyn PlaneFeed> {
    let mut f = feed_by_name("OpenSky").expect("OpenSky registered");
    f.load_settings(&settings.parse().unwrap()).unwrap();
    f.start(&args()).unwrap();
    f
}

fn find<'a>(planes: &'a [FeedPlane], hex: &str) -> &'a FeedPlane {
    planes.iter().find(|p| p.hex == hex).unwrap()
}

fn basic(user: &str, pw: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = format!("{user}:{pw}").into_bytes();
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            out.push(if i <= c.len() {
                char::from(T[(n >> (18 - 6 * i) & 63) as usize])
            } else {
                '='
            });
        }
    }
    format!("Basic {out}")
}

#[test]
fn opensky_maps_fixture() {
    let mut server = Server::new();
    // A custom endpoint is requested as configured.
    let m = server
        .mock("GET", "/api/states/all")
        .with_body(fixture("opensky_states.json"))
        .create();
    let s = format!("url = \"{}/api/states/all\"", server.url());
    let planes = feed(&s).get_planes(&args()).unwrap();
    m.assert();
    // 10 states: on ground, null time, zero speed and a short array dropped
    assert_eq!(planes.len(), 6);
    let p = find(&planes, "3C5EE2");
    assert_eq!(p.call, "EWG754");
    assert_eq!(p.lon, 10.0616);
    assert_eq!(p.lat, 50.1156);
    // geometric altitude [m] → ft
    assert_eq!(p.alt_ft, 9806.94 * 3.28084);
    // `/api/states/all` index 13 is `geo_altitude` (index 7 is `baro_altitude`), #364
    assert_eq!(p.altitude_provenance, AltitudeProvenance::Geometric);
    assert_eq!(p.track, 112.09);
    assert_eq!(p.speed_kts, 229.85 * 3.6 / 1.852);
    assert_eq!(p.time, DateTime::from_timestamp(1_795_438_420, 0));
    assert!(p.type_code.is_empty());
    assert_eq!(p.reg, "");
    assert_eq!(p.category, 0);
    // null geometric altitude: double.MinValue * 3.28084
    assert_eq!(find(&planes, "744832").alt_ft, f64::MIN * 3.28084);
    assert_eq!(
        find(&planes, "744832").altitude_provenance,
        AltitudeProvenance::Unknown
    );
    // integer altitude in the JSON
    assert_eq!(find(&planes, "440452").alt_ft, 10668.0 * 3.28084);
}

#[test]
fn finding_64_both_request_paths_expand_bounds_and_preserve_raw_escapes() {
    let mut server = Server::new();
    let template = "/api/states/all?lamin=%MINLAT%&lomin=%MINLON%&lamax=%MAXLAT%&lomax=%MAXLON%&x=%2f%26%20&tail=%DA";
    let expected = "/api/states/all?lamin=50&lomin=10&lamax=51&lomax=12&x=%2f%26%20&tail=%DA";
    let token = server
        .mock("POST", "/token")
        .with_body(r#"{"access_token":"synthetic-token","expires_in":300}"#)
        .create();
    let anonymous = server
        .mock("GET", expected)
        .match_header("authorization", Matcher::Missing)
        .with_body(r#"{"states":[]}"#)
        .create();
    let authorized = server
        .mock("GET", expected)
        .match_header("authorization", "Bearer synthetic-token")
        .with_body(r#"{"states":[]}"#)
        .create();
    let base = format!("url = \"{}{template}\"", server.url());
    let oauth = format!("{base}\ntoken_url = \"{}/token\"\noauth_client_id = \"test\"\noauth_secret = \"synthetic-secret\"", server.url());
    for settings in [&base, &oauth] {
        assert!(feed(settings).get_planes(&args()).unwrap().is_empty());
    }
    token.assert();
    anonymous.assert();
    authorized.assert();
}

#[test]
fn opensky_oauth_fetches_and_caches_token() {
    let mut server = Server::new();
    let token = server
        .mock("POST", "/token")
        .match_header("authorization", basic("my-client", "s3cr3t").as_str())
        .match_body("grant_type=client_credentials")
        .with_body(r#"{"access_token":"tok123","expires_in":1800,"token_type":"Bearer"}"#)
        .expect(1)
        .create();
    let states = server
        .mock("GET", "/api/states/all")
        .match_header("authorization", "Bearer tok123")
        .with_body(fixture("opensky_states.json"))
        .expect(2)
        .create();
    let s = format!(
        "url = \"{0}/api/states/all\"\ntoken_url = \"{0}/token\"\noauth_client_id = \"my-client\"\noauth_secret = \"s3cr3t\"",
        server.url()
    );
    let mut f = feed(&s);
    assert_eq!(f.get_planes(&args()).unwrap().len(), 6);
    assert_eq!(f.get_planes(&args()).unwrap().len(), 6);
    token.assert();
    states.assert();
}

#[test]
fn opensky_anonymous_sends_no_authorization() {
    let mut server = Server::new();
    let m = server
        .mock("GET", "/api/states/all")
        .match_header("authorization", Matcher::Missing)
        .with_body(fixture("opensky_states.json"))
        .create();
    // only one of id/secret set: anonymous
    let s = format!(
        "url = \"{}/api/states/all\"\noauth_client_id = \"my-client\"",
        server.url()
    );
    feed(&s).get_planes(&args()).unwrap();
    m.assert();
}

#[test]
fn opensky_401_is_err() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/states/all")
        .with_status(401)
        .create();
    let s = format!("url = \"{}/api/states/all\"", server.url());
    let r = feed(&s).get_planes(&args());
    assert!(matches!(r, Err(FeedError::Http(_))), "{r:?}");
}

#[test]
fn opensky_non_json_response_is_err() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/states/all")
        .with_body("Too many requests")
        .create();
    let s = format!("url = \"{}/api/states/all\"", server.url());
    assert!(matches!(
        feed(&s).get_planes(&args()),
        Err(FeedError::Parse(_))
    ));
}

#[test]
fn secrets_not_in_error_display() {
    let mut server = Server::new();
    server.mock("POST", "/token?x=1").with_status(401).create();
    let s = format!(
        "url = \"{0}/api/states/all\"\ntoken_url = \"{0}/token?x=1\"\noauth_client_id = \"my-client\"\noauth_secret = \"s3cr3t-value\"",
        server.url()
    );
    let e = feed(&s).get_planes(&args()).unwrap_err();
    let text = format!("{e} {e:?}");
    assert!(text.contains("401"), "{text}");
    assert!(!text.contains("s3cr3t-value"), "{text}");
}

#[test]
fn opensky_defaults() {
    let f = feed_by_name("OpenSky").unwrap();
    let t = f.settings();
    assert_eq!(
        t["url"].as_str(),
        Some("https://opensky-network.org/api/states/all")
    );
    assert_eq!(
        t["token_url"].as_str(),
        Some("https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token")
    );
    assert_eq!(t["oauth_client_id"].as_str(), Some(""));
    assert_eq!(t["oauth_secret"].as_str(), Some(""));
    assert!(f.enabled_by_default());
}

#[test]
#[ignore = "live network: opensky-network.org"]
fn opensky_live() {
    let mut f = feed_by_name("OpenSky").unwrap();
    f.start(&args()).unwrap();
    let planes = f.get_planes(&args()).unwrap();
    assert!(!planes.is_empty());
}

#[test]
fn huge_expires_in_does_not_panic() {
    // the token server is a user setting; its expires_in must not overflow the clock (final
    // review #2)
    let mut server = Server::new();
    server
        .mock("POST", "/token")
        .with_body(r#"{"access_token":"tok123","expires_in":18446744073709551615}"#)
        .create();
    server
        .mock("GET", "/api/states/all")
        .with_body(fixture("opensky_states.json"))
        .create();
    let s = format!(
        "url = \"{0}/api/states/all\"\ntoken_url = \"{0}/token\"\noauth_client_id = \"c\"\noauth_secret = \"s\"",
        server.url()
    );
    let mut f = feed(&s);
    assert_eq!(f.get_planes(&args()).unwrap().len(), 6);
    assert_eq!(f.get_planes(&args()).unwrap().len(), 6);
}

#[test]
fn opensky_default_url() {
    assert_eq!(
        as_feeds::feeds::opensky::DEFAULT_URL,
        "https://opensky-network.org/api/states/all"
    );
}

#[test]
fn load_settings_preserves_explicit_endpoint_for_host_migration() {
    let mut feed = feed_by_name("OpenSky").unwrap();
    let endpoint = "https://api.opensky-network.org/api/states/all?auth=%2f";
    let table = toml::Table::from_iter([("url".into(), endpoint.into())]);
    feed.load_settings(&table).unwrap();
    assert_eq!(feed.settings()["url"].as_str(), Some(endpoint));
}

fn concrete(settings: &str) -> as_feeds::feeds::opensky::OpenSky {
    let mut f = as_feeds::feeds::opensky::OpenSky::default();
    f.load_settings(&settings.parse().unwrap()).unwrap();
    f.start(&args()).unwrap();
    f
}

#[test]
fn token_not_logged() {
    const TOKEN: &str = "tok-SECRET-9f3a";
    let mut server = Server::new();
    server
        .mock("POST", "/token")
        .with_body(format!(
            r#"{{"access_token":"{TOKEN}","expires_in":1800,"token_type":"Bearer"}}"#
        ))
        .create();
    server
        .mock("GET", "/api/states/all")
        .match_header("authorization", format!("Bearer {TOKEN}").as_str())
        .with_body(fixture("opensky_states.json"))
        .create();
    let s = format!(
        "url = \"{0}/api/states/all\"\ntoken_url = \"{0}/token\"\noauth_client_id = \"my-client\"\noauth_secret = \"s3cr3t-value\"",
        server.url()
    );
    let mut f = concrete(&s);
    assert_eq!(f.get_planes(&args()).unwrap().len(), 6);
    let shown = format!("{f:?}");
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(!shown.contains("s3cr3t-value"), "{shown}");

    // error path: the states request fails after the token was fetched
    let mut bad = Server::new();
    bad.mock("POST", "/token")
        .with_body(format!(
            r#"{{"access_token":"{TOKEN}","expires_in":1800,"token_type":"Bearer"}}"#
        ))
        .create();
    bad.mock("GET", "/api/states/all").with_status(500).create();
    let s = format!(
        "url = \"{0}/api/states/all\"\ntoken_url = \"{0}/token\"\noauth_client_id = \"my-client\"\noauth_secret = \"s3cr3t-value\"",
        bad.url()
    );
    let mut f = concrete(&s);
    let e = f.get_planes(&args()).unwrap_err();
    let text = format!("{e} {e:?} {f:?}");
    assert!(text.contains("500"), "{text}");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("s3cr3t-value"), "{text}");
}

#[test]
fn finding_65_state_longitude_does_not_set_icao_type() {
    let body = r#"{"states":[["3C0001","TE1ST","",100,100,10,50,7000,false,220,45,0,null,7000,null,false,0]]}"#;
    let plane = as_feeds::feeds::opensky::parse(body).unwrap().remove(0);
    assert_eq!(plane.lon, 10.0);
    assert!(plane.type_code.is_empty());
}

#[test]
fn finding_66_null_callsign_preserves_state_with_position_time() {
    let body = r#"{"states":[["3C0001",null,"",100,100,10,50,7000,false,220,45,0,null,7000,null,false,0]]}"#;
    let planes = as_feeds::feeds::opensky::parse(body).unwrap();
    assert_eq!(planes.len(), 1);
    assert!(planes[0].call.is_empty());
    assert_eq!(planes[0].time, DateTime::from_timestamp(100, 0));
}
