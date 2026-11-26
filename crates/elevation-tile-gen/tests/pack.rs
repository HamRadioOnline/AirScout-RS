//! The `elevation-tile-gen zip` and `catalogue` commands, and a self-hosted tree read by the
//! app's updater.

mod common;

use as_elevation::{
    read_files_zip, update_square, Catalogue, ElevationModel, ElevationStore, TILE_MISSING,
};
use as_geo::latlon::GRect;
use as_geo::maidenhead::{self, PositionInRectangle};
use assert_cmd::Command;
use chrono::{DateTime, Utc};
use common::{golden, source_case, Synth};
use elevation_tile_gen::source::identify;
use elevation_tile_gen::tiles::{srtm_tiles, Outcome};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const T1: &str = "2026-11-24T12:00:00Z";
const T2: &str = "2026-11-25T08:00:00Z";

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("elevation-tile-gen")
        .unwrap()
        .args(args)
        // hang guard only, not a speed check: slow, loaded CI runners need minutes
        .timeout(Duration::from_secs(600))
        .output()
        .unwrap()
}

fn ok_json(o: &std::process::Output) -> Value {
    assert!(
        o.status.success(),
        "exit {:?}\nstderr: {}",
        o.status,
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).expect("JSON on stdout")
}

fn failed(o: &std::process::Output) -> String {
    assert_eq!(o.status.code(), Some(1), "stdout: {:?}", o.stdout);
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// A tile tree from the golden synthetic sources `names` (written to `tmp`), at `T1`.
fn tree(tmp: &Path, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
    let g = golden();
    let srcs: Vec<PathBuf> = names
        .iter()
        .map(|n| Synth::from_case(source_case(&g, n)).write(tmp))
        .collect();
    let out = tmp.join("out");
    let mut args = vec!["tiles"];
    args.extend(srcs.iter().map(|p| s(p)));
    args.extend(["--out", s(&out), "--time", T1]);
    ok_json(&run(&args));
    (out, srcs)
}

fn zip_cmd(out: &Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["zip", "--out", s(out)];
    args.extend(extra);
    run(&args)
}

fn catalogue_cmd(out: &Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["catalogue", "--out", s(out)];
    args.extend(extra);
    run(&args)
}

/// The `.loc` file names in `dir`, sorted (a directory listing, not the code under test).
fn loc_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".loc"))
        .collect();
    names.sort();
    names
}

fn archive(path: &Path) -> zip::ZipArchive<std::fs::File> {
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

fn entry_bytes(z: &mut zip::ZipArchive<std::fs::File>, i: usize) -> (String, Vec<u8>) {
    let mut f = z.by_index(i).unwrap();
    let mut b = Vec::new();
    f.read_to_end(&mut b).unwrap();
    (f.name().to_string(), b)
}

#[test]
fn zip_packs_square_flat_deflated() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    let v = ok_json(&zip_cmd(&out, &[]));
    assert_eq!(
        v,
        serde_json::json!({"squares": 1, "tiles": 287, "removed": 0})
    );
    let dir = out.join("JO").join("50");
    let names = loc_names(&dir);
    assert_eq!(names.len(), 287);
    assert_eq!(names[0], "JO50MB.loc");
    let mut z = archive(&out.join("JO").join("JO50.zip"));
    assert_eq!(z.len(), 287);
    for (i, want) in names.iter().enumerate() {
        {
            let f = z.by_index(i).unwrap();
            assert_eq!(f.compression(), zip::CompressionMethod::Deflated, "{want}");
            let t = f.last_modified().unwrap();
            assert_eq!(
                (
                    t.year(),
                    t.month(),
                    t.day(),
                    t.hour(),
                    t.minute(),
                    t.second()
                ),
                (2026, 11, 24, 12, 0, 0),
                "{want}"
            );
        }
        let (name, bytes) = entry_bytes(&mut z, i);
        assert_eq!(&name, want);
        assert_eq!(bytes, std::fs::read(dir.join(want)).unwrap(), "{want}");
    }
    assert!(!out.join("JO").join("JO50.zip.part").exists());
}

