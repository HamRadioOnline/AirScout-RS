//! The web API (stage 4d Task 4): settings, feed settings and table routes.
use airscout::services::open_data;
use airscout::web_api::{feed_web_name, scrub_public_url, strip_userinfo, WebApi, WebCtx};
use airscout::web_settings_json::settings_json;
use as_aircraft::db::{airports_from_json, AircraftDb};
use as_aircraft::PlaneInfoCache;
use as_settings::{Settings, WatchlistItem};
use as_web::query::Query;
use as_web::route::{route, ApiRoute, Route};
use as_web::server::Api;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

fn settings() -> Settings {
    let mut s = Settings {
        my_call: "TE1ST".into(),
        my_lat: 50.02,
        my_lon: 10.04,
        dx_call: "TE2ND".into(),
        dx_lat: 50.02,
        dx_lon: 12.04,
        ..Settings::default()
    };
    s.gui.my_calls = vec!["TE1ST".into()];
    s.watchlist.0.push(WatchlistItem {
        call: "TE2ND".into(),
        loc: "JO60AA".into(),
        checked: true,
        ..Default::default()
    });
    s
}

fn api(dir: &Path, s: Settings) -> WebApi {
    WebApi::new(WebCtx {
        paths: open_data(dir).unwrap(),
        cache: Arc::new(PlaneInfoCache::new()),
        live: Arc::new(RwLock::new(s)),
        aircraft: None,
        tmp_dir: dir.join("tmp"),
        stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

/// The body `api` answers for the raw request target `raw`.
fn get(api: &WebApi, raw: &str) -> String {
    match route(raw) {
        Route::Api(r) => api.answer(r, raw, &Query::from_raw_url(raw)),
        other => panic!("{raw} routes to {other:?}"),
    }
}

/// The settings members in `Settings.Designer.cs` declaration order.
const SETTINGS_MEMBERS: [&str; 45] = [
    "MyCall",
    "DXCall",
    "Band",
    "Planes_MinAlt",
    "Map_AutoCenter",
    "Planes_MaxAlt",
    "Planes_Filter_Min_Category",
    "MinLon",
    "MaxLon",
    "MinLat",
    "MaxLat",
    "MyLat",
    "MyLon",
    "DXLat",
    "DXLon",
    "Path_BestCaseElevation",
    "Planes_PlaneFeed2",
    "Planes_PlaneFeed3",
    "Planes_PlaneFeed1",
    "Planes_Position_TTL",
    "Server_Name",
    "Map_Provider",
    "InfoWin_Metric",
    "InfoWin_Position",
    "InfoWin_Time",
    "InfoWin_Epsilon",
    "InfoWin_Squint",
    "InfoWin_Track",
    "InfoWin_Type",
    "InfoWin_Alt",
    "InfoWin_Dist",
    "InfoWin_Speed",
    "InfoWin_Angle",
    "Planes_Filter_Max_Circumcircle",
    "Locator_MaxLength",
    "Locator_AutoLength",
    "Locator_SmallLettersForSubsquares",
    "Watchlist",
    "Watchlist_SyncWithKST",
    "Path_Band_Settings",
    "MyCalls",
    "Map_SmallMarkers",
    "Map_Providers",
    "KST_Websocket",
    "KST_WebSocket_Timeout",
];

/// The top-level member names of an indented object, in text order.
fn top_level_keys(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("  \""))
        .map(|l| l.split('"').next().unwrap().to_string())
        .collect()
}

#[test]
fn settings_json_members_exact() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.gui.planes_filter_max_circumcircle_km = 250.9;
    let a = api(dir.path(), s);
    let text = get(&a, "/settings.json");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let o = v.as_object().unwrap();

    let mut keys: Vec<&str> = o.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want = SETTINGS_MEMBERS.to_vec();
    want.sort_unstable();
    assert_eq!(keys, want);
    assert_eq!(top_level_keys(&text), SETTINGS_MEMBERS);
    assert!(text.find("\"MyCall\"").unwrap() < text.find("\"Band\"").unwrap());

    assert_eq!(o["Band"], 1296);
    assert_eq!(o["MyCall"], "TE1ST");
    assert_eq!(o["DXCall"], "TE2ND");
    assert_eq!(o["MyCalls"], serde_json::json!(["TE1ST"]));
    assert_eq!(o["KST_WebSocket_Timeout"], 150.0);
    assert!(text.contains("\"KST_WebSocket_Timeout\": 150.0"));
    assert_eq!(o["KST_Websocket"], "ws://localhost:9881/AirScout");
    assert_eq!(o["Planes_Filter_Max_Circumcircle"], 250);
    assert!(text.contains("\"Planes_Filter_Max_Circumcircle\": 250,"));
    assert_eq!(o["Locator_MaxLength"], 10.0);
    assert!(text.contains("\"Locator_MaxLength\": 10.0,"));
    assert_eq!(o["Locator_AutoLength"], true);
    assert_eq!(o["Locator_SmallLettersForSubsquares"], false);
    assert_eq!(o["Planes_Filter_Min_Category"], 0);
    assert_eq!(o["Planes_Position_TTL"], 5);
    assert_eq!(o["Planes_MinAlt"], 5000);
    assert!(text.contains("\"MyLat\": 50.02,"));
    assert_eq!(o["Map_SmallMarkers"], true);
    assert_eq!(o["Server_Name"], "AS");
    assert_eq!(o["Planes_PlaneFeed2"], "");
    assert_eq!(o["Planes_PlaneFeed3"], "");
    assert_eq!(o["Planes_PlaneFeed1"], "OpenSky");

    let providers = o["Map_Providers"].as_array().unwrap();
    assert_eq!(providers.len(), 2);
    let names: Vec<_> = providers.iter().map(|p| p["Name"].clone()).collect();
    assert_eq!(names, ["OpenStreetMap", "OpenTopoMap"]);
    for p in providers {
        assert!(p["APIKey"].is_null());
        assert_eq!(p["MinZoom"], 1);
        assert_eq!(p["MaxZoom"], 19);
    }
    assert_eq!(
        providers[1]["Attribution"],
        "Map ©  <a href=\"https://www.opentopomap.org/\">OpenStreetMap</a>, Data © \
         <a href=\"https://www.openstreetmap.org/\">OpenStreetMap</a> contributors"
    );

    let rows = o["Path_Band_Settings"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0]["BAND"], "50M");
    let row = rows.iter().find(|r| r["BAND"] == "144M").unwrap();
    assert_eq!(row["MAXDISTANCE"], 10.0);
    assert_eq!(row["K-FACTOR"], 1.33);

    let w = o["Watchlist"].as_array().unwrap();
    assert_eq!(w.len(), 1);
    let item = &w[0];
    assert_eq!(item["Call"], "TE2ND");
    assert_eq!(item["Checked"], true);
    let wtext = &text[text.find("\"Watchlist\": [").unwrap()..];
    let order = ["Call", "Loc", "Checked", "Selected", "OutOfRange", "Remove"];
    let pos: Vec<usize> = order
        .iter()
        .map(|k| wtext.find(&format!("\"{k}\"")).unwrap())
        .collect();
    assert!(pos.windows(2).all(|p| p[0] < p[1]), "{pos:?}");
}

