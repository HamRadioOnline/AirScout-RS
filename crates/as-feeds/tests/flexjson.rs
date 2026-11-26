//! FlexJSON feed: column auto-detection on synthetic shapes
//! (`tests/fixtures/feeds/flexjson_*.json`).
//!
//! The expected indices are derived by hand from `FlexJSON.cs`:
//! - `GetAllPlaneInfos` flattens every object/array, takes the most common container length
//!   `n` and turns each container of length `n` into a row `[key, v1, .., vn]`. Column 0 is
//!   the container's key: a fresh GUID for array elements, the member name otherwise.
//! - The finders run in the order Hex, UTC, Type, Lat, Lon, Alt, Track, Speed, Reg. Each
//!   takes the first not yet assigned column with the most matching cells and keeps it only
//!   if that count is `> (int)(0.9 * rows)`. Alt also needs a column mean `> 5000`, Track a
//!   column maximum `<= 360`, Speed a maximum `> 360`.
//! - `IsReg` is true for any text containing `-`, so the GUID column 0 wins the Reg search
//!   whenever it is still free. `GetPlanes` reads a column only if its index is `> 0`.
use as_feeds::registry::feed_by_name;
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{Duration, Utc};
use mockito::Server;

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

/// A FlexJSON feed that fetches `body` from a mock server, plus `extra` settings.
fn run(body: &str, extra: &str) -> (Box<dyn PlaneFeed>, Result<Vec<FeedPlane>, FeedError>) {
    let mut server = Server::new();
    let m = server.mock("GET", "/feed.json").with_body(body).create();
    let mut f = feed_by_name("FlexJSON").expect("FlexJSON registered");
    let s = format!("url = \"{}/feed.json\"\n{extra}", server.url());
    f.load_settings(&s.parse().unwrap()).unwrap();
    f.start(&args()).unwrap();
    let r = f.get_planes(&args());
    m.assert();
    (f, r)
}

/// The persisted indices `[hex, utc, type, lat, lon, alt, track, speed, reg]`.
fn indices(f: &dyn PlaneFeed) -> [i64; 9] {
    let t = f.settings();
    [
        "index_hex",
        "index_utc",
        "index_type",
        "index_lat",
        "index_lon",
        "index_alt",
        "index_track",
        "index_speed",
        "index_reg",
    ]
    .map(|k| t[k].as_integer().unwrap_or_else(|| panic!("{k}")))
}

#[test]
fn detects_columns_array_of_arrays() {
    // rows: [guid, hex, call, lat, lon, alt, track, speed, type, reg, vspeed]
    // Hex 1; no UTC; Type 8; Lat 3 (Lon 4 matches IsLat as well, 3 comes first); Lon 4;
    // Alt 5 (mean > 5000); Track 6 (max <= 360); Speed 7 (max > 360); Reg 0: the GUID column
    // matches IsReg in every row and comes before column 9.
    let before = Utc::now();
    let (f, r) = run(&fixture("flexjson_array_of_arrays.json"), "");
    let planes = r.unwrap();
    assert_eq!(indices(f.as_ref()), [1, -1, 8, 3, 4, 5, 6, 7, 0]);
    assert_eq!(planes.len(), 12);
    let p = &planes[0];
    assert_eq!(p.hex, "3C6589");
    assert_eq!(p.lat, 50.1234);
    assert_eq!(p.lon, 10.5678);
    assert_eq!(p.alt_ft, 37000.0);
    assert_eq!(p.track, 245.0);
    assert_eq!(p.speed_kts, 450.0);
    assert_eq!(p.type_code, "A320");
    // the call column is never detected, the reg column lost to column 0
    assert_eq!(p.call, "");
    assert_eq!(p.reg, "");
    // no UTC column: DateTime.UtcNow
    let t = p.time.unwrap();
    assert!(t >= before && t <= Utc::now());
}

#[test]
fn detects_columns_vrs_like() {
    // rows: [guid, Id, Icao, Reg, Alt, Lat, Long, Spd, Trak, Type, Call]
    // The dictionary branch of GetPlaneInfo keeps only primitives and strings: the decimal
    // Lat/Long values become "". Hex 2, Type 9, Alt 4 (Id has mean > 5000 but no cell
    // <= 100000), Track 8, Speed 7, Reg 0; Lat and Lon stay -1 and GetPlanes throws.
    let (f, r) = run(&fixture("flexjson_vrs_like.json"), "");
    assert!(
        matches!(r, Err(FeedError::Parse(ref m)) if m.contains("Lat")),
        "{r:?}"
    );
    // the indices found so far are kept (Settings.Save before the throw)
    assert_eq!(indices(f.as_ref()), [2, -1, 9, -1, -1, 4, 8, 7, 0]);
}

#[test]
fn detects_columns_nested() {
    // rows: [hex key, lat, lon, alt, track, speed, type, reg, call]
    // Containers: root (2), meta (2), data (1), aircraft (12), 12 arrays of 8 -> n = 8.
    // Hex 0 (the member name), Type 6, Lat 1, Lon 2, Alt 3, Track 4, Speed 5, Reg 7
    // (column 0 is already taken by Hex).
    let (f, r) = run(&fixture("flexjson_nested.json"), "");
    let planes = r.unwrap();
    assert_eq!(indices(f.as_ref()), [0, -1, 6, 1, 2, 3, 4, 5, 7]);
    assert_eq!(planes.len(), 12);
    let p = &planes[3];
    assert_eq!(p.lat, 50.4567);
    assert_eq!(p.reg, "G-EUPT");
    assert_eq!(p.type_code, "A21N");
}