#[test]
fn zip_remove_loc_verifies_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    let v = ok_json(&zip_cmd(&out, &["--remove-loc"]));
    assert_eq!(v["squares"], 1);
    assert_eq!(v["tiles"], 287);
    assert_eq!(v["removed"], 287);
    assert!(!out.join("JO").join("50").exists());
    assert_eq!(archive(&out.join("JO").join("JO50.zip")).len(), 287);
}

#[test]
fn zip_remove_loc_keeps_other_files() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    let dir = out.join("JO").join("50");
    std::fs::write(dir.join("notes.txt"), "keep").unwrap();
    std::fs::write(out.join("JO").join("JO50MB.loc"), "not in a square dir").unwrap();
    let v = ok_json(&zip_cmd(&out, &["--remove-loc"]));
    assert_eq!(v["removed"], 287);
    assert!(loc_names(&dir).is_empty());
    assert_eq!(std::fs::read(dir.join("notes.txt")).unwrap(), b"keep");
    assert!(out.join("JO").join("JO50MB.loc").exists());
}

#[test]
fn zip_bad_tile_removes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    ok_json(&zip_cmd(&out, &[]));
    let zip_path = out.join("JO").join("JO50.zip");
    let old = std::fs::read(&zip_path).unwrap();
    assert_eq!(archive(&zip_path).len(), 287);
    let dir = out.join("JO").join("50");
    let mb = dir.join("JO50MB.loc");
    let mut text = std::fs::read(&mb).unwrap();
    text.push(b'x');
    std::fs::write(&mb, &text).unwrap();

    let e = failed(&zip_cmd(&out, &["--remove-loc"]));
    assert!(e.contains("JO50MB.loc"), "{e}");
    assert_eq!(loc_names(&dir).len(), 287);
    assert_eq!(std::fs::read(&zip_path).unwrap(), old);
    assert!(!out.join("JO").join("JO50.zip.part").exists());
}

/// The names and bytes of every entry of a zip.
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut z = archive(path);
    (0..z.len()).map(|i| entry_bytes(&mut z, i)).collect()
}

#[test]
fn zip_again_keeps_tiles_of_an_earlier_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let g = golden();
    let e011 = Synth::from_case(source_case(&g, "N50E011.hgt")).write(tmp.path());
    // the same synthetic data as the west half of JO50 (SRTM3, `JO50AB` .. `JO50LX`)
    let e010 = Synth::from_case(source_case(&g, "N50E011.hgt")).write_as(tmp.path(), "N50E010.hgt");
    let out = tmp.path().join("out");
    let tiles = |src: &Path, time: &str| {
        ok_json(&run(&["tiles", s(src), "--out", s(&out), "--time", time]));
    };
    let zip_path = out.join("JO").join("JO50.zip");

    tiles(&e011, T1);
    assert_eq!(ok_json(&zip_cmd(&out, &["--remove-loc"]))["removed"], 287);
    let first = entries(&zip_path);
    assert_eq!(first.len(), 287);

    tiles(&e010, T1);
    let v = ok_json(&zip_cmd(&out, &["--remove-loc"]));
    assert_eq!(
        v,
        serde_json::json!({"squares": 1, "tiles": 574, "removed": 287})
    );
    let both = entries(&zip_path);
    assert_eq!(both.len(), 574);
    let names: Vec<&str> = both.iter().map(|(n, _)| n.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(names[0], "JO50AB.loc");
    // every tile of the first batch, byte for byte
    for (name, bytes) in &first {
        let (_, b) = both.iter().find(|(n, _)| n == name).unwrap();
        assert_eq!(b, bytes, "{name}");
    }

    // a .loc file on disk replaces the old entry of the same name
    tiles(&e011, T2);
    let mb = std::fs::read(out.join("JO").join("50").join("JO50MB.loc")).unwrap();
    assert!(String::from_utf8_lossy(&mb).contains("2026-11-25T08:00:00Z"));
    let v = ok_json(&zip_cmd(&out, &[]));
    assert_eq!(
        v,
        serde_json::json!({"squares": 1, "tiles": 574, "removed": 0})
    );
    let again = entries(&zip_path);
    assert_eq!(again.len(), 574);
    let (_, b) = again.iter().find(|(n, _)| n == "JO50MB.loc").unwrap();
    assert_eq!(b, &mb);
    let (_, b) = again.iter().find(|(n, _)| n == "JO50AB.loc").unwrap();
    assert_eq!(b, &both.iter().find(|(n, _)| n == "JO50AB.loc").unwrap().1);
}