#[test]
fn map_provider_falls_back_to_a_served_one() {
    for (selected, served) in [
        ("OpenStreetMap", "OpenStreetMap"),
        ("OpenTopoMap", "OpenTopoMap"),
        ("OCM Landscape", "OpenStreetMap"),
        ("", "OpenStreetMap"),
    ] {
        let mut s = settings();
        s.gui.map_provider = selected.into();
        let v: serde_json::Value = serde_json::from_str(&settings_json(&s)).unwrap();
        assert_eq!(v["Map_Provider"], served, "{selected}");
    }
}

#[test]
fn settings_json_serves_all_three_feed_slots() {
    let mut s = settings();
    s.feed2 = "VRSWebServer".into();
    s.feed3 = "Readsb".into();
    let v: serde_json::Value = serde_json::from_str(&settings_json(&s)).unwrap();
    assert_eq!(v["Planes_PlaneFeed1"], "OpenSky");
    assert_eq!(v["Planes_PlaneFeed2"], "VRSWebServer");
    assert_eq!(v["Planes_PlaneFeed3"], "Readsb");
}

/// Every file below `dir` with its size and modification time.
fn listing(dir: &Path) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let m = e.metadata().unwrap();
            if m.is_dir() {
                todo.push(e.path());
            }
            out.insert(e.path(), (m.len(), m.modified().unwrap()));
        }
    }
    out
}

#[test]
fn settings_json_locator_small_letters() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.gui.locator_small_letters_for_subsquares = true;
    let a = api(dir.path(), s);
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/settings.json")).unwrap();
    assert_eq!(v["Locator_SmallLettersForSubsquares"], true);
}

