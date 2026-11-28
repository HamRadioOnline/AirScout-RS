//! ADSBSupport feed (`tests/fixtures/feeds/adsbsupport_planes.json`).
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, PlaneFeed};
use chrono::DateTime;
use mockito::{Matcher, Server};

fn fixture() -> String {
    std::fs::read_to_string(
        as_golden::repo_root().join("tests/fixtures/feeds/adsbsupport_planes.json"),
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
    let mut f = feed_by_name("ADSBSupport").expect("ADSBSupport registered");
    f.load_settings(&settings.parse().unwrap()).unwrap();
    f.start(&args()).unwrap();
    f
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
fn adsbsupport_basic_auth() {
    let mut server = Server::new();
    // no token header unless the user sets one; the bbox is substituted
    let m = server
        .mock(
            "GET",
            Matcher::Regex(r"^/planes\?fSBnd=50&fNBnd=51&fWBnd=10&fEBnd=12&age=300$".into()),
        )
        .match_header("authorization", basic("airscout", "pw-1").as_str())
        .match_header("token", Matcher::Missing)
        .with_body(fixture())
        .create();
    let s = format!(
        "url = \"{}/planes?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%&age=300\"\npassword = \"pw-1\"",
        server.url()
    );
    let planes = feed(&s).get_planes(&args()).unwrap();
    m.assert();
    // one entry without receivedAt is dropped
    assert_eq!(planes.len(), 3);
    let p = &planes[0];
    assert_eq!(p.hex, "3c6589");
    assert_eq!(p.call, "DLH4AB");
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.lat, 50.612345);
    assert_eq!(p.track, f64::from(245.71_f32));
    assert_eq!(p.speed_kts, f64::from(482.3_f32));
    assert_eq!(p.category, 3);
    assert_eq!(p.vspeed, 64);
    assert_eq!(p.time, DateTime::from_timestamp(1_795_435_199, 300_000_000));
    let d = &planes[2];
    assert_eq!(d.alt_ft, f64::from(i32::MIN));
    assert!(d.track.is_nan());
    assert_eq!(d.category, 1);

    let with_token = server
        .mock("GET", "/t")
        .match_header("token", "abc")
        .with_body(fixture())
        .create();
    let s = format!(
        "url = \"{}/t\"\npassword = \"pw-1\"\ntoken = \"abc\"",
        server.url()
    );
    feed(&s).get_planes(&args()).unwrap();
    with_token.assert();
}

#[test]
fn adsbsupport_geo_altitude() {
    let mut server = Server::new();
    server.mock("GET", "/p").with_body(fixture()).create();
    let s = format!(
        "url = \"{}/p\"\npassword = \"pw\"\nuse_geo_alt = true",
        server.url()
    );
    let planes = feed(&s).get_planes(&args()).unwrap();
    assert_eq!(planes[0].alt_ft, 37625.0);
    assert_eq!(planes[1].alt_ft, f64::from(i32::MIN));
}

#[test]
fn adsbsupport_no_password_is_config_err() {
    let mut server = Server::new();
    let m = server.mock("GET", "/p").expect(0).create();
    let s = format!("url = \"{}/p\"", server.url());
    let r = feed(&s).get_planes(&args());
    assert!(matches!(r, Err(FeedError::Config(_))), "{r:?}");
    m.assert();
}

#[test]
fn secrets_not_in_error_display() {
    let mut server = Server::new();
    server.mock("GET", "/p").with_status(401).create();
    let s = format!(
        "url = \"{}/p\"\npassword = \"pw-secret-42\"\ntoken = \"tok-secret-43\"",
        server.url()
    );
    let f = feed(&s);
    let mut f = f;
    let e = f.get_planes(&args()).unwrap_err();
    assert!(matches!(e, FeedError::Http(_)));
    let text = format!("{e} {e:?}");
    assert!(text.contains("401"), "{text}");
    assert!(!text.contains("pw-secret-42") && !text.contains("tok-secret-43"));
}

#[test]
fn adsbsupport_truncated_json_is_err() {
    let mut server = Server::new();
    let body = fixture();
    server
        .mock("GET", "/p")
        .with_body(&body[..body.len() / 2])
        .create();
    let s = format!("url = \"{}/p\"\npassword = \"pw\"", server.url());
    assert!(matches!(
        feed(&s).get_planes(&args()),
        Err(FeedError::Parse(_))
    ));
}

#[test]
fn adsbsupport_defaults() {
    let f = feed_by_name("ADSBSupport").unwrap();
    assert!(!f.enabled_by_default());
    assert!(f.disclaimer().unwrap().contains("credentials"));
    let t = f.settings();
    assert_eq!(
        t["url"].as_str(),
        Some("https://airscout.hamradio4.me/planes?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%&age=300")
    );
    assert_eq!(t["username"].as_str(), Some("airscout"));
    assert_eq!(t["password"].as_str(), Some(""));
    assert_eq!(t["token"].as_str(), Some(""));
    assert_eq!(t["use_geo_alt"].as_bool(), Some(false));
    assert_eq!(t["timeout_s"].as_integer(), Some(30));
}

#[test]
fn finding_67_keeps_checked_receiver_epoch_milliseconds() {
    let body = r#"{"planes":[
        {"hex":"3C0001","receivedAt":1795435199300},
        {"hex":"3C0002","receivedAt":9223372036854775807},
        {"hex":"3C0003","receivedAt":null}
    ]}"#;
    let reports = as_feeds::feeds::adsbsupport::parse(body, false).unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].time,
        chrono::DateTime::from_timestamp(1_795_435_199, 300_000_000)
    );
}
