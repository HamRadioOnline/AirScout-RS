//! `airscout-cli horizon-gen`: the C# HorizonGenerator with the correct CSV columns.

mod common;

use as_elevation::{ElevationModel, ElevationStore};
use as_geo::band::Band;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_propagation::epsilon_max_from_path;
use as_settings::default_band_table;
use as_stations::{GeoSource, Location, StationDb};
use as_wintest::dotnet::fixed;
use assert_cmd::Command;
use chrono::Utc;
use common::fixture_data_dir;
use serde_json::json;
use std::path::Path;

const HEADER: &str = "bearing[deg];eps_50M[deg];eps_70M[deg];eps_144M[deg];eps_432M[deg];\
eps_1.2G[deg];eps_2.3G[deg];eps_3.4G[deg];eps_5.7G[deg];eps_10G[deg];eps_24G[deg];eps_47G[deg];\
eps_76G[deg]";

/// Store a station at the centre of its locator in `<dir>/stations.db3`.
fn add_station(dir: &Path, call: &str, loc: &str) {
    let p = maidenhead::gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle).unwrap();
    let db = StationDb::open(&dir.join("stations.db3")).unwrap();
    db.location_insert_or_update_if_newer(&Location::new(
        call,
        loc,
        p.lat,
        p.lon,
        GeoSource::FromUser,
        0,
        Utc::now(),
    ))
    .unwrap();
}

fn gen(data: &Path, out: &Path, extra: &[&str]) -> std::process::Output {
    Command::cargo_bin("airscout-cli")
        .unwrap()
        .arg("horizon-gen")
        .args(["--model", "globe", "--dist", "20", "--data"])
        .arg(data)
        .arg("--out")
        .arg(out)
        .args(extra)
        .output()
        .unwrap()
}

fn gen_ok(data: &Path, out: &Path, extra: &[&str]) -> serde_json::Value {
    let o = gen(data, out, extra);
    assert!(
        o.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}

fn gen_err(data: &Path, out: &Path, extra: &[&str]) -> String {
    let o = gen(data, out, extra);
    assert!(!o.status.success(), "expected failure");
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// CSV rows split into fields, header excluded.
fn rows(path: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .skip(1)
        .map(|l| l.split(';').map(str::to_string).collect())
        .collect()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Ground height plus 40 m and the bearing path of TE1ST at JO61AA, as the tool computes them.
fn te1st_path(data: &Path, bearing: f64) -> (f64, Vec<i16>) {
    let p = maidenhead::gpoint_from_loc("JO61AA", PositionInRectangle::MiddleMiddle).unwrap();
    let store = ElevationStore::open(data).unwrap();
    let m = ElevationModel::Globe;
    let h1 = f64::from(store.elevation(m, p.lat, p.lon, true).unwrap()) + 40.0;
    let ep = store
        .create_path_from_bearing(m, p.lat, p.lon, bearing, 20.0, 1000.0, false, None)
        .unwrap()
        .unwrap();
    (h1, ep.path)
}

#[test]
fn writes_one_csv_per_station() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE1ST", "JO61AA");
    add_station(dir.path(), "TE2ND", "JO50XX");
    let out = tempfile::tempdir().unwrap();
    let v = gen_ok(dir.path(), out.path(), &[]);
    assert_eq!(v["stations"], 2, "{v}");
    assert_eq!(v["incomplete"], json!([]), "{v}");
    for f in ["TE1ST_JO61AA.csv", "TE2ND_JO50XX.csv"] {
        let text = std::fs::read_to_string(out.path().join(f)).unwrap();
        assert!(text.ends_with('\n'));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 361, "{f}");
        assert_eq!(lines[0], HEADER);
        for (j, l) in lines[1..].iter().enumerate() {
            let fields: Vec<&str> = l.split(';').collect();
            assert_eq!(fields.len(), 13, "{f} line {j}: {l}");
            assert_eq!(fields[0], j.to_string());
            for x in &fields[1..] {
                let (_, frac) = x.split_once('.').unwrap_or_else(|| panic!("{x}"));
                assert_eq!(frac.len(), 8, "{f} line {j}: {x}");
                assert!(frac.bytes().all(|b| b.is_ascii_digit()), "{x}");
            }
        }
    }
}

/// Guards the CSV wiring and format (band order, F1 per band, degree conversion,
/// `fixed(…, 8)`), not the maths: the loop below mirrors the tool's shared-path loop.
#[test]
fn columns_match_library() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE1ST", "JO61AA");
    let out = tempfile::tempdir().unwrap();
    gen_ok(dir.path(), out.path(), &[]);
    let csv = rows(&out.path().join("TE1ST_JO61AA.csv"));
    let table = default_band_table();
    let re = 6371.0 * 1.33;
    for j in [0usize, 90, 271] {
        let (h1, mut elv) = te1st_path(dir.path(), j as f64);
        for (i, b) in Band::all_except_none_and_all().iter().enumerate() {
            let hp = epsilon_max_from_path(
                h1,
                &mut elv,
                20.0,
                1000.0,
                b.to_ghz(),
                table[b].f1_clearance,
                4797.0,
                re,
            );
            let want = fixed(hp.eps_min / std::f64::consts::PI * 180.0, 8);
            assert_eq!(csv[j][i + 1], want, "bearing {j} band {}", b.name());
        }
    }
}