#[test]
fn settings_json_writes_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    let before = listing(dir.path());
    assert!(!before.is_empty());
    get(&a, "/settings.json");
    assert_eq!(listing(dir.path()), before);
}

/// One request target with a valid query for every content route.
const ALL_ROUTES: [&str; 14] = [
    "/AircraftList.json",
    "/planes.json",
    "/version.json",
    "/settings.json",
    "/bands.json",
    "/bandvalues.json",
    "/aircraftcategories.json",
    "/location.json?call=TE1ST&loc=JO50AA",
    "/qrv.json?call=TE1ST&loc=JO50AA&band=1.2G",
    "/elevationpath.json?mycall=TE1ST&myloc=JO50AA&dxcall=TE2ND&dxloc=JO60AA",
    "/propagationpath.json?mycall=TE1ST&myloc=JO50AA&dxcall=TE2ND&dxloc=JO60AA&band=1.2G",
    "/nearestplanes.json?mycall=TE1ST&myloc=JO50AA&dxcall=TE2ND&dxloc=JO60AA&band=1.2G",
    "/airports.json",
    "/planefeedsettings.json",
];

#[test]
fn all_routes_cover_every_api_route() {
    let mut seen: Vec<ApiRoute> = Vec::new();
    for raw in ALL_ROUTES {
        let Route::Api(r) = route(raw) else {
            panic!("{raw}")
        };
        if !seen.contains(&r) {
            seen.push(r);
        }
    }
    assert_eq!(seen.len(), ALL_ROUTES.len());
}

#[test]
fn no_secret_in_any_route() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.gui.thunderforest_api_key = "SECRET-TEST-9".into();
    let mut with_secrets = 0;
    for feed in as_feeds::registry::all_feeds() {
        let mut t = toml::Table::new();
        for (n, key) in feed.secret_keys().iter().enumerate() {
            t.insert((*key).into(), format!("SECRET-TEST-{}", n + 1).into());
        }
        // the secrets reach the feed's own settings
        let mut loaded = as_feeds::registry::feed_by_name(feed.name()).unwrap();
        loaded.load_settings(&t).unwrap();
        if !feed.secret_keys().is_empty() {
            assert!(format!("{:?}", loaded.settings()).contains("SECRET-TEST"));
            with_secrets += 1;
        }
        s.feed_settings
            .insert(feed.name().into(), toml::Value::Table(t));
    }
    assert!(with_secrets >= 3);

    for feed in as_feeds::registry::all_feeds() {
        s.feed = feed.name().into();
        let a = api(dir.path(), s.clone());
        for raw in ALL_ROUTES {
            let body = get(&a, raw);
            assert!(
                !body.contains("SECRET-TEST"),
                "{} {raw}: {body}",
                feed.name()
            );
        }
    }
}

#[test]
fn vrs_feed_settings_shape() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "VRSWebServer".into();
    let mut t = toml::Table::new();
    t.insert("url".into(), "http://vrs.invalid/AircraftList.json".into());
    t.insert("password".into(), "SECRET-TEST-1".into());
    s.feed_settings
        .insert("VRSWebServer".into(), toml::Value::Table(t));
    let a = api(dir.path(), s.clone());
    assert_eq!(
        get(&a, "/planefeedsettings.json"),
        "[{\"Name\":\"VRSWebServer\",\"Value\":{\"URL\":\"http://vrs.invalid/AircraftList.json\",\
         \"URL2\":\"\",\"LoadShare\":true,\"Timeout\":30}}]"
    );
    // the page compares `Planes_PlaneFeed1` with this name (`dist/index.html:835-836`)
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/settings.json")).unwrap();
    assert_eq!(v["Planes_PlaneFeed1"], "VRSWebServer");
    assert_eq!(feed_web_name("VRSWebServer"), "VRSWebServer");
    assert_eq!(feed_web_name("OpenSky"), "OpenSky");
}

#[test]
fn vrs_without_table_uses_feed_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "VRSWebServer".into();
    let a = api(dir.path(), s);
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/planefeedsettings.json")).unwrap();
    let url = v[0]["Value"]["URL"].as_str().unwrap();
    assert!(!url.is_empty());
    assert_eq!(v[0]["Value"]["Timeout"], 30);
}

#[test]
fn vrs_bad_table_gives_empty_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "VRSWebServer".into();
    let mut t = toml::Table::new();
    t.insert("url".into(), toml::Value::Integer(5));
    s.feed_settings
        .insert("VRSWebServer".into(), toml::Value::Table(t));
    let a = api(dir.path(), s);
    assert_eq!(
        get(&a, "/planefeedsettings.json"),
        "[{\"Name\":\"VRSWebServer\",\"Value\":{}}]"
    );
}

