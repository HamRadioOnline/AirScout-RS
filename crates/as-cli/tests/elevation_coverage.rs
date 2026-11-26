//! `airscout-cli elevation-coverage`: the C# ElevationCoverageMapper as a JSON and GeoJSON tool.

mod common;

use assert_cmd::Command;
use common::fixture_data_dir;
use mockito::Matcher;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;

/// The test catalogue: four valid tiles in three squares, one non-locator key (`XX99ZZ`), a line
/// without `;` and a line with a bad date (both skipped by the reader).
const CAT: &str = "JO50AA.loc;2018-02-27 08:26:32Z\n\
jo50ab.loc;2018-02-27 08:26:32Z\n\
JO61UF.loc;2018-02-27 08:26:32Z\n\
JN58TD.loc;2018-02-27 08:26:32Z\n\
XX99ZZ.loc;2018-02-27 08:26:32Z\n\
broken line\n\
JO50AC.loc;bad\n";

fn write_cat(dir: &Path, text: &str) -> std::path::PathBuf {
    let p = dir.join("files.cat");
    std::fs::write(&p, text).unwrap();
    p
}

fn zip_of(cat: &str) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("files.cat", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(cat.as_bytes()).unwrap();
    z.finish().unwrap().into_inner()
}

/// Run the subcommand with a fresh temp dir for the child (`TMPDIR`, `TMP`, `TEMP`).
fn coverage(tmp: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("airscout-cli")
        .unwrap()
        .arg("elevation-coverage")
        .args(args)
        .env("TMPDIR", tmp)
        .env("TMP", tmp)
        .env("TEMP", tmp)
        .output()
        .unwrap()
}

fn ok_json(out: &std::process::Output) -> Value {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn fails_with(out: &std::process::Output, text: &str) {
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(text), "stderr: {err}");
}

#[test]
fn needs_catalogue_or_download() {
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", Matcher::Any).expect(0).create();
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cat = write_cat(dir.path(), CAT);
    let url = server.url();
    let out = coverage(tmp.path(), &["--base-url", &url]);
    fails_with(&out, "give --catalogue");
    let out = coverage(
        tmp.path(),
        &[
            "--base-url",
            &url,
            "--download",
            "--catalogue",
            cat.to_str().unwrap(),
        ],
    );
    fails_with(&out, "not both");
    mock.assert();
}

#[test]
fn download_once_with_user_agent() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/GLOBE/files.zip")
        .match_header("user-agent", Matcher::Regex("^AirScout-RS/".into()))
        .with_body(zip_of(CAT))
        .expect(1)
        .create();
    let tmp = tempfile::tempdir().unwrap();
    let url = server.url();
    let out = coverage(tmp.path(), &["--download", "--base-url", &url]);
    let v = ok_json(&out);
    mock.assert();
    assert_eq!(v["tiles"], 4);
    assert_eq!(v["invalid"], 1);
    assert_eq!(v["squares"], 3);
    assert_eq!(v["model"], "GLOBE");
    assert_eq!(v["catalogue"], format!("{url}/GLOBE/files.zip"));
    assert_eq!(v["found"], Value::Null);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!(
            "GLOBE: Downloading elevation tile catalogue from {url}/GLOBE/files.zip"
        )),
        "stderr: {err}"
    );
    let left: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(left.is_empty(), "temp dir not empty: {left:?}");
}

#[test]
fn download_error_removes_temp_dir() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/GLOBE/files.zip")
        .with_body("not a zip")
        .expect(1)
        .create();
    let tmp = tempfile::tempdir().unwrap();
    let out = coverage(tmp.path(), &["--download", "--base-url", &server.url()]);
    assert_eq!(out.status.code(), Some(1));
    mock.assert();
    let left: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(left.is_empty(), "temp dir not empty: {left:?}");
}

#[test]
fn counts_found_and_missing_against_store() {
    let data = fixture_data_dir();
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cat = write_cat(dir.path(), CAT);
    let out = coverage(
        tmp.path(),
        &[
            "--catalogue",
            cat.to_str().unwrap(),
            "--data",
            data.path().to_str().unwrap(),
        ],
    );
    let v = ok_json(&out);
    assert_eq!(v["found"], 3);
    assert_eq!(v["missing"], 1);
    assert_eq!(v["invalid"], 1);
    assert_eq!(v["tiles"], 4);
    assert_eq!(v["area"], json!([-90.0, -180.0, 90.0, 180.0]));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("GLOBE: 3 tile(s) found, 1 more tile(s) available and missing."),
        "stderr: {err}"
    );
}

