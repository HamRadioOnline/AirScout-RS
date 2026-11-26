//! `identify` and the `elevation-tile-gen tiles` command.

mod common;

use as_elevation::{ElevationModel, ElevationStore, ElevationTile, ELV_MISSING, TILE_MISSING};
use as_geo::maidenhead::{self, PositionInRectangle};
use assert_cmd::Command;
use chrono::{DateTime, Utc};
use common::{files_under, golden, source_case, sparse, Synth};
use elevation_tile_gen::source::{identify, SourceKind};
use elevation_tile_gen::tiles::{srtm_tiles, Outcome};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const T1: &str = "2026-11-24T12:00:00Z";
const T2: &str = "2026-11-25T08:30:00Z";
const SRTM3_LEN: u64 = 1201 * 1201 * 2;

fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// The golden `N50E011.hgt` synthetic source, written to `dir`.
fn n50e011(dir: &Path) -> PathBuf {
    Synth::from_case(source_case(&golden(), "N50E011.hgt")).write(dir)
}

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("elevation-tile-gen")
        .unwrap()
        .args(args)
        // hang guard only, not a speed check: slow, loaded CI runners need minutes
        .timeout(Duration::from_secs(600))
        .output()
        .unwrap()
}

fn tiles(src: &[&Path], out: &Path, extra: &[&str]) -> std::process::Output {
    let mut args: Vec<&str> = vec!["tiles"];
    args.extend(src.iter().map(|p| p.to_str().unwrap()));
    args.extend(["--out", out.to_str().unwrap()]);
    args.extend(extra);
    run(&args)
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

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn locs(out: &Path) -> Vec<PathBuf> {
    files_under(out)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "loc"))
        .collect()
}

fn no_part(out: &Path) {
    let part: Vec<_> = files_under(out)
        .into_iter()
        .filter(|p| p.to_string_lossy().ends_with(".part"))
        .collect();
    assert!(part.is_empty(), "{part:?}");
}

fn last_updated_line(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .find(|l| l.starts_with("  \"LastUpdated\""))
        .unwrap()
        .to_string()
}

#[test]
fn identify_accepts_and_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let accept = [
        (
            "N50E011.hgt",
            SRTM3_LEN,
            SourceKind::Srtm3,
            50,
            11,
            1201,
            1201,
        ),
        (
            "s01w002.HGT",
            SRTM3_LEN,
            SourceKind::Srtm3,
            -1,
            -2,
            1201,
            1201,
        ),
        (
            "N50E010.hgt",
            3601 * 3601 * 2,
            SourceKind::Srtm1,
            50,
            10,
            3601,
            3601,
        ),
        (
            "C10G",
            4800 * 10800 * 2,
            SourceKind::Globe,
            50,
            0,
            4800,
            10800,
        ),
    ];
    for (name, len, kind, lat, lon, rows, cols) in accept {
        let p = d.join(name);
        sparse(&p, len);
        let s = identify(&p).unwrap().unwrap_or_else(|| panic!("{name}"));
        assert_eq!(s.path, p);
        assert_eq!(
            (s.kind, s.lat_base, s.lon_base, s.rows, s.cols),
            (kind, lat, lon, rows, cols),
            "{name}"
        );
    }
    assert_eq!(SourceKind::Srtm3.name(), "SRTM3");
    assert_eq!(SourceKind::Srtm1.model(), ElevationModel::Srtm1);
    assert_eq!(SourceKind::Globe.model(), ElevationModel::Globe);

    let sub = d.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let reject: [(PathBuf, u64); 8] = [
        (d.join("N5.hgt"), SRTM3_LEN),
        (d.join("N50E011X.hgt"), SRTM3_LEN),
        (d.join("x.hgt"), SRTM3_LEN),
        (sub.join("N50E011.hgt"), 100),
        (d.join("C10G.bin"), 4800 * 10800 * 2),
        (d.join("ABC"), 4800 * 10800 * 2),
        (d.join("Q10G"), 4800 * 10800 * 2),
        (d.join("C1XG"), 4800 * 10800 * 2),
    ];
    for (p, len) in &reject {
        sparse(p, *len);
        assert_eq!(identify(p).unwrap(), None, "{p:?}");
    }
    let dir = d.join("D10G");
    std::fs::create_dir(&dir).unwrap();
    assert_eq!(identify(&dir).unwrap(), None);
    assert_eq!(identify(&d.join("missing.hgt")).unwrap(), None);
}