#[test]
fn other_and_unknown_feed_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "OpenSky".into();
    let a = api(dir.path(), s.clone());
    assert_eq!(
        get(&a, "/planefeedsettings.json"),
        "[{\"Name\":\"OpenSky\",\"Value\":{}}]"
    );
    s.feed = "NoSuchFeed".into();
    let a = api(dir.path(), s);
    assert_eq!(get(&a, "/planefeedsettings.json"), "[]");
}

/// The text of the C# harness case `func` (`tests/golden/web_json.json`).
fn golden(func: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/web_json.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["fn"] == func)
        .unwrap()["result"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn bands_routes_match_csharp() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    assert_eq!(
        get(&a, "/bands.json"),
        "[\"50M\",\"70M\",\"144M\",\"432M\",\"1.2G\",\"2.3G\",\"3.4G\",\"5.7G\",\"10G\",\"24G\",\
         \"47G\",\"76G\"]"
    );
    // C1: `Name` is the band text, as the harness shows (not the survey's `B50M`)
    assert_eq!(get(&a, "/bandvalues.json"), golden("band_values"));
    assert!(get(&a, "/bandvalues.json").contains("{\"Name\":\"1.2G\",\"Value\":1296}"));
    assert_eq!(
        get(&a, "/aircraftcategories.json"),
        "[{\"Name\":\"None\",\"Value\":0},{\"Name\":\"Light\",\"Value\":1},\
         {\"Name\":\"Medium\",\"Value\":2},{\"Name\":\"Heavy\",\"Value\":3},\
         {\"Name\":\"Superheavy\",\"Value\":4}]"
    );
    assert_eq!(
        get(&a, "/aircraftcategories.json"),
        golden("aircraft_categories")
    );
}

#[test]
fn version_is_a_json_string() {
    let dir = tempfile::tempdir().unwrap();
    let a = api(dir.path(), settings());
    assert_eq!(
        get(&a, "/version.json"),
        format!("\"{}\"", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn airports_without_db_answers_error() {
    let dir = tempfile::tempdir().unwrap();
    // a directory where the database file belongs: the open fails
    std::fs::create_dir(dir.path().join("aircrafts.db3")).unwrap();
    let a = api(dir.path(), settings());
    let body = get(&a, "/airports.json");
    assert!(body.starts_with("Error: airports: "), "{body}");
}

#[test]
fn airports_from_db() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = api(dir.path(), settings());
    let db = AircraftDb::open_in_memory().unwrap();
    let json = "[{\"ICAO\":\"XXAA\",\"IATA\":\"XAA\",\"Lat\":50.5,\"Lon\":10.25,\"Alt\":300.0,\
                \"Airport\":\"Test Field\",\"Country\":\"Testland\",\
                \"LastUpdated\":\"2026-11-24T12:00:00Z\"}]";
    db.airport_bulk_insert_or_update_if_newer(&airports_from_json(json).unwrap())
        .unwrap();
    a.ctx.aircraft = Some(Arc::new(db));
    assert_eq!(
        get(&a, "/airports.json"),
        "[{\"ICAO\":\"XXAA\",\"IATA\":\"XAA\",\"Lat\":50.5,\"Lon\":10.25,\"Alt\":300.0,\
         \"Airport\":\"Test Field\",\"Country\":\"Testland\",\
         \"LastUpdated\":\"2026-11-24T12:00:00Z\"}]"
    );
}

/// A URL with credentials as userinfo.
const URL_WITH_USERINFO: &str = "http://SECRET-TEST-U:SECRET-TEST-P@example.invalid/x";

#[test]
fn userinfo_never_served() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "VRSWebServer".into();
    let mut t = toml::Table::new();
    t.insert("url".into(), URL_WITH_USERINFO.into());
    t.insert("url2".into(), URL_WITH_USERINFO.into());
    s.feed_settings
        .insert("VRSWebServer".into(), toml::Value::Table(t));
    s.web.kst_websocket = URL_WITH_USERINFO.into();
    let a = api(dir.path(), s);
    for raw in ALL_ROUTES {
        let body = get(&a, raw);
        assert!(!body.contains("SECRET-TEST"), "{raw}: {body}");
    }
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/planefeedsettings.json")).unwrap();
    assert_eq!(v[0]["Value"]["URL"], "http://example.invalid/x");
    assert_eq!(v[0]["Value"]["URL2"], "http://example.invalid/x");
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/settings.json")).unwrap();
    assert_eq!(v["KST_Websocket"], "http://example.invalid/x");
}

/// A query credential never reaches the LAN, while the page's placeholders survive (`#257`, `#277`).
#[test]
fn query_credentials_never_served() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.feed = "VRSWebServer".into();
    let mut t = toml::Table::new();
    t.insert(
        "url".into(),
        "http://localhost:7780/AircraftList.json?fSBnd=%MINLAT%&key=SECRET-TEST".into(),
    );
    t.insert(
        "url2".into(),
        "http://localhost:7780/AircraftList.json?key=SECRET-TEST".into(),
    );
    s.feed_settings
        .insert("VRSWebServer".into(), toml::Value::Table(t));
    s.web.kst_websocket = "ws://localhost:9881/AirScout?token=SECRET-TEST".into();
    let a = api(dir.path(), s);
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/planefeedsettings.json")).unwrap();
    assert_eq!(
        v[0]["Value"]["URL"],
        "http://localhost:7780/AircraftList.json?fSBnd=%MINLAT%"
    );
    assert_eq!(
        v[0]["Value"]["URL2"],
        "http://localhost:7780/AircraftList.json"
    );
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/settings.json")).unwrap();
    assert_eq!(v["KST_Websocket"], "ws://localhost:9881/AirScout");
}

