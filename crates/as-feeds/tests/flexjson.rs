//! FlexJSON fixture and synthetic tests for explicit columns and checked inference.
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
fn arrays_map_when_ambiguous_coordinates_are_configured() {
    let before = Utc::now();
    let (feed, result) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "index_lat = 3\nindex_lon = 4",
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 12);
    assert_eq!(indices(feed.as_ref()), [-1, -1, -1, 3, 4, -1, -1, -1, -1]);
    let plane = &planes[0];
    assert_eq!(plane.hex, "3C6589");
    assert_eq!((plane.lat, plane.lon), (50.1234, 10.5678));
    assert_eq!(
        (plane.alt_ft, plane.track, plane.speed_kts),
        (37000.0, 245.0, 450.0)
    );
    assert_eq!(plane.type_code, "A320");
    assert!(!plane.reg.is_empty());
    assert!(plane.time.unwrap() >= before && plane.time.unwrap() <= Utc::now());
}

#[test]
fn positional_aliases_extend_the_named_vrs_like_layout() {
    let (_, result) = run(
        &fixture("flexjson_vrs_like.json"),
        "index_lon = 6\nindex_speed = 7\nindex_track = 8",
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 12);
    assert_eq!(planes[0].hex, "3C6589");
    assert_eq!(planes[0].lat, 50.1234);
}

#[test]
fn nested_arrays_infer_the_actual_object_key() {
    let (feed, result) = run(
        &fixture("flexjson_nested.json"),
        "index_lat = 1\nindex_lon = 2",
    );
    let planes = result.unwrap();
    assert_eq!(indices(feed.as_ref()), [-1, -1, -1, 1, 2, -1, -1, -1, -1]);
    assert_eq!(planes.len(), 12);
    assert!(!planes[3].hex.is_empty());
    assert_eq!(planes[3].lat, 50.4567);
    assert_eq!(planes[3].reg, "G-EUPT");
    assert_eq!(planes[3].type_code, "A21N");
}

#[test]
fn manual_indices_are_kept() {
    let (feed, result) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "index_reg = 9\nindex_alt = 5\nindex_lat = 3\nindex_lon = 4",
    );
    assert_eq!(result.unwrap().len(), 12);
    assert_eq!(indices(feed.as_ref()), [-1, -1, -1, 3, 4, 5, -1, -1, 9]);
}

#[test]
fn both_epoch_units_are_inferred_within_the_detection_window() {
    let now = Utc::now();
    for millis in [false, true] {
        let stamp = |i| {
            let time = now - Duration::seconds(i);
            if millis {
                time.timestamp_millis()
            } else {
                time.timestamp()
            }
        };
        let rows: Vec<String> = (0..12)
            .map(|i| {
                format!(
                    r#"["3C65{i:02}", {}, 50.25, 10.25, 30000, 80, 430]"#,
                    stamp(i)
                )
            })
            .collect();
        let (feed, result) = run(
            &format!("[{}]", rows.join(",")),
            "index_lat = 3\nindex_lon = 4",
        );
        let planes = result.unwrap();
        assert_eq!(indices(feed.as_ref())[1], -1);
        let at = planes[3].time.unwrap();
        assert_eq!(
            if millis {
                at.timestamp_millis()
            } else {
                at.timestamp()
            },
            stamp(3)
        );
    }
}