#[test]
fn cli_writes_srtm3_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let src = n50e011(tmp.path());
    let out = tmp.path().join("out");
    let o = tiles(&[&src], &out, &["--time", T1]);
    let v = ok_json(&o);
    assert_eq!(v["model"], "SRTM3");
    assert_eq!(v["sources"], 1);
    assert_eq!(v["written"], 287);
    assert_eq!(v["void"], 1);
    assert_eq!(v["kept"], 0);
    assert!(
        stderr(&o).contains("N50E011.hgt: 287 written, 1 void, 0 kept"),
        "{}",
        stderr(&o)
    );
    let mb = out.join("JO").join("50").join("JO50MB.loc");
    let text = std::fs::read_to_string(&mb).unwrap();
    assert!(
        text.starts_with("{\r\n  \"TileIndex\": \"JO50MB\",\r\n"),
        "{text:.80}"
    );
    assert!(text.ends_with("  \"LastUpdated\": \"2026-11-24T12:00:00Z\"\r\n}\r\n"));
    let mtime = std::fs::metadata(&mb).unwrap().modified().unwrap();
    let want: SystemTime = time(T1).into();
    let diff = mtime.duration_since(want).unwrap_or_else(|e| e.duration());
    assert!(diff <= Duration::from_secs(2), "{diff:?}");
    assert!(!out.join("JO").join("50").join("JO50MA.loc").exists());
    assert_eq!(locs(&out).len(), 287);
    no_part(&out);
}

#[test]
fn round_trip_through_as_elevation() {
    let tmp = tempfile::tempdir().unwrap();
    let src = n50e011(tmp.path());
    let out = tmp.path().join("out");
    ok_json(&tiles(&[&src], &out, &["--time", T1]));

    let db = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(db.path()).unwrap();
    let parsed: Vec<ElevationTile> = locs(&out)
        .iter()
        .map(|p| ElevationTile::from_loc_json(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect();
    assert_eq!(parsed.len(), 287);
    store
        .insert_tiles_atomic(ElevationModel::Srtm3, &parsed)
        .unwrap();

    let s = identify(&src).unwrap().unwrap();
    let data = std::fs::read(&src).unwrap();
    let mem = srtm_tiles(&s, &data, time(T1), &|_| false).unwrap();
    let mut checked = 0;
    for t in mem.iter().filter_map(|o| match o {
        Outcome::Tile(t) => Some(t),
        _ => None,
    }) {
        let (step_lat, step_lon) = (1.0 / 24.0 / t.rows as f64, 2.0 / 24.0 / t.columns as f64);
        for cell in (0..t.rows * t.columns).step_by(7) {
            let (col, row) = (cell / t.rows, cell % t.rows);
            let lat = t.base_lat() + (row as f64 + 0.5) * step_lat;
            let lon = t.base_lon() + (col as f64 + 0.5) * step_lon;
            let e = store
                .elevation(ElevationModel::Srtm3, lat, lon, false)
                .unwrap();
            assert_eq!(e, t.get_elevation(lat, lon), "{} ({row}, {col})", t.index);
            checked += 1;
        }
    }
    assert!(checked > 287 * 700, "{checked}");

    let at = |loc: &str, row: usize, col: usize| {
        let lat = maidenhead::lat_from_loc(loc, PositionInRectangle::BottomLeft).unwrap();
        let lon = maidenhead::lon_from_loc(loc, PositionInRectangle::BottomLeft).unwrap();
        let lat = lat + (row as f64 + 0.5) / 24.0 / 50.0;
        let lon = lon + (col as f64 + 0.5) * 2.0 / 24.0 / 100.0;
        store
            .elevation(ElevationModel::Srtm3, lat, lon, false)
            .unwrap()
    };
    // file row 1149, column 0: ((1149 * 31 + 0 * 17) % 5200) - 700
    assert_eq!(at("JO50MB", 0, 0), 3719);
    assert_eq!(at("JO50MA", 10, 10), TILE_MISSING);
    // all void, but written: all_void is not reset per tile (finding 3)
    assert_eq!(at("JO50NA", 10, 10), ELV_MISSING);
}

#[test]
fn existing_tiles_kept_unless_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let src = n50e011(tmp.path());
    let out = tmp.path().join("out");
    let mb = out.join("JO").join("50").join("JO50MB.loc");

    let v = ok_json(&tiles(&[&src], &out, &["--time", T1]));
    assert_eq!(v["written"], 287);
    let first = std::fs::read(&mb).unwrap();

    let v = ok_json(&tiles(&[&src], &out, &["--time", T2]));
    assert_eq!(
        (v["written"].clone(), v["kept"].clone()),
        (0.into(), 287.into())
    );
    assert_eq!(v["void"], 1);
    assert_eq!(std::fs::read(&mb).unwrap(), first);
    assert!(last_updated_line(&mb).contains(T1));

    let v = ok_json(&tiles(&[&src], &out, &["--time", T2, "--overwrite"]));
    assert_eq!(
        (v["written"].clone(), v["kept"].clone()),
        (287.into(), 0.into())
    );
    assert!(last_updated_line(&mb).contains(T2));
    no_part(&out);
}

#[test]
fn mixed_models_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let a = n50e011(tmp.path());
    let b = tmp.path().join("N50E010.hgt");
    sparse(&b, 3601 * 3601 * 2);
    let out = tmp.path().join("out");
    let o = tiles(&[&a, &b], &out, &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o)
            .contains("error: inputs mix SRTM3 and SRTM1: run once per model with its own --out"),
        "{}",
        stderr(&o)
    );
    assert!(locs(&out).is_empty());
}