#[test]
fn column_zero_never_mapped() {
    // Hex is detected in column 0 (the member names), but GetPlanes reads a column only if
    // its index is > 0: every hex is "".
    let (_, r) = run(&fixture("flexjson_nested.json"), "");
    assert!(r.unwrap().iter().all(|p| p.hex.is_empty()));
    // the same with a hand-set index 0
    let (_, r) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "index_hex = 0\nindex_call = 2",
    );
    let planes = r.unwrap();
    assert!(planes.iter().all(|p| p.hex.is_empty()));
    assert_eq!(planes[1].call, "KLM1023");
}

#[test]
fn manual_indices_are_kept() {
    // set indices are not searched again and their columns are excluded from the search
    let (f, r) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "index_reg = 9\nindex_alt = 5",
    );
    r.unwrap();
    assert_eq!(indices(f.as_ref()), [1, -1, 8, 3, 4, 5, 6, 7, 9]);
}

#[test]
fn metre_and_kmh_units() {
    // "m" and "km/h" convert in To_Alt/To_Speed; the detection compares with "m " and
    // "km/h " (trailing space) and so ignores the units
    let (f, r) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "units_alt = \"m\"\nunits_speed = \"km/h\"",
    );
    let planes = r.unwrap();
    assert_eq!(indices(f.as_ref()), [1, -1, 8, 3, 4, 5, 6, 7, 0]);
    assert_eq!(planes[0].alt_ft, f64::from(i32::MIN)); // 37000 m -> 121391 ft > 100000
    assert_eq!(planes[2].alt_ft, f64::from((24000.0 * 3.28084) as i32));
    assert_eq!(planes[0].speed_kts, f64::from((450.0 / 1.852) as i32));
}

#[test]
fn utc_ms_column_detected() {
    // IsUTC accepts only 13-digit millisecond time stamps within 1000 s of now
    let now = Utc::now();
    let json = |stamp: &dyn Fn(i64) -> i64| {
        let rows: Vec<String> = (0..12)
            .map(|i| {
                format!(
                    "[\"3C65{i:02}\", {}, 50.{i}5, 10.{i}5, 3{i:02}00, {}, {}]",
                    stamp(i),
                    10 * i,
                    400 + i
                )
            })
            .collect();
        format!("[{}]", rows.join(","))
    };
    let ms = |i| (now - Duration::seconds(i)).timestamp_millis();
    let (f, r) = run(&json(&ms), "");
    let planes = r.unwrap();
    assert_eq!(indices(f.as_ref())[1], 2);
    assert_eq!(planes[3].time.unwrap().timestamp_millis(), ms(3));
    // 10-digit seconds fail the millisecond check that follows the seconds check
    let s = |i| (now - Duration::seconds(i)).timestamp();
    let (f, _) = run(&json(&s), "");
    assert_eq!(indices(f.as_ref())[1], -1);
}

#[test]
fn decimal_altitude_is_unset() {
    // To_Alt/To_Track/To_Speed parse integers only
    let json = fixture("flexjson_array_of_arrays.json").replace("37000", "37000.5");
    let (_, r) = run(&json, "index_alt = 5");
    assert_eq!(r.unwrap()[0].alt_ft, f64::from(i32::MIN));
}

#[test]
fn json_null_is_err() {
    // AddElement calls GetType() on every value: a null anywhere throws
    let json = fixture("flexjson_array_of_arrays.json").replacen("\"A320\"", "null", 1);
    let (_, r) = run(&json, "");
    assert!(matches!(r, Err(FeedError::Parse(_))), "{r:?}");
}

#[test]
fn min_planes_not_enforced() {
    // Min_Planes / Min_Elements are settings only; C# never reads them
    let (_, r) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "min_planes = 50\nmin_elements = 20",
    );
    assert_eq!(r.unwrap().len(), 12);
}

#[test]
fn empty_url_is_config_err() {
    let mut f = feed_by_name("FlexJSON").unwrap();
    f.start(&args()).unwrap();
    assert!(matches!(f.get_planes(&args()), Err(FeedError::Config(_))));
}

#[test]
fn flexjson_defaults() {
    let f = feed_by_name("FlexJSON").unwrap();
    // on by default (stage-2 Global Constraints); the URL is empty until the user sets one
    assert!(f.enabled_by_default());
    assert!(f.disclaimer().is_some());
    let t = f.settings();
    assert_eq!(t["url"].as_str(), Some(""));
    assert_eq!(t["timeout_s"].as_integer(), Some(30));
    assert_eq!(t["units_alt"].as_str(), Some("ft"));
    assert_eq!(t["units_speed"].as_str(), Some("kts"));
    assert_eq!(t["min_elements"].as_integer(), Some(10));
    assert_eq!(t["min_planes"].as_integer(), Some(50));
    assert_eq!(t["index_call"].as_integer(), Some(-1));
    assert_eq!(indices(f.as_ref()), [-1; 9]);
}