#[test]
fn catalogue_zip_by_extension_any_case() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let p = dir.path().join("FILES.ZIP");
    std::fs::write(&p, zip_of(CAT)).unwrap();
    let v = ok_json(&coverage(tmp.path(), &["--catalogue", p.to_str().unwrap()]));
    assert_eq!(v["tiles"], 4);
    assert_eq!(v["catalogue"], p.to_str().unwrap());
}

#[test]
fn no_store_creates_nothing() {
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cat = write_cat(dir.path(), CAT);
    let v = ok_json(&coverage(
        tmp.path(),
        &[
            "--catalogue",
            cat.to_str().unwrap(),
            "--data",
            data.path().to_str().unwrap(),
        ],
    ));
    assert_eq!(v["found"], 0);
    assert_eq!(v["missing"], 4);
    assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
}

#[test]
fn area_filter_crosses_square_corner() {
    // #289: the four tiles at the corner of JO50, JO51, JO60 and JO61, and one far away.
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let ts = ";2018-02-27 08:26:32Z\n";
    let text: String = ["JO50XX", "JO51XA", "JO60AX", "JO61AA", "JO50AA"]
        .iter()
        .map(|t| format!("{t}.loc{ts}"))
        .collect();
    let cat = write_cat(dir.path(), &text);
    let c = cat.to_str().unwrap();
    let v = ok_json(&coverage(
        tmp.path(),
        &["--catalogue", c, "--area", "50.99,11.99,51.01,12.01"],
    ));
    assert_eq!(v["tiles"], 4);
    assert_eq!(v["squares"], 4);
    assert_eq!(v["area"], json!([50.99, 11.99, 51.01, 12.01]));
    let out = coverage(tmp.path(), &["--catalogue", c, "--area", "51,12,50,13"]);
    fails_with(&out, "bad area");
    let out = coverage(tmp.path(), &["--catalogue", c, "--area", "50,12,91,13"]);
    fails_with(&out, "bad area");
}

#[test]
fn geojson_square_and_tile_levels() {
    let data = fixture_data_dir();
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cat = write_cat(dir.path(), CAT);
    let g = dir.path().join("g.json");
    let v = ok_json(&coverage(
        tmp.path(),
        &[
            "--catalogue",
            cat.to_str().unwrap(),
            "--data",
            data.path().to_str().unwrap(),
            "--geojson",
            g.to_str().unwrap(),
        ],
    ));
    assert_eq!(v["geojson"], g.to_str().unwrap());
    let gj: Value = serde_json::from_slice(&std::fs::read(&g).unwrap()).unwrap();
    assert_eq!(gj["type"], "FeatureCollection");
    let feats = gj["features"].as_array().unwrap();
    assert_eq!(feats.len(), 3);
    let jo50 = feats
        .iter()
        .find(|f| f["properties"]["square"] == "JO50")
        .unwrap();
    assert_eq!(
        jo50["properties"],
        json!({"square": "JO50", "tiles": 2, "found": 2, "missing": 0})
    );
    assert_eq!(jo50["geometry"]["type"], "Polygon");
    let ring = jo50["geometry"]["coordinates"][0].as_array().unwrap();
    assert_eq!(ring.len(), 5);
    assert_eq!(ring[0], json!([10.0, 50.0]));
    assert_eq!(ring[0], ring[4]);
    assert_eq!(ring[2], json!([12.0, 51.0]));
    assert!(!dir.path().join("g.json.part").exists());

    let v = ok_json(&coverage(
        tmp.path(),
        &[
            "--catalogue",
            cat.to_str().unwrap(),
            "--geojson",
            g.to_str().unwrap(),
            "--level",
            "tile",
        ],
    ));
    assert_eq!(v["found"], Value::Null);
    let gj: Value = serde_json::from_slice(&std::fs::read(&g).unwrap()).unwrap();
    let feats = gj["features"].as_array().unwrap();
    assert_eq!(feats.len(), 4);
    assert!(feats
        .iter()
        .all(|f| f["properties"]["state"] == "available"));
    assert!(feats.iter().any(|f| f["properties"]["tile"] == "JO50AB"));
}
