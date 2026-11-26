mod common;

use common::fixture_data_dir;

use as_geo::{latlon, maidenhead};
use assert_cmd::Command;

const MM: maidenhead::PositionInRectangle = maidenhead::PositionInRectangle::MiddleMiddle;

fn run(args: &[&str]) -> serde_json::Value {
    let out = Command::cargo_bin("airscout-cli")
        .unwrap()
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn path_on_fixture_data() {
    let dir = fixture_data_dir();
    let data = dir.path().to_str().unwrap();
    let v = run(&[
        "path", "--from", "JO50AA", "--to", "JO61UF", "--band", "144", "--h1", "10", "--h2", "10",
        "--model", "globe", "--data", data,
    ]);
    let a = maidenhead::gpoint_from_loc("JO50AA", MM).unwrap();
    let b = maidenhead::gpoint_from_loc("JO61UF", MM).unwrap();
    let d = latlon::distance(a.lat, a.lon, b.lat, b.lon);
    assert_eq!(v["valid"], true, "{v}");
    assert!(
        (v["distance_km"].as_f64().unwrap() - d).abs() <= 1e-9,
        "{v}"
    );
    assert!(
        (v["bearing12"].as_f64().unwrap() - latlon::bearing(a.lat, a.lon, b.lat, b.lon)).abs()
            <= 1e-9
    );
    assert!(v["eps1_min_deg"].as_f64().unwrap().is_finite());
    assert!(v["eps2_min_deg"].as_f64().unwrap().is_finite());
    let info = v["info_points"].as_array().unwrap();
    assert_eq!(info.len(), d as usize + 2);
    let end = info.last().unwrap();
    assert!((end["dist_km"].as_f64().unwrap() - d).abs() < 1e-9, "#352");
    assert_eq!(end["f1"], 0.0);
}

#[test]
fn path_outside_data_is_invalid_not_error() {
    let dir = fixture_data_dir();
    let v = run(&[
        "path",
        "--from",
        "JO50AA",
        "--to",
        "JO72AA",
        "--band",
        "144",
        "--model",
        "globe",
        "--data",
        dir.path().to_str().unwrap(),
    ]);
    assert_eq!(v["valid"], false, "{v}");
}

#[test]
fn horizon_prints_360_points() {
    let dir = fixture_data_dir();
    let v = run(&[
        "horizon",
        "--loc",
        "JO61AA",
        "--h",
        "10",
        "--dist",
        "50",
        "--band",
        "432",
        "--model",
        "globe",
        "--data",
        dir.path().to_str().unwrap(),
    ]);
    let pts = v["points"].as_array().unwrap();
    assert_eq!(pts.len(), 360);
    assert_eq!(pts[90]["azimuth"], 90);
    assert_eq!(v["valid"], true);
}

#[test]
fn bad_locator_is_error_exit() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("airscout-cli")
        .unwrap()
        .args([
            "path",
            "--from",
            "XX99",
            "--to",
            "JO61UF",
            "--data",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

/// Review I1: squares the server has no file for (404, open sea) count as downloaded, so a path
/// over them is valid; they are remembered for later runs without --download.
#[test]
fn sea_squares_count_as_complete() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex("^/GLOBE/JO/JO[5-7][0-9].zip$".into()),
        )
        .with_status(404)
        .expect_at_least(3)
        .create();
    let dir = fixture_data_dir();
    let data = dir.path().to_str().unwrap();
    let url = server.url();
    let args = [
        "path",
        "--from",
        "JO50AA",
        "--to",
        "JO72AA",
        "--band",
        "144",
        "--model",
        "globe",
        "--data",
        data,
        "--base-url",
        &url,
    ];
    let mut with_dl = args.to_vec();
    with_dl.push("--download");
    assert_eq!(run(&with_dl)["valid"], true);
    _m.assert();
    assert_eq!(run(&args)["valid"], true);
}

#[test]
fn server_error_on_download_is_error_exit() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(500)
        .create();
    let dir = fixture_data_dir();
    Command::cargo_bin("airscout-cli")
        .unwrap()
        .args([
            "path",
            "--from",
            "JO50AA",
            "--to",
            "JO72AA",
            "--model",
            "globe",
            "--download",
            "--base-url",
            &server.url(),
            "--data",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn finding_15_antimeridian_prepares_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_str().unwrap();
    for args in [
        vec![
            "path",
            "--from",
            "10,179.5",
            "--to",
            "10,-179.5",
            "--data",
            data,
        ],
        vec![
            "horizon", "--loc", "10,179.9", "--dist", "50", "--data", data,
        ],
    ] {
        let out = Command::cargo_bin("airscout-cli")
            .unwrap()
            .args(&args)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}: {err}");
        let checked: Vec<_> = err
            .lines()
            .filter_map(|line| line.split_once(" tiles for square ").map(|(_, sq)| sq))
            .collect();
        assert_eq!(checked, ["RJ99", "AJ09", "RK90", "AK00"]);
    }
}

// ------------------------------------------------------------------ stage 2

fn run_err(args: &[&str]) -> String {
    let out = Command::cargo_bin("airscout-cli")
        .unwrap()
        .args(args)
        .output()
        .unwrap();
    assert!(!out.status.success());
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn feeds_lists_all_11() {
    let v = run(&["feeds"]);
    let feeds = v.as_array().unwrap();
    assert_eq!(feeds.len(), 11, "{v}");
    let names: Vec<&str> = feeds.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names[0], "OpenSky");
    assert!(names.contains(&"FlexJSON") && names.contains(&"RTL1090"));
    let ads = feeds.iter().find(|f| f["name"] == "ADSBSupport").unwrap();
    assert_eq!(ads["enabled_by_default"], false);
    assert!(ads["disclaimer"].as_str().unwrap().contains("credentials"));
    let osk = feeds.iter().find(|f| f["name"] == "OpenSky").unwrap();
    assert_eq!(osk["enabled_by_default"], true);
    assert!(osk["disclaimer"].is_null());
}

#[test]
fn planes_from_mock_readsb() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let body = format!(
        r#"{{"now":{now}.5,"aircraft":[
        {{"hex":"3c6589","flight":"DLH4AB  ","alt_baro":37000,"gs":482.3,"track":245.71,"lat":50.612345,"lon":10.987654,"seen_pos":1.2}},
        {{"hex":"4ca7b2","flight":"RYR12KA ","alt_baro":35975,"gs":451.0,"track":123.4,"lat":50.301,"lon":11.402,"seen_pos":0.0}},
        {{"hex":"3e1c40","flight":"DIEXY   ","alt_baro":3500,"gs":102.0,"track":15.0,"lat":50.45,"lon":11.12,"seen_pos":2.5}},
        {{"hex":"3c0001","flight":"FAR1    ","alt_baro":30000,"gs":400.0,"track":90.0,"lat":53.0,"lon":11.0,"seen_pos":0.5}}]}}"#
    );
    let mut server = mockito::Server::new();
    let m = server
        .mock("GET", "/data/aircraft.json")
        .with_body(body)
        .expect(2)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let url = format!("url={}/data/aircraft.json", server.url());
    let v = run(&[
        "planes",
        "--feed",
        "Readsb",
        "--area",
        "50,10,51,12",
        "--data",
        dir.path().to_str().unwrap(),
        "--cycles",
        "2",
        "--feed-setting",
        &url,
    ]);
    m.assert();
    let planes = v.as_array().unwrap();
    // 3500 ft is below Planes_MinAlt (5000 m), 53° N is outside the area
    let hexes: Vec<&str> = planes.iter().map(|p| p["hex"].as_str().unwrap()).collect();
    assert_eq!(hexes.len(), 2, "{v}");
    assert!(hexes
        .iter()
        .all(|h| ["3C6589", "4CA7B2"].contains(&h.to_uppercase().as_str())));
    let p = planes
        .iter()
        .find(|p| p["hex"].as_str().unwrap().eq_ignore_ascii_case("3c6589"))
        .unwrap();
    assert_eq!(p["alt_ft"], 37000.0);
    assert_eq!(p["lat"], 50.612345);
    // the aircraft database is written by the worker
    assert!(dir.path().join("aircrafts.db3").exists());
}