#[test]
fn legacy_f1_uses_csharp_thresholds() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE1ST", "JO61AA");
    let (new, old) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    gen_ok(dir.path(), new.path(), &[]);
    gen_ok(dir.path(), old.path(), &["--legacy-f1"]);
    let a = rows(&new.path().join("TE1ST_JO61AA.csv"));
    let b = rows(&old.path().join("TE1ST_JO61AA.csv"));
    // column 1 = eps_50M (F1 0.1 against 0.2), column 5 = eps_1.2G (0.6 in both)
    assert!(a.iter().zip(&b).any(|(x, y)| x[1] != y[1]));
    assert!(a.iter().zip(&b).all(|(x, y)| x[5] == y[5]));
}

/// Evidence for the Q5 finding: the C# loop's first pass (BNONE, 0 GHz) takes no point and does
/// not cut the shared path, so the C# columns are the correct ones shifted by one.
#[test]
fn bnone_pass_takes_no_point() {
    let dir = fixture_data_dir();
    let (h1, mut elv) = te1st_path(dir.path(), 90.0);
    let n = elv.len();
    let re = 6371.0 * 1.33;
    let hp = epsilon_max_from_path(h1, &mut elv, 20.0, 1000.0, 0.0, 0.2, 4797.0, re);
    assert_eq!(hp.eps_min, f64::MIN);
    assert_eq!(elv.len(), n);
}

#[test]
fn stations_filter_and_missing_db() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE1ST", "JO61AA");
    add_station(dir.path(), "TE2ND", "JO50XX");
    let out = tempfile::tempdir().unwrap();
    gen_ok(dir.path(), out.path(), &["--stations", "te2nd"]);
    assert_eq!(names(out.path()), ["TE2ND_JO50XX.csv"]);

    let e = gen_err(dir.path(), out.path(), &["--stations", "TE9XX"]);
    assert!(e.contains("no station matches TE9XX"), "{e}");

    let empty = fixture_data_dir();
    let out2 = tempfile::tempdir().unwrap();
    let e = gen_err(empty.path(), out2.path(), &[]);
    assert!(e.contains("no station database at"), "{e}");
    assert!(!empty.path().join("stations.db3").exists());
}

#[test]
fn file_names_are_safe_and_unique() {
    let dir = fixture_data_dir();
    for call in ["TE1ST/P", "TE1ST_P", "../TE1ST"] {
        add_station(dir.path(), call, "JO61AA");
    }
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("out");
    let v = gen_ok(dir.path(), &out, &[]);
    assert_eq!(v["stations"], 3, "{v}");
    assert_eq!(
        names(&out),
        [
            "TE1ST_P_JO61AA-2.csv",
            "TE1ST_P_JO61AA.csv",
            "___TE1ST_JO61AA.csv"
        ]
    );
    assert_eq!(names(root.path()), ["out"]);
}

/// APFS and NTFS compare file names without case: names that differ only in case must not collide.
#[test]
fn file_names_differing_in_case_do_not_collide() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE1ST", "JO61AA");
    add_station(dir.path(), "TE1ST", "jo61aa");
    let out = tempfile::tempdir().unwrap();
    let v = gen_ok(dir.path(), out.path(), &[]);
    assert_eq!(v["stations"], 2, "{v}");
    let listed: Vec<String> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_lowercase())
        .collect();
    assert_eq!(listed.len(), 2, "{v}");
    assert_ne!(listed[0], listed[1], "{v}");
    let on_disk = names(out.path());
    assert_eq!(on_disk.len(), 2, "{on_disk:?}");
    let lower: std::collections::HashSet<String> =
        on_disk.iter().map(|n| n.to_lowercase()).collect();
    assert_eq!(lower.len(), 2, "{on_disk:?}");
}

#[test]
fn outside_elevation_data_is_listed_incomplete() {
    let dir = fixture_data_dir();
    add_station(dir.path(), "TE2ND", "JO72AA");
    let out = tempfile::tempdir().unwrap();
    let v = gen_ok(dir.path(), out.path(), &[]);
    let text = std::fs::read_to_string(out.path().join("TE2ND_JO72AA.csv")).unwrap();
    assert_eq!(text.lines().count(), 361);
    assert_eq!(v["incomplete"], json!(["TE2ND_JO72AA.csv"]), "{v}");
}
