//! `identify` and the `elevation-tile-gen tiles` command.

mod common;

use as_elevation::{ElevationModel, ElevationStore, ElevationTile, ELV_MISSING};
use as_geo::maidenhead::{self, PositionInRectangle};
use assert_cmd::Command;
use chrono::{DateTime, Utc};
use common::{files_under, golden, source_case, sparse, Synth};
use elevation_tile_gen::source::{identify, Source, SourceKind};
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
    assert_eq!(v["written"], 288);
    assert_eq!(v["void"], 0);
    assert_eq!(v["kept"], 0);
    assert!(
        stderr(&o).contains("N50E011.hgt: 288 written, 0 void, 0 kept"),
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
    // JO50MA is now written (its samples shift one cell south under #294, so the tile no
    // longer lines up with the void band).
    assert!(out.join("JO").join("50").join("JO50MA.loc").exists());
    assert_eq!(locs(&out).len(), 288);
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
    assert_eq!(parsed.len(), 288);
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
    assert!(checked > 288 * 700, "{checked}");

    let at = |loc: &str, row: usize, col: usize| {
        let lat = maidenhead::lat_from_loc(loc, PositionInRectangle::BottomLeft).unwrap();
        let lon = maidenhead::lon_from_loc(loc, PositionInRectangle::BottomLeft).unwrap();
        let lat = lat + (row as f64 + 0.5) / 24.0 / 50.0;
        let lon = lon + (col as f64 + 0.5) * 2.0 / 24.0 / 100.0;
        store
            .elevation(ElevationModel::Srtm3, lat, lon, false)
            .unwrap()
    };
    // The one-cell-south fix (#294) shifts every sample down a row. JO50MA (0,0) is now
    // written and its bottom-left cell reads the file's bottom row 1200, col 0:
    // ((1200 * 31 + 0 * 17) % 5200) - 700 = 100.
    assert_eq!(at("JO50MA", 0, 0), 100);
    // JO50MB (0,0) reads file row 1150, col 0 — the top of the void band (rows 1150..1199).
    assert_eq!(at("JO50MB", 0, 0), ELV_MISSING);
    // JO50NA (0,0) reads file row 1200, col 100: ((1200 * 31 + 100 * 17) % 5200) - 700 = 1800.
    assert_eq!(at("JO50NA", 0, 0), 1800);
}

#[test]
fn existing_tiles_kept_unless_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let src = n50e011(tmp.path());
    let out = tmp.path().join("out");
    let mb = out.join("JO").join("50").join("JO50MB.loc");

    let v = ok_json(&tiles(&[&src], &out, &["--time", T1]));
    assert_eq!(v["written"], 288);
    let first = std::fs::read(&mb).unwrap();

    let v = ok_json(&tiles(&[&src], &out, &["--time", T2]));
    assert_eq!(
        (v["written"].clone(), v["kept"].clone()),
        (0.into(), 288.into())
    );
    assert_eq!(v["void"], 0);
    assert_eq!(std::fs::read(&mb).unwrap(), first);
    assert!(last_updated_line(&mb).contains(T1));

    let v = ok_json(&tiles(&[&src], &out, &["--time", T2, "--overwrite"]));
    assert_eq!(
        (v["written"].clone(), v["kept"].clone()),
        (288.into(), 0.into())
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
    assert_eq!(v["written"], 288 + 288);
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

/// A tiny SRTM3 source: 48 x 24 samples → 2 x 2 samples per tile, 12 x 24 = 288 tiles.
/// `sample(r, c)` encodes the file position so an off-by-one row is caught.
fn tiny_srtm(tmp: &Path) -> (Source, Vec<u8>) {
    let (rows, cols) = (48usize, 24usize);
    let mut data = vec![0u8; rows * cols * 2];
    for r in 0..rows {
        for c in 0..cols {
            let v = ((r * 100 + c) as i64) as i16;
            data[(r * cols + c) * 2..][..2].copy_from_slice(&v.to_be_bytes());
        }
    }
    let src = Source {
        path: tmp.join("N50E011.hgt"),
        kind: SourceKind::Srtm3,
        lat_base: 50,
        lon_base: 11,
        rows,
        cols,
    };
    (src, data)
}

/// #294: an SRTM tile's sample at cell `(k, l)` reads the file row `frows - 1 - j*rows - l`
/// (matching GLOBE), not `frows - 2 - …`, which is one sample north. Every expected sample is
/// hand-derived from the fill formula; the file's bottom row (`frows - 1`, read only after the
/// fix) is the one-cell-south edge case.
#[test]
fn srtm_sample_reads_the_cell_south_row() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, data) = tiny_srtm(tmp.path());
    let (rows, cols) = (src.rows, src.cols);
    let out = srtm_tiles(&src, &data, time(T1), &|_| false).unwrap();
    assert_eq!(out.len(), 288);
    let (tile_rows, tile_cols) = (rows / 24, cols / 12);
    for (t, o) in out.iter().enumerate() {
        let Outcome::Tile(tile) = o else {
            panic!("tile {t} unexpectedly void");
        };
        assert_eq!((tile.rows, tile.columns), (tile_rows, tile_cols));
        let (i, j) = ((t / 24) as i64, (t % 24) as i64);
        for k in 0..tile_cols {
            for l in 0..tile_rows {
                let r = rows as i64 - 1 - j * tile_rows as i64 - l as i64;
                let c = i * tile_cols as i64 + k as i64;
                let want = (r * 100 + c) as i16;
                assert_eq!(
                    tile.elv[k * tile_rows + l],
                    want,
                    "tile {t} ({i},{j}) cell ({k},{l})"
                );
            }
        }
    }
}

/// #293: `all_void` is reset per SRTM tile, so an all-void tile that follows a data tile is
/// skipped (`Outcome::Void`) instead of being written with a bogus `MinElv`/`MaxElv`.
#[test]
fn srtm_void_tile_after_data_tile_is_not_written() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, mut data) = tiny_srtm(tmp.path());
    let (rows, cols) = (src.rows, src.cols);
    let (tile_rows, tile_cols) = (rows / 24, cols / 12);
    // Overwrite the cells of tiles (0, 0) and (0, 2) with the void flag; tile (0, 1) keeps
    // data. Tile (i, j) reads rows [frows - 1 - (j + 1) * tile_rows + 1, frows - 1 - j * tile_rows]
    // and columns [i * tile_cols, (i + 1) * tile_cols).
    let set_void = |data: &mut Vec<u8>, i: usize, j: usize| {
        for l in 0..tile_rows {
            for k in 0..tile_cols {
                let r = rows as i64 - 1 - j as i64 * tile_rows as i64 - l as i64;
                let c = i as i64 * tile_cols as i64 + k as i64;
                let at = (r * cols as i64 + c) as usize * 2;
                data[at..at + 2].copy_from_slice(&ELV_MISSING.to_be_bytes());
            }
        }
    };
    set_void(&mut data, 0, 0);
    set_void(&mut data, 0, 2);
    let out = srtm_tiles(&src, &data, time(T1), &|_| false).unwrap();
    // outcome order: i outer, j inner → index i * 24 + j
    assert!(matches!(out[0], Outcome::Void(_)), "first tile is all void");
    assert!(matches!(out[1], Outcome::Tile(_)), "second tile has data");
    assert!(
        matches!(out[2], Outcome::Void(_)),
        "an all-void tile after a data tile must be skipped, not written"
    );
}