#[test]
fn planes_unknown_feed_or_bad_area_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    assert!(run_err(&[
        "planes",
        "--feed",
        "Nope",
        "--area",
        "50,10,51,12",
        "--data",
        d
    ])
    .contains("unknown feed"));
    assert!(
        run_err(&["planes", "--feed", "Readsb", "--area", "50,10,51", "--data", d])
            .contains("area")
    );
    // the worker reports the swapped area as an error, as C# does
    let e = run_err(&[
        "planes",
        "--feed",
        "Readsb",
        "--area",
        "51,10,50,12",
        "--data",
        d,
    ]);
    assert!(e.contains("Area boundaries mismatch"), "{e}");
}

#[test]
fn nearest_on_fixture_path() {
    let dir = fixture_data_dir();
    let data = dir.path().to_str().unwrap();
    let a = maidenhead::gpoint_from_loc("JO50AA", MM).unwrap();
    let b = maidenhead::gpoint_from_loc("JO61UF", MM).unwrap();
    let mid = latlon::midpoint(a.lat, a.lon, b.lat, b.lon);
    let brg = latlon::bearing(a.lat, a.lon, b.lat, b.lon);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // one plane on the path midpoint at 36000 ft flying across it, one far away
    let planes = serde_json::json!([
        {"Hex": "3C6589", "Call": "DLH4AB", "Lat": mid.lat, "Lon": mid.lon, "Alt": 36000.0,
         "Track": (brg + 90.0) % 360.0, "Speed": 450.0, "Type": "A320", "Category": 3, "Time": now},
        {"Hex": "4CA7B2", "Call": "RYR12KA", "Lat": 45.0, "Lon": 2.0, "Alt": 36000.0,
         "Track": 90.0, "Speed": 450.0, "Time": now}
    ]);
    let file = dir.path().join("planes.json");
    std::fs::write(&file, planes.to_string()).unwrap();
    let v = run(&[
        "nearest",
        "--from",
        "JO50AA",
        "--to",
        "JO61UF",
        "--planes",
        file.to_str().unwrap(),
        "--band",
        "144",
        "--model",
        "globe",
        "--data",
        data,
    ]);
    let found = v["planes"].as_array().unwrap();
    // the far plane is outside half the path length around the midpoint
    assert_eq!(found.len(), 1, "{v}");
    let p = &found[0];
    assert_eq!(p["hex"], "3C6589");
    assert_eq!(p["potential"], 100, "{v}");
    assert!(p["int_qrb"].as_f64().unwrap() < 20.0, "{v}");
    assert!(p["minutes"].is_null() || p["minutes"] == 0, "{v}");
    assert_eq!(v["valid"], true);
}