#[test]
fn min_planes_not_enforced() {
    let (_, result) = run(
        &fixture("flexjson_array_of_arrays.json"),
        "min_planes = 50\nmin_elements = 20\nindex_lat = 3\nindex_lon = 4",
    );
    assert_eq!(result.unwrap().len(), 12);
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

const EXPLICIT_COLUMNS: &str =
    "index_hex = 1\nindex_lat = 2\nindex_lon = 3\nindex_alt = 4\nindex_track = 5\nindex_speed = 6\n";

#[test]
fn finding_70_maps_real_column_zero_and_ignores_array_placeholder() {
    let settings = "index_hex = 0\nindex_lat = 1\nindex_lon = 2\nindex_alt = 3\nindex_track = 4\nindex_speed = 5";
    let (_, keyed) = run(r#"{"3C0001":[50.25,10.25,12000,80,430]}"#, settings);
    assert_eq!(keyed.unwrap()[0].hex, "3C0001");
    let (_, array) = run(
        r#"[["3C0001",50.25,10.25,12000,80,430,"G-TEST"]]"#,
        EXPLICIT_COLUMNS,
    );
    assert_eq!(array.unwrap()[0].reg, "G-TEST");
}

#[test]
fn finding_71_uses_common_metric_ranges_and_conversion() {
    let settings = format!("{EXPLICIT_COLUMNS}units_alt = \" M \"\nunits_speed = \" KM/H \"");
    let (_, result) = run(
        r#"[
        ["3C0001",50.25,10.25,10000,80,185.2],
        ["3C0002",50.25,10.25,10000,80,1001],
        ["3C0003",50.25,10.25,30001,80,185.2],
        ["3C0004",50.25,10.25,10000,80,-0.1]
    ]"#,
        &settings,
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 1);
    assert!((planes[0].speed_kts - 100.0).abs() < 1e-9);
    assert!((planes[0].alt_ft - 32808.4).abs() < 1e-9);
}

#[test]
fn finding_72_keeps_fractional_motion_and_altitude() {
    let (_, result) = run(
        r#"[["3C0001",50.25,10.25,37000.5,245.3,100.5]]"#,
        EXPLICIT_COLUMNS,
    );
    let planes = result.unwrap();
    assert_eq!(
        (planes[0].alt_ft, planes[0].track, planes[0].speed_kts),
        (37000.5, 245.3, 100.5)
    );
}

#[test]
fn finding_73_named_objects_keep_fractional_fields_in_any_order() {
    let (_, result) = run(
        r#"[
        {"ICAO":"3C0001","latitude":50.25,"longitude":10.25,"altitude":12000.5,"groundtrack":60.5,"groundspeed":100},
        {"groundspeed":100,"groundtrack":65.5,"altitude":13000.7,"longitude":11.25,"latitude":51.25,"ICAO":"3C0002"}
    ]"#,
        "",
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 2);
    assert_eq!((planes[0].lat, planes[0].lon), (50.25, 10.25));
    assert_eq!((planes[1].lat, planes[1].lon), (51.25, 11.25));
    assert_eq!(planes[1].speed_kts, 100.0);
}

#[test]
fn finding_74_null_cells_and_rows_keep_neighboring_positions() {
    let (_, result) = run(
        r#"{"meta":null,"data":[null,
        ["3C0001",50.25,10.25,12000,80,430,null,"G-TEST"]
    ]}"#,
        &format!("{EXPLICIT_COLUMNS}index_call = 7\nindex_reg = 8"),
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 1);
    assert!(planes[0].call.is_empty());
    assert_eq!(planes[0].reg, "G-TEST");
    assert!(run("null", "").1.is_err());
}

#[test]
fn finding_75_checks_epochs_offsets_and_missing_values_separately() {
    let (_, result) = run(
        r#"[
        ["3C0001",50.25,10.25,12000,80,430,1795435200],
        ["3C0002",50.25,10.25,12000,80,430,"001795435200000"],
        ["3C0003",50.25,10.25,12000,80,430,"2026-10-25T02:30:00+02:00"],
        ["3C0004",50.25,10.25,12000,80,430,"2026-10-25T02:30:00+01:00"],
        ["3C0005",50.25,10.25,12000,80,430,946684800],
        ["3C0006",50.25,10.25,12000,80,430,1.7954352e9],
        ["3C0007",50.25,10.25,12000,80,430,null],
        ["3C0008",50.25,10.25,12000,80,430,10000000000]
    ]"#,
        &format!("{EXPLICIT_COLUMNS}index_utc = 7"),
    );
    let planes = result.unwrap();
    assert_eq!(planes.len(), 7);
    assert_eq!(
        planes[0].time,
        chrono::DateTime::from_timestamp(1_795_435_200, 0)
    );
    assert_eq!(planes[0].time, planes[1].time);
    assert_eq!(
        (planes[3].time.unwrap() - planes[2].time.unwrap()).num_seconds(),
        3600
    );
    assert_eq!(
        planes[4].time,
        chrono::DateTime::from_timestamp(946_684_800, 0)
    );
    assert_eq!(planes[5].hex, "3C0007");
    assert!(planes[5].time.unwrap() > chrono::DateTime::from_timestamp(1_795_435_200, 0).unwrap());
    assert_eq!(
        planes[6].time,
        chrono::DateTime::from_timestamp(10_000_000, 0)
    );
}

#[test]
fn finding_77_ambiguous_arrays_need_explicit_indices_and_inference_stays_runtime() {
    let body = r#"[
        ["3C0001",50.25,10.25,12000,80,430,123456],
        ["3C0002",50.26,10.26,13000,90,431,-12345]
    ]"#;
    assert!(matches!(run(body, "").1, Err(FeedError::Config(_))));
    let (feed, result) = run(body, "index_lat = 2\nindex_lon = 3");
    let planes = result.unwrap();
    assert_eq!(planes.len(), 2);
    assert_eq!((planes[0].lat, planes[0].lon), (50.25, 10.25));
    assert!(planes
        .iter()
        .all(|plane| plane.reg.is_empty() && plane.call.is_empty()));
    assert_eq!(indices(feed.as_ref()), [-1, -1, -1, 2, 3, -1, -1, -1, -1]);
}