#[test]
fn zip_rejects_a_doubled_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    let dir = out.join("JO").join("50");
    std::fs::copy(dir.join("JO50MB.loc"), dir.join("JO50MB.loc.loc")).unwrap();
    let e = failed(&zip_cmd(&out, &["--remove-loc"]));
    assert!(e.contains("JO50MB.loc.loc"), "{e}");
    assert_eq!(loc_names(&dir).len(), 288);
    assert!(!out.join("JO").join("JO50.zip").exists());
}

#[test]
fn catalogue_lists_every_tile() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    ok_json(&zip_cmd(&out, &[]));
    let v = ok_json(&catalogue_cmd(&out, &["--time", T2]));
    assert_eq!(
        v,
        serde_json::json!({"tiles": 287, "from": "zip", "files": ["files.cat", "files.zip"]})
    );

    // the expected file from literals and a directory listing (pre-flight C7)
    let mut want = String::from(
        "// (D)igital (E)levation (Model) tile catalogue\r\n\
         // created 2026-11-25 with elevation-tile-gen (AirScout-RS)\r\n\
         // GLOBE data homepage: https://www.ngdc.noaa.gov/mgg/topo/globe.html\r\n\
         // SRTM3 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl3_v003\r\n\
         // SRTM1 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl1_v003\r\n\
         \r\n",
    );
    let names = loc_names(&out.join("JO").join("50"));
    assert_eq!(names.len(), 287);
    for n in &names {
        want.push_str(&format!("{n};2026-11-24 12:00:00Z\r\n"));
    }
    let cat = std::fs::read(out.join("files.cat")).unwrap();
    assert_eq!(String::from_utf8(cat.clone()).unwrap(), want);

    let mut z = archive(&out.join("files.zip"));
    assert_eq!(z.len(), 1);
    let (name, bytes) = entry_bytes(&mut z, 0);
    assert_eq!(name, "files.cat");
    assert_eq!(bytes, cat);

    let files = read_files_zip(&out.join("files.zip")).unwrap();
    assert_eq!(files.len(), 287);
    assert!(files.keys().all(|k| k.starts_with("JO50")), "{files:?}");

    // the root files.zip is not scanned
    ok_json(&catalogue_cmd(&out, &["--time", T2]));
    assert_eq!(std::fs::read(out.join("files.cat")).unwrap(), cat);
    for p in ["files.cat.part", "files.zip.part"] {
        assert!(!out.join(p).exists(), "{p}");
    }
    // not even when it holds tiles: a scan would see every tile twice
    std::fs::copy(out.join("JO").join("JO50.zip"), out.join("files.zip")).unwrap();
    ok_json(&catalogue_cmd(&out, &["--time", T2]));
    assert_eq!(std::fs::read(out.join("files.cat")).unwrap(), cat);
}

#[test]
fn catalogue_from_loc_reads_tile_times() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    // a file time that is not the tile's LastUpdated (finding 8)
    let mb = out.join("JO").join("50").join("JO50MB.loc");
    std::fs::File::options()
        .write(true)
        .open(&mb)
        .unwrap()
        .set_modified(time(T2).into())
        .unwrap();
    let v = ok_json(&catalogue_cmd(&out, &["--from", "loc", "--time", T2]));
    assert_eq!(v["tiles"], 287);
    assert_eq!(v["from"], "loc");
    let text = std::fs::read_to_string(out.join("files.cat")).unwrap();
    assert!(
        text.contains("\r\n\r\nJO50MB.loc;2026-11-24 12:00:00Z\r\n"),
        "{text:.600}"
    );
    assert!(text.ends_with("JO50XX.loc;2026-11-24 12:00:00Z\r\n"));
}