#[test]
fn no_source_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let x = tmp.path().join("x.hgt");
    sparse(&x, SRTM3_LEN);
    let out = tmp.path().join("out");
    let o = tiles(&[&x], &out, &[]);
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("not an SRTM1, SRTM3 or GLOBE file:"), "{e}");
    assert!(e.contains("x.hgt"), "{e}");
    assert!(e.contains("error: no source file"), "{e}");
    assert!(!out.exists());
}

#[test]
fn directory_argument_expands_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("src");
    std::fs::create_dir(&dir).unwrap();
    let g = golden();
    Synth::from_case(source_case(&g, "S01W002.hgt")).write(&dir);
    n50e011(&dir);
    std::fs::write(dir.join("readme.txt"), "x").unwrap();
    let out = tmp.path().join("out");
    let o = tiles(&[&dir], &out, &["--time", T1]);
    let v = ok_json(&o);
    assert_eq!(v["sources"], 2);
    assert_eq!(v["written"], 287 + 288);
    let e = stderr(&o);
    let (n, s) = (
        e.find("N50E011.hgt:").unwrap(),
        e.find("S01W002.hgt:").unwrap(),
    );
    assert!(n < s, "{e}");
    assert!(e.contains("not an SRTM1, SRTM3 or GLOBE file:"), "{e}");
}

#[test]
fn squares_filter_globe() {
    let tmp = tempfile::tempdir().unwrap();
    // C12: the fill of square JO50 only, without voids
    let src = Synth::from_case(source_case(&golden(), "C10G"))
        .with_rects(vec![[4680, 4800, 1200, 1440]], vec![])
        .write(tmp.path());
    let out = tmp.path().join("out");
    let v = ok_json(&tiles(&[&src], &out, &["--squares", "jo50", "--time", T1]));
    assert_eq!(v["model"], "GLOBE");
    assert_eq!(v["written"], 576);
    let all = locs(&out);
    assert_eq!(all.len(), 576);
    let square = out.join("JO").join("50");
    assert!(all.iter().all(|p| p.parent() == Some(square.as_path())));
    no_part(&out);
}