#[test]
fn strip_userinfo_cases() {
    for (raw, served) in [
        // http, https, ws and wss: the `url` crate's canonical form without userinfo
        ("http://u:p@host/x?a=b@c", "http://host/x?a=b@c"),
        ("http://user@host:81/x", "http://host:81/x"),
        (
            "ws://u:p@w@localhost:9881/AirScout",
            "ws://localhost:9881/AirScout",
        ),
        ("http://u:p@host", "http://host/"),
        (
            "http:///SECRET-TEST-U:SECRET-TEST-P@host/x",
            "http://host/x",
        ),
        (
            "http:\\\\SECRET-TEST-U:SECRET-TEST-P@host/x",
            "http://host/x",
        ),
        ("http://\\SECRET-TEST-U:SECRET-TEST-P@host", "http://host/"),
        ("http:SECRET-TEST-U:SECRET-TEST-P@host/x", "http://host/x"),
        ("WSS:SECRET-TEST-U:SECRET-TEST-P@host", "wss://host/"),
        (
            "http://SECRET-TEST-U:SECRET-TEST-P@host?a@b",
            "http://host/?a@b",
        ),
        (" http://SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("\thttp://SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("\x00http://SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("http://SECRET-TEST-U:SECRET-TEST-P@h/x \x1f", "http://h/x"),
        ("ht\ttp://SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("http:/\n/SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("http:/\\\t/SECRET-TEST-U:SECRET-TEST-P@h", "http://h/"),
        ("http://SECRET-TEST-U:SECRET-\rTEST-P@h", "http://h/"),
        // without credentials: byte-identical
        ("HTTP://Host/X/?q=1", "HTTP://Host/X/?q=1"),
        ("http://host/path@x", "http://host/path@x"),
        ("http://host/path@x?y=a@b", "http://host/path@x?y=a@b"),
        (
            "ws://localhost:9881/AirScout",
            "ws://localhost:9881/AirScout",
        ),
        ("http://[::1]:80/x", "http://[::1]:80/x"),
        ("http://host?x=a@b", "http://host?x=a@b"),
        ("http://host#f@x", "http://host#f@x"),
        ("http://host\\p@x", "http://host\\p@x"),
        ("", ""),
        ("not a url", "not a url"),
        // another scheme, or no URL: only the text after the last `@`
        ("u:p@host/x", "host/x"),
        ("host/a://b@c", "c"),
        ("//SECRET-TEST-U:SECRET-TEST-P@host/x", "host/x"),
        ("\\\\SECRET-TEST-U:SECRET-TEST-P@host", "host"),
        ("foo://SECRET-TEST-U:SECRET-TEST-P@h\\x@y", "y"),
        ("http://SECRET-TEST-U:SECRET-TEST-P@", ""),
    ] {
        let out = strip_userinfo(raw);
        assert_eq!(out, served, "{raw:?}");
        assert!(!out.contains("SECRET-TEST"), "{raw:?}");
    }
}

#[test]
fn scrub_public_url_cases() {
    for (raw, served) in [
        // no query: userinfo stripped, everything else byte-identical
        ("http://u:p@host/x", "http://host/x"),
        (
            "ws://localhost:9881/AirScout",
            "ws://localhost:9881/AirScout",
        ),
        // placeholder-only query survives (the page substitutes these, #257)
        (
            "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%\
             &fWBnd=%MINLON%&fEBnd=%MAXLON%",
            "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%\
             &fWBnd=%MINLON%&fEBnd=%MAXLON%",
        ),
        // a literal query token (credential or not) is dropped, the placeholder is kept
        (
            "http://localhost/a?lat=%MINLAT%&api_key=SECRET-TEST",
            "http://localhost/a?lat=%MINLAT%",
        ),
        ("http://localhost/a?key=SECRET-TEST", "http://localhost/a"),
        ("http://localhost/a?age=300", "http://localhost/a"),
        (
            "http://localhost/a?bounds=%MINLAT%,%MAXLAT%",
            "http://localhost/a?bounds=%MINLAT%,%MAXLAT%",
        ),
        // a query mixed into the value is dropped (not placeholder-only)
        (
            "http://localhost/a?lat=%MINLAT%SECRET-TEST",
            "http://localhost/a",
        ),
        // the fragment is dropped; a `?` inside the fragment is not a query
        ("http://localhost/a?x=1#frag", "http://localhost/a"),
        ("http://localhost/a#f?x=1", "http://localhost/a"),
        // the KST websocket drops its query too (#277)
        (
            "ws://localhost:9881/AirScout?token=SECRET-TEST",
            "ws://localhost:9881/AirScout",
        ),
    ] {
        let out = scrub_public_url(raw);
        assert_eq!(out, served, "{raw:?}");
        assert!(!out.contains("SECRET-TEST"), "{raw:?}");
    }
    // a path-embedded token still reaches the LAN (owner decision B: warned, not stripped)
    assert_eq!(
        scrub_public_url("http://localhost/SECRET-TEST/x?lat=%MINLAT%"),
        "http://localhost/SECRET-TEST/x?lat=%MINLAT%"
    );
}

/// A malformed, non-HTTP/WS or scheme-less value is not served at all, so a query credential
/// behind an `@` can never reach the LAN; a well-formed URL drops its query credential and its
/// userinfo (`#257`, `#277`).
#[test]
fn scrub_public_url_drops_query_before_userinfo() {
    for (raw, served) in [
        // a space in the host makes the parse fail: the value is not served
        ("http://bad host/AircraftList.json?email=u@SECRET-TEST", ""),
        // a non-http/ws scheme: the value is not served
        ("custom://host/a?user=a@SECRET-TEST", ""),
        // no scheme: the value is not served
        ("//host/a?email=a@SECRET-TEST", ""),
        // userinfo plus a query token: both are stripped
        ("http://u:p@host/x?token=SECRET-TEST", "http://host/x"),
        ("http://u:p@host?token=SECRET-TEST", "http://host/"),
        // an `@` outside the userinfo is ambiguous (a password may have ended the authority):
        // the value is not served
        ("http://host/x?a=u@SECRET-TEST", ""),
        // ws/wss with userinfo and a query token
        ("ws://u:p@host/x?token=SECRET-TEST", "ws://host/x"),
        ("wss://u:p@host/x?token=SECRET-TEST", "wss://host/x"),
        // IPv6 host
        ("http://[::1]/x?token=SECRET-TEST", "http://[::1]/x"),
    ] {
        let out = scrub_public_url(raw);
        assert_eq!(out, served, "{raw:?}");
        assert!(!out.contains("SECRET-TEST"), "{raw:?}");
    }
}

/// A separator-only or empty parameter value never counts as a placeholder, so the parameter
/// (name and value) is dropped entirely (`#257`).
#[test]
fn scrub_public_url_separator_only_value_is_dropped() {
    for raw in [
        "http://localhost:7780/AircraftList.json?SECRET-TEST=,",
        "http://localhost:7780/AircraftList.json?SECRET-TEST= ",
        "http://localhost:7780/AircraftList.json?SECRET-TEST=\t",
    ] {
        let out = scrub_public_url(raw);
        assert_eq!(out, "http://localhost:7780/AircraftList.json", "{raw:?}");
        assert!(!out.contains("SECRET-TEST"), "{raw:?}");
    }
}

/// The page substitutes placeholders case-insensitively (`scoutbase.core.js` upper-cases), so a
/// lower-case `%minlat%` is still a placeholder and its parameter survives (`#257`).
#[test]
fn scrub_public_url_lower_case_placeholders_kept() {
    let out = scrub_public_url("http://host/feed?lat=%minlat%&lon=%minlon%&key=SECRET-TEST");
    assert_eq!(out, "http://host/feed?lat=%minlat%&lon=%minlon%");
}

/// `KST_Websocket` drops its entire query — even the page placeholders, which `airscout.wtkst.js`
/// does not substitute (`#277`).
#[test]
fn kst_websocket_drops_its_entire_query() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings();
    s.web.kst_websocket = "ws://localhost:9881/AirScout?lat=%MINLAT%&token=SECRET-TEST".into();
    let a = api(dir.path(), s);
    let v: serde_json::Value = serde_json::from_str(&get(&a, "/settings.json")).unwrap();
    assert_eq!(v["KST_Websocket"], "ws://localhost:9881/AirScout");
}

/// The exact served string for a placeholder query: the `url` crate must serialize `%MINLAT%`
/// literally (not `%25MINLAT%25`) so `airscout.feeds.js` can substitute it (`#257`).
#[test]
fn scrub_public_url_serves_the_placeholder_literally() {
    assert_eq!(
        scrub_public_url(
            "http://localhost:7780/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%\
             &fWBnd=%MINLON%&fEBnd=%MAXLON%"
        ),
        "http://localhost:7780/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%\
         &fWBnd=%MINLON%&fEBnd=%MAXLON%"
    );
    assert_eq!(
        scrub_public_url("http://localhost/a?bounds=%MINLAT%,%MAXLAT%"),
        "http://localhost/a?bounds=%MINLAT%,%MAXLAT%"
    );
}

/// A value whose credential boundary is ambiguous — a `?`/`#` inside a password, more than one
/// `@`, no scheme, or a non-http/ws scheme — is not served (`#257`, `#277`).
#[test]
fn scrub_public_url_fails_closed_on_ambiguous_credentials() {
    for raw in [
        // a `?` inside the password: the `@` is no longer the userinfo separator, and the
        // intended userinfo would parse as a host/port — not served
        "http://user:p?w@host/AircraftList.json",
        "ws://user:p?w@host/AirScout",
        // a numeric password makes the truncated prefix parse as `host:port` — not served
        "http://user:1234?pw@host/AircraftList.json",
        "ws://user:1234?pw@host/AirScout",
        "http://user:1234#pw@host/AircraftList.json?token=SECRET-TEST",
        "ws://user:1234#pw@host/AirScout?token=SECRET-TEST",
        // a `#` inside the password (or before the `@`): same
        "http://user:p#w@host/AircraftList.json?token=SECRET-TEST",
        "ws://user:p#w@host/AirScout?token=SECRET-TEST",
        // a default, empty or no port: the truncated prefix is a bare host — still not served
        "http://SECRET-USER:80/SECRET-PASS@host/x",
        "https://SECRET-USER:443?SECRET-PASS@host/x",
        "http://SECRET-USER:/SECRET-PASS@host/x",
        "http://SECRET-USER:?SECRET-PASS@host/x",
        "ws://SECRET-USER/SECRET-PASS@host/x",
        "http://SECRET-USER:p\\w@host/x",
        // an `@` anywhere outside the userinfo is ambiguous: not served
        "http://host:8080/x?a=b@c",
        // more than one `@`: a password could hold one
        "http://u:p@w@localhost:9881/AirScout",
        "http://user@host#f@x",
        // another scheme, or no scheme
        "ftp://user:pass@host/x",
        "//SECRET-TEST-U:SECRET-TEST-P@host/x",
    ] {
        assert_eq!(scrub_public_url(raw), "", "{raw:?}");
    }
}

/// A percent-encoded `@` in the userinfo is cleared; an IPv6 host is served unchanged (`#257`).
#[test]
fn scrub_public_url_clears_percent_encoded_userinfo_and_keeps_ipv6() {
    let out = scrub_public_url("http://SECRET%40TEST:pass@example.invalid/x");
    assert_eq!(out, "http://example.invalid/x");
    assert!(!out.contains("SECRET"));
    assert_eq!(
        scrub_public_url("http://u:p@[::1]:8080/x"),
        "http://[::1]:8080/x"
    );
}

/// Inputs that leaked credentials through earlier hand parsers (re-reviews 2 and 3).
#[test]
fn strip_userinfo_earlier_leaks() {
    const UI: &str = "SECRET-TEST-U:SECRET-TEST-P@";
    let forms = [
        // re-review 2
        " http://{UI}h",
        "\thttp://{UI}h",
        "\x01http://{UI}h",
        "\x00http://{UI}h",
        "\x0bhttp://{UI}h",
        "ht\ttp://{UI}h",
        "h\nttp://{UI}h",
        "http:/\t/{UI}h",
        "http:/\n/{UI}h",
        "http:\n//{UI}h",
        "http:/\\\t/{UI}h",
        "foo://h\\{UI}i",
        // re-review 3
        "x@ http://{UI}h",
        "@ http://{UI}h",
        "@\x0bhttp://{UI}h",
        "foo:x@ http://{UI}h",
        "foo:x@\x00http://{UI}h",
        "mailto:a@\x01https://{UI}h/x",
        "a:b@ ws://{UI}h:9881/AirScout",
        "a@ wss://{UI}h",
        "a@ ftp://{UI}h",
        "a@ foo://{UI}h",
        "a@\x0chttp:\\\\{UI}h",
        "foo:x@ \x00\x1fhttp://{UI}h",
        "foo:x@\t http://{UI}h",
        "ä@ http://{UI}h",
        "é:ö@ http://{UI}h",
        "a@ //{UI}evil.invalid/x",
        "foo:a@ //{UI}evil.invalid/x",
        "foo://\\{UI}h",
        "HTTP:\\/a@{UI}\\q@r:",
    ];
    let base = url::Url::parse("http://example.invalid/dir/page").unwrap();
    for form in forms {
        let raw = form.replace("{UI}", UI);
        let out = strip_userinfo(&raw);
        assert!(!out.contains("SECRET-TEST"), "{raw:?} -> {out:?}");
        if let Ok(u) = base.join(&out) {
            assert_eq!(u.username(), "", "{raw:?} -> {out:?}");
            assert_eq!(u.password(), None, "{raw:?} -> {out:?}");
        }
    }
}

#[test]
fn strip_userinfo_is_linear() {
    let start = std::time::Instant::now();
    for raw in [
        format!("http://{}host", "@/".repeat(500_000)),
        "@/".repeat(500_000),
        format!(
            "http://SECRET-TEST-U:SECRET-TEST-P@h/{}",
            "a".repeat(1_000_000)
        ),
    ] {
        assert!(!strip_userinfo(&raw).contains("SECRET-TEST"));
    }
    let took = start.elapsed();
    assert!(took < std::time::Duration::from_secs(1), "{took:?}");
}

/// A deterministic xorshift PRNG (no extra crate).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }

    /// `n` random pieces of `xs`, `0..=max` of them.
    fn some(&mut self, xs: &[&str], max: u64) -> String {
        let n = self.next() % (max + 1);
        (0..n).map(|_| self.pick(xs)).collect()
    }
}