#[test]
fn catalogue_duplicate_tile_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, _) = tree(tmp.path(), &["N50E011.hgt"]);
    ok_json(&zip_cmd(&out, &[]));
    let jo = out.join("JO");
    std::fs::copy(jo.join("JO50.zip"), jo.join("JO50-copy.zip")).unwrap();
    let e = failed(&catalogue_cmd(&out, &[]));
    assert!(e.contains("tile JO50MB.loc is in"), "{e}");
    assert!(!out.join("files.cat").exists());
}

fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn self_hosted_tree_feeds_the_updater() {
    let tmp = tempfile::tempdir().unwrap();
    let (out, srcs) = tree(tmp.path(), &["N50E011.hgt", "S01W002.hgt"]);
    let v = ok_json(&zip_cmd(&out, &[]));
    assert_eq!(v["squares"], 2);
    assert_eq!(v["tiles"], 287 + 288);
    ok_json(&catalogue_cmd(&out, &["--time", T2]));

    let mut server = mockito::Server::new();
    let mut mocks = Vec::new();
    for rel in ["files.zip", "JO/JO50.zip", "II/II99.zip"] {
        let body = std::fs::read(out.join(rel)).unwrap();
        mocks.push(
            server
                .mock("GET", format!("/SRTM3/{rel}").as_str())
                .with_status(200)
                .with_body(body)
                .expect(1)
                .create(),
        );
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let cache = tempfile::tempdir().unwrap();
    let cat = Catalogue::fetch(
        &client,
        &server.url(),
        ElevationModel::Srtm3,
        GRect::new(-1.5, -2.5, 51.0, 12.0),
        cache.path(),
    )
    .unwrap();

    // every generated tile, from the directory listings
    let mut generated = BTreeSet::new();
    for dir in [out.join("JO").join("50"), out.join("II").join("99")] {
        for n in loc_names(&dir) {
            generated.insert(n.trim_end_matches(".loc").to_string());
        }
    }
    assert_eq!(generated.len(), 287 + 288);
    let listed: BTreeSet<String> = cat.files.keys().cloned().collect();
    assert_eq!(listed, generated);
    assert!(cat.files.values().all(|t| *t == time(T1)));

    let db = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(db.path()).unwrap();
    let srtm3 = ElevationModel::Srtm3;
    let n = update_square(
        &client,
        &server.url(),
        srtm3,
        "JO50",
        &store,
        None,
        &mut |_| {},
    );
    assert_eq!(n.unwrap(), 287);
    let n = update_square(
        &client,
        &server.url(),
        srtm3,
        "II99",
        &store,
        None,
        &mut |_| {},
    );
    assert_eq!(n.unwrap(), 288);
    for m in &mocks {
        m.assert();
    }

    let mut checked = 0;
    for src in &srcs {
        let id = identify(src).unwrap().unwrap();
        let data = std::fs::read(src).unwrap();
        for o in srtm_tiles(&id, &data, time(T1), &|_| false).unwrap() {
            let Outcome::Tile(t) = o else { continue };
            let (step_lat, step_lon) = (1.0 / 24.0 / t.rows as f64, 2.0 / 24.0 / t.columns as f64);
            for cell in (0..t.rows * t.columns).step_by(7) {
                let (col, row) = (cell / t.rows, cell % t.rows);
                let lat = t.base_lat() + (row as f64 + 0.5) * step_lat;
                let lon = t.base_lon() + (col as f64 + 0.5) * step_lon;
                let e = store.elevation(srtm3, lat, lon, false).unwrap();
                assert_eq!(e, t.get_elevation(lat, lon), "{} ({row}, {col})", t.index);
                checked += 1;
            }
        }
    }
    assert!(checked > (287 + 288) * 700, "{checked}");

    // a tile the generator skipped stays missing
    let lat = maidenhead::lat_from_loc("JO50MA", PositionInRectangle::MiddleMiddle).unwrap();
    let lon = maidenhead::lon_from_loc("JO50MA", PositionInRectangle::MiddleMiddle).unwrap();
    assert_eq!(
        store.elevation(srtm3, lat, lon, false).unwrap(),
        TILE_MISSING
    );
}