#[test]
fn nearest_reports_null_eta_for_near_receding_plane() {
    let dir = fixture_data_dir();
    let data = dir.path().to_str().unwrap();
    let a = maidenhead::gpoint_from_loc("JO50AA", MM).unwrap();
    let b = maidenhead::gpoint_from_loc("JO61UF", MM).unwrap();
    let mid = latlon::midpoint(a.lat, a.lon, b.lat, b.lon);
    let brg = latlon::bearing(a.lat, a.lon, b.lat, b.lon);
    let side = latlon::destination_point(mid.lat, mid.lon, brg + 90.0, 1.0);
    let away = (latlon::bearing(side.lat, side.lon, mid.lat, mid.lon) + 180.0) % 360.0;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let planes = serde_json::json!([
        {"Hex": "3C6589", "Call": "DLH4AB", "Lat": side.lat, "Lon": side.lon, "Alt": 36000.0,
         "Track": away, "Speed": 450.0, "Type": "A320", "Category": 3, "Time": now}
    ]);
    let file = dir.path().join("planes_eta_null.json");
    std::fs::write(&file, planes.to_string()).unwrap();
    let v = run(&[
        "nearest",
        "--from",
        "JO50AA",
        "--to",
        "JO61UF",
        "--planes",
        file.to_str().unwrap(),
        "--band",
        "144",
        "--model",
        "globe",
        "--data",
        data,
    ]);
    let found = v["planes"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{v}");
    let p = &found[0];
    assert!(p["int_qrb"].as_f64().unwrap() < 10.0, "{v}");
    assert!(p["minutes"].is_null(), "{v}");
    assert!(p["forward_distance_km"].is_null(), "{v}");
}

#[test]
fn aircraft_update_from_mock() {
    use std::io::Write;
    let zip_of = |name: &str| {
        let body = std::fs::read(
            as_golden::repo_root().join(format!("tests/fixtures/aircraft/{name}.json")),
        )
        .unwrap();
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file(
                format!("{name}.json"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            z.write_all(&body).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    };
    let mut server = mockito::Server::new();
    let _m: Vec<_> = [
        "AircraftTypes",
        "Airports",
        "Aircrafts",
        "AircraftRegistrations",
        "Airlines",
    ]
    .iter()
    .map(|n| {
        server
            .mock("GET", format!("/{n}.zip").as_str())
            .with_body(zip_of(n))
            .create()
    })
    .collect();
    let dir = tempfile::tempdir().unwrap();
    let v = run(&[
        "aircraft-update",
        "--data",
        dir.path().to_str().unwrap(),
        "--base-url",
        &format!("{}/", server.url()),
    ]);
    assert_eq!(v["aircrafts"], 16, "{v}");
    assert_eq!(v["aircraft_types"], 26);
    assert_eq!(v["airlines"], 10);
    assert_eq!(v["airports"], 8);
    assert_eq!(v["registrations"], 12);
}