/// `s` with tab, LF and CR put in at random places.
fn sprinkle(rng: &mut Rng, s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if rng.next().is_multiple_of(8) {
            out.push_str(rng.pick(&["\t", "\n", "\r"]));
        }
        out.push(c);
    }
    out
}

#[test]
fn strip_userinfo_leaves_no_userinfo_for_the_url_parser() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut parsed = 0;
    for _ in 0..20_000 {
        let lead = rng.some(&[" ", "\x00", "\x01", "\x1f", "\t", "\n"], 2);
        let scheme = rng.pick(&["http", "https", "ws", "wss", "foo", "HTTP", ""]);
        let colon = if scheme.is_empty() { "" } else { ":" };
        let slashes = rng.some(&["/", "\\"], 4);
        let before = rng.some(&["a", ":", "@", "%40", "b@"], 3);
        let host = rng.pick(&["h", "example.invalid", "[::1]", "h:81", ""]);
        let tail = rng.some(&["/", "\\", "?", "#", "@", ":", "x", "q@r", "\t"], 5);
        let trail = rng.some(&[" ", "\x1f", "\x00"], 2);
        let head = sprinkle(
            &mut rng,
            &format!("{scheme}{colon}{slashes}{before}SECRET-TEST-U:SECRET-TEST-P@"),
        );
        let raw = format!("{lead}{head}{host}{tail}{trail}");
        let out = strip_userinfo(&raw);
        assert!(!out.contains("SECRET-TEST-P"), "{raw:?} -> {out:?}");
        assert!(!out.contains("SECRET-TEST-U"), "{raw:?} -> {out:?}");
        if let Ok(u) = url::Url::parse(&out) {
            parsed += 1;
            assert_eq!(u.username(), "", "{raw:?} -> {out:?}");
            assert_eq!(u.password(), None, "{raw:?} -> {out:?}");
        }
    }
    assert!(parsed > 5_000, "{parsed}");
}
