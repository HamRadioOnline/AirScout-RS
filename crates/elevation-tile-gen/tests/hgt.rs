//! The `compare`, `downsample` and `preview` commands against the C# `ElevationTileGenerator`
//! vectors (`tests/golden/elevation_tilegen.json`, synthetic sources).

mod common;

use as_elevation::{palette_color, ElevationError, ElevationTile};
use assert_cmd::Command;
use chrono::{DateTime, Utc};
use common::{fnv_bytes, golden, source_case, Synth};
use elevation_tile_gen::hgt::read_file;
use elevation_tile_gen::hgt::{diff_bytes, diff_stats, downsample, histogram_csv, DiffStats};
use elevation_tile_gen::preview::{read_tiles, square_rgba};
use elevation_tile_gen::source::identify;
use elevation_tile_gen::tiles::{srtm_tiles, Outcome};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

const SRTM1: usize = 3601;
const SRTM3: usize = 1201;
const VOID: i16 = -32768;

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

fn be(data: &[u8], side: usize, r: usize, c: usize) -> i16 {
    let at = (r * side + c) * 2;
    i16::from_be_bytes([data[at], data[at + 1]])
}

fn be_bytes(values: &[i16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn time(t: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(t).unwrap().with_timezone(&Utc)
}

fn golden_result(func: &str) -> Value {
    let g = golden();
    let r = g.by_fn(func).next().expect(func).result.clone();
    r
}

#[test]
fn downsample_matches_csharp() {
    let g = golden();
    let src = Synth::from_case(source_case(&g, "dsrc.hgt")).bytes();
    let r = golden_result("downsample");
    let out = downsample(&src).unwrap();
    assert_eq!(out.len() as i64, r["len"].as_i64().unwrap());
    assert_eq!(fnv_bytes(&out), r["fnv"]);
}

/// The 3 x 3 block of point `(i, j)`: its `i32` sum and whether a sample is void.
fn block(src: &[u8], i: usize, j: usize) -> (i32, bool) {
    let mut sum = 0i32;
    let mut void = false;
    for k in 0..3 {
        for l in 0..3 {
            let e = be(src, SRTM1, 3 * i + k, 3 * j + l);
            void |= e == VOID;
            sum += i32::from(e);
        }
    }
    (sum, void)
}

/// Owner decision S13: where C# is clean the port equals C#; an overflowing block gives the
/// `i32` sum / 9; a block with a void gives a void.
#[test]
fn downsample_deviation_follows_ruling() {
    let g = golden();
    let src = Synth::from_case(source_case(&g, "dsrc-ov.hgt")).bytes();
    let r = golden_result("downsample_deviation");
    let out = downsample(&src).unwrap();
    assert_eq!(out.len() as i64, r["len"].as_i64().unwrap());
    // the C# output has overflow and void points, so the port's bytes must differ
    assert_ne!(fnv_bytes(&out), r["fnv"]);

    // the classes of the golden counts, from the synthetic source
    let (mut clean, mut overflow, mut void) = (0, 0, 0);
    for i in 0..1200 {
        for j in 0..1200 {
            let (sum, has_void) = block(&src, i, j);
            let got = be(&out, SRTM3, i, j);
            if has_void {
                void += 1;
                assert_eq!(got, VOID, "void ({i}, {j})");
            } else {
                if i16::try_from(sum).is_ok() {
                    clean += 1;
                } else {
                    overflow += 1;
                }
                assert_eq!(i32::from(got), sum / 9, "({i}, {j})");
            }
        }
    }
    let counts = &r["counts"];
    assert_eq!(clean, counts["clean"].as_i64().unwrap());
    assert_eq!(overflow, counts["overflow"].as_i64().unwrap());
    assert_eq!(void, counts["void"].as_i64().unwrap());

    let mut seen = [0; 5];
    for c in r["cells"].as_array().unwrap() {
        let (i, j) = (
            c["row"].as_u64().unwrap() as usize,
            c["col"].as_u64().unwrap() as usize,
        );
        let csharp = c["csharp"].as_i64().unwrap();
        let got = i64::from(be(&out, SRTM3, i, j));
        match c["class"].as_str().unwrap() {
            "clean" | "raw" => {
                seen[0] += 1;
                assert_eq!(got, csharp, "{c}");
            }
            "overflow" => {
                seen[1] += 1;
                let (sum, _) = block(&src, i, j);
                assert_eq!(got, i64::from(sum / 9), "{c}");
                assert_ne!(got, csharp, "{c}");
            }
            "void" => {
                seen[2] += 1;
                assert_eq!(got, i64::from(VOID), "{c}");
                assert_ne!(got, csharp, "{c}");
            }
            "dup" => {
                seen[3] += 1;
                assert_eq!(got, i64::from(be(&out, SRTM3, i, 1199)), "{c}");
            }
            other => panic!("class {other}"),
        }
        if c["class"] == "raw" {
            seen[4] += 1;
            assert_eq!(got, i64::from(be(&src, SRTM1, 3600, 3 * j)), "{c}");
        }
    }
    assert!(seen[..4].iter().all(|&n| n > 0), "{seen:?}");
    // the raw row keeps the void of SRTM1 sample (3600, 0)
    assert_eq!(be(&out, SRTM3, 1200, 0), VOID);
}

#[test]
fn downsample_has_no_short_overflow() {
    let mut src = be_bytes(&vec![4000i16; SRTM1 * SRTM1]);
    let out = downsample(&src).unwrap();
    assert_eq!(out.len(), SRTM3 * SRTM3 * 2);
    for i in 0..SRTM3 {
        for j in 0..SRTM3 {
            assert_eq!(be(&out, SRTM3, i, j), 4000, "({i}, {j})");
        }
    }
    // one void in the block of (0, 0)
    src[(SRTM1 + 1) * 2..(SRTM1 + 1) * 2 + 2].copy_from_slice(&VOID.to_be_bytes());
    let out = downsample(&src).unwrap();
    assert_eq!(be(&out, SRTM3, 0, 0), VOID);
    assert_eq!(be(&out, SRTM3, 0, 1), 4000);
    assert_eq!(be(&out, SRTM3, 1, 0), 4000);

    let e = downsample(&src[..src.len() - 2]).unwrap_err();
    assert!(matches!(e, ElevationError::InvalidArgument(_)), "{e}");
}

#[test]
fn diff_matches_csharp() {
    let g = golden();
    let a = Synth::from_case(source_case(&g, "N50E011.hgt"));
    let b = Synth::from_case(source_case(&g, "cmp-b.hgt"));
    let (ab, bb) = (a.bytes(), b.bytes());
    let r = golden_result("compare");
    let diff = diff_bytes(&ab, &bb).unwrap();
    assert_eq!(diff.len() as i64, r["len"].as_i64().unwrap());
    assert_eq!(fnv_bytes(&diff), r["fnv"]);
    // inputs big-endian, output little-endian (`BinaryWriter`, `MainDlg.cs:1133`)
    let at = (600 * SRTM3 + 300) * 2;
    let want = a.value(600, 300).wrapping_sub(1000);
    assert_eq!(diff[at..at + 2], want.to_le_bytes());
    assert_ne!(want.to_le_bytes(), want.to_be_bytes());
}

#[test]
fn diff_stats_count_voids() {
    let a = be_bytes(&[10, VOID, 5, 7]);
    let b = be_bytes(&[10, 3, 2, -600]);
    let diff = diff_bytes(&a, &b).unwrap();
    let values: Vec<i16> = diff
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes(*p))
        .collect();
    assert_eq!(values, [0, VOID, 3, VOID]);
    let (stats, hist) = diff_stats(&diff);
    assert_eq!(
        stats,
        DiffStats {
            samples: 4,
            width: 2,
            differing: 1,
            voids: 2,
            min: Some(0),
            max: Some(3),
            mean: Some(1.5),
        }
    );
    assert_eq!(
        histogram_csv(&hist, 0, 3),
        "Difference[m];Count\n0;1\n1;0\n2;0\n3;1\n"
    );
    let (none, hist) = diff_stats(&VOID.to_le_bytes());
    assert_eq!(
        (none.voids, none.min, none.max, none.mean),
        (1, None, None, None)
    );
    assert!(hist.is_empty());
}

#[test]
fn compare_lengths_differ_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b, c, d) = (
        tmp.path().join("a.hgt"),
        tmp.path().join("b.hgt"),
        tmp.path().join("c.hgt"),
        tmp.path().join("d.hgt"),
    );
    std::fs::write(&a, [0u8; 4]).unwrap();
    std::fs::write(&b, [0u8; 6]).unwrap();
    std::fs::write(&c, [0u8; 5]).unwrap();
    std::fs::write(&d, [0u8; 5]).unwrap();
    let out = tmp.path().join("diff.hgtdiff");
    let err = failed(&run(&["compare", s(&a), s(&b), "--out", s(&out)]));
    assert!(err.contains("different length"), "{err}");
    let err = failed(&run(&["compare", s(&c), s(&d), "--out", s(&out)]));
    assert!(err.contains("odd length"), "{err}");
    assert!(!out.exists());

    // a good run writes the diff and the histogram and prints the stats
    std::fs::write(&a, be_bytes(&[10, VOID, 5, 7])).unwrap();
    std::fs::write(&b, be_bytes(&[10, 3, 2, -600])).unwrap();
    let csv = tmp.path().join("hist.csv");
    let v = ok_json(&run(&[
        "compare",
        s(&a),
        s(&b),
        "--out",
        s(&out),
        "--histogram",
        s(&csv),
    ]));
    assert_eq!(v["samples"], 4);
    assert_eq!(v["voids"], 2);
    assert_eq!(v["mean"], 1.5);
    assert_eq!(std::fs::read(&out).unwrap(), [0, 0, 0, 0x80, 3, 0, 0, 0x80]);
    assert_eq!(
        std::fs::read_to_string(&csv).unwrap(),
        "Difference[m];Count\n0;1\n1;0\n2;0\n3;1\n"
    );
}

/// I2: `compare` refuses an existing `--out` or `--histogram` unless `--overwrite`, as
/// `downsample` does. An input given as `--out` is never replaced without the flag.
#[test]
fn compare_refuses_existing_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.hgt"), tmp.path().join("b.hgt"));
    std::fs::write(&a, be_bytes(&[10, VOID, 5, 7])).unwrap();
    std::fs::write(&b, be_bytes(&[10, 3, 2, -600])).unwrap();
    let b_bytes = std::fs::read(&b).unwrap();
    let (out, csv) = (tmp.path().join("d.hgtdiff"), tmp.path().join("h.csv"));
    let diff = [0, 0, 0, 0x80, 3, 0, 0, 0x80];
    let cmd = |out: &Path, extra: &[&str]| {
        let mut v = vec!["compare", s(&a), s(&b), "--out", s(out)];
        v.extend_from_slice(extra);
        run(&v)
    };

    // an input as `--out` is refused and left as it was
    let err = failed(&cmd(&b, &[]));
    assert!(
        err.contains("exists: use --overwrite to replace it"),
        "{err}"
    );
    assert_eq!(std::fs::read(&b).unwrap(), b_bytes);

    // an existing `--out` is refused and keeps its bytes
    std::fs::write(&out, b"old").unwrap();
    let err = failed(&cmd(&out, &[]));
    assert!(err.contains("d.hgtdiff exists"), "{err}");
    assert_eq!(std::fs::read(&out).unwrap(), b"old");

    // an existing `--histogram` is refused before anything is written
    std::fs::remove_file(&out).unwrap();
    std::fs::write(&csv, b"old").unwrap();
    let err = failed(&cmd(&out, &["--histogram", s(&csv)]));
    assert!(err.contains("h.csv exists"), "{err}");
    assert!(!out.exists());
    assert_eq!(std::fs::read(&csv).unwrap(), b"old");

    // with `--overwrite` both are replaced
    std::fs::write(&out, b"old").unwrap();
    ok_json(&cmd(&out, &["--histogram", s(&csv), "--overwrite"]));
    assert_eq!(std::fs::read(&out).unwrap(), diff);
    assert!(std::fs::read_to_string(&csv)
        .unwrap()
        .starts_with("Difference[m];Count\n"));
    assert!(!tmp.path().join("d.hgtdiff.part").exists());
}

#[test]
fn downsample_cli_guards() {
    let g = golden();
    let tmp = tempfile::tempdir().unwrap();
    // the `dsrc` data under an SRTM1 name, so that it identifies
    let input = Synth::from_case(source_case(&g, "dsrc.hgt")).write_as(tmp.path(), "N50E010.hgt");
    let out = tmp.path().join("N50E010-3.hgt");
    ok_json(&run(&["downsample", s(&input), "--out", s(&out)]));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(bytes.len(), SRTM3 * SRTM3 * 2);
    assert_eq!(fnv_bytes(&bytes), golden_result("downsample")["fnv"]);
    assert!(!tmp.path().join("N50E010-3.hgt.part").exists());

    let err = failed(&run(&["downsample", s(&input), "--out", s(&out)]));
    assert!(err.contains("exists"), "{err}");
    ok_json(&run(&[
        "downsample",
        s(&input),
        "--out",
        s(&out),
        "--overwrite",
    ]));

    // an SRTM3 file is not an SRTM1 input
    let err = failed(&run(&[
        "downsample",
        s(&out),
        "--out",
        s(&tmp.path().join("x.hgt")),
    ]));
    assert!(err.contains("not an SRTM1 file"), "{err}");
    let srtm3 = tmp.path().join("N50E011.hgt");
    std::fs::copy(&out, &srtm3).unwrap();
    let err = failed(&run(&[
        "downsample",
        s(&srtm3),
        "--out",
        s(&tmp.path().join("y.hgt")),
    ]));
    assert!(err.contains("not an SRTM1 file"), "{err}");
    assert!(!tmp.path().join("x.hgt").exists() && !tmp.path().join("y.hgt").exists());
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    rgba[at..at + 4].try_into().unwrap()
}

fn colour(e: i16) -> [u8; 4] {
    let [r, g, b] = palette_color((f64::from(e) / 3000.0 * 100.0).clamp(0.0, 100.0));
    [r, g, b, 255]
}

/// The tiles of the golden `N50E011.hgt` source.
fn n50e011_tiles(dir: &Path) -> (Vec<ElevationTile>, std::path::PathBuf) {
    let g = golden();
    let path = Synth::from_case(source_case(&g, "N50E011.hgt")).write(dir);
    let src = identify(&path).unwrap().unwrap();
    let data = std::fs::read(&path).unwrap();
    let tiles = srtm_tiles(&src, &data, time("2026-11-24T12:00:00Z"), &|_| false)
        .unwrap()
        .into_iter()
        .filter_map(|o| match o {
            Outcome::Tile(t) => Some(t),
            _ => None,
        })
        .collect();
    (tiles, path)
}

#[test]
fn preview_places_tiles() {
    let tmp = tempfile::tempdir().unwrap();
    let (tiles, src) = n50e011_tiles(tmp.path());
    assert!(!tiles.iter().any(|t| t.index == "JO50MA"));
    let mb = tiles.iter().find(|t| t.index == "JO50MB").unwrap();
    // cell (k, l) is elv[k * rows + l]
    let cell = |k: usize, l: usize| mb.elv[k * mb.rows + l];
    assert_eq!(
        [cell(0, 0), cell(0, 49), cell(1, 49), cell(0, 48)],
        [3719, 2200, 2217, 2231]
    );

    let (w, h, rgba) = square_rgba(&tiles).unwrap();
    assert_eq!((w, h), (2400, 1200));
    assert_eq!(rgba.len(), 2400 * 1200 * 4);
    assert_eq!(pixel(&rgba, w, 1200, 1200 - 50 - 1), colour(3719));
    let p = [
        pixel(&rgba, w, 1200, 1100),
        pixel(&rgba, w, 1201, 1100),
        pixel(&rgba, w, 1200, 1101),
    ];
    assert_eq!(p, [colour(2200), colour(2217), colour(2231)]);
    assert!(p[0] != p[1] && p[1] != p[2] && p[0] != p[2], "{p:?}");
    // JO50MA (x 1200..1300, y 1150..1200) is missing: transparent
    for (x, y) in [(1200, 1199), (1299, 1150), (1250, 1175)] {
        assert_eq!(pixel(&rgba, w, x, y)[3], 0, "({x}, {y})");
    }
    // JO50AB..: west half missing
    assert_eq!(pixel(&rgba, w, 0, 0)[3], 0);

    // CLI on the zip of the Task 4 `zip` command
    let out = tmp.path().join("out");
    ok_json(&run(&[
        "tiles",
        s(&src),
        "--out",
        s(&out),
        "--time",
        "2026-11-24T12:00:00Z",
    ]));
    ok_json(&run(&["zip", "--out", s(&out)]));
    let zip = out.join("JO").join("JO50.zip");
    let png = tmp.path().join("JO50.png");
    let v = ok_json(&run(&["preview", s(&zip), "--out", s(&png)]));
    assert_eq!(
        (v["width"].clone(), v["height"].clone()),
        (2400.into(), 1200.into())
    );
    assert_eq!(v["tiles"], tiles.len());
    assert!(!tmp.path().join("JO50.png.part").exists());
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png).unwrap()));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (2400, 1200));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(&buf[..info.buffer_size()], &rgba[..]);
}

fn tile(index: &str, rows: usize, columns: usize) -> ElevationTile {
    ElevationTile::new(
        index,
        0,
        50.0,
        11.0,
        0,
        51.0,
        12.0,
        rows,
        columns,
        vec![100; rows * columns],
        time("2026-11-24T12:00:00Z"),
    )
    .unwrap()
}

#[test]
fn preview_rejects_other_shapes_and_squares() {
    let bad = |tiles: &[ElevationTile], what: &str| {
        let e = square_rgba(tiles).unwrap_err();
        assert!(
            matches!(&e, ElevationError::InvalidArgument(m) if m.contains(what)),
            "{e}"
        );
    };
    bad(&[], "no tile");
    bad(&[tile("JO50MB", 1, 1000)], "JO50MB");
    bad(&[tile("JO50MB", 100, 50)], "JO50MB");
    bad(&[tile("JO50", 50, 100)], "JO50");
    bad(
        &[tile("JO50MB", 50, 100), tile("JO51MB", 50, 100)],
        "JO51MB",
    );
    bad(
        &[tile("JO50MB", 50, 100), tile("JO50MC", 150, 300)],
        "JO50MC",
    );
    for (rows, columns, w, h) in [
        (5, 10, 240, 120),
        (50, 100, 2400, 1200),
        (150, 300, 7200, 3600),
    ] {
        let (gw, gh, _) = square_rgba(&[tile("JO50XX", rows, columns)]).unwrap();
        assert_eq!((gw, gh), (w, h));
    }

    // a 1 x 1000 tile in a zip: exit 1, no PNG
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("JO50.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    z.start_file("JO50MB.loc", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(tile("JO50MB", 1, 1000).to_loc_json().as_bytes())
        .unwrap();
    z.finish().unwrap();
    let png = tmp.path().join("JO50.png");
    let err = failed(&run(&["preview", s(&zip_path), "--out", s(&png)]));
    assert!(err.contains("JO50MB"), "{err}");
    assert!(!png.exists());
    assert!(!tmp.path().join("JO50.png.part").exists());

    // a missing input is an error that names it
    let missing = tmp.path().join("JO51.zip");
    let err = failed(&run(&["preview", s(&missing), "--out", s(&png)]));
    assert!(err.contains("JO51.zip"), "{err}");
    assert!(!png.exists());
}

#[test]
fn preview_reads_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, src) = n50e011_tiles(tmp.path());
    let out = tmp.path().join("out");
    ok_json(&run(&[
        "tiles",
        s(&src),
        "--out",
        s(&out),
        "--time",
        "2026-11-24T12:00:00Z",
    ]));
    let png = tmp.path().join("p.png");
    let v = ok_json(&run(&[
        "preview",
        s(&out.join("JO").join("50")),
        "--out",
        s(&png),
    ]));
    assert_eq!(v["width"], 2400);
    assert!(png.exists());

    // I2: an existing `--out` is refused unless `--overwrite`, also the square zip itself
    let dir = out.join("JO").join("50");
    std::fs::write(&png, b"old").unwrap();
    let err = failed(&run(&["preview", s(&dir), "--out", s(&png)]));
    assert!(
        err.contains("p.png exists: use --overwrite to replace it"),
        "{err}"
    );
    assert_eq!(std::fs::read(&png).unwrap(), b"old");
    ok_json(&run(&["preview", s(&dir), "--out", s(&png), "--overwrite"]));
    assert_ne!(std::fs::read(&png).unwrap(), b"old");
    ok_json(&run(&["zip", "--out", s(&out)]));
    let zip = out.join("JO").join("JO50.zip");
    let zip_bytes = std::fs::read(&zip).unwrap();
    let err = failed(&run(&["preview", s(&zip), "--out", s(&zip)]));
    assert!(err.contains("exists"), "{err}");
    assert_eq!(std::fs::read(&zip).unwrap(), zip_bytes);
}

/// A zip of the given `.loc` entries.
fn zip_of(path: &Path, tiles: &[(&str, ElevationTile)]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, t) in tiles {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(t.to_loc_json().as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

/// `read_tiles` stops at the first tile of another square or shape, so it never holds more
/// than one square of tiles.
#[test]
fn read_tiles_stops_at_another_square() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = |entries: &[(&str, ElevationTile)], what: &str| {
        let path = tmp.path().join("x.zip");
        zip_of(&path, entries);
        let e = read_tiles(&path).unwrap_err().to_string();
        assert!(e.contains(what), "{e}");
    };
    bad(
        &[
            ("JO50MB.loc", tile("JO50MB", 50, 100)),
            ("JO51MB.loc", tile("JO51MB", 50, 100)),
        ],
        "JO51MB.loc",
    );
    bad(
        &[
            ("JO50MB.loc", tile("JO50MB", 50, 100)),
            ("JO50MC.loc", tile("JO50MC", 150, 300)),
        ],
        "JO50MC.loc",
    );
    bad(
        &[
            ("JO50MB.loc", tile("JO50MB", 50, 100)),
            ("b/JO50MB.loc", tile("JO50MB", 50, 100)),
        ],
        "appears twice",
    );
    let path = tmp.path().join("ok.zip");
    zip_of(
        &path,
        &[
            ("JO50MB.loc", tile("JO50MB", 50, 100)),
            ("JO50MC.loc", tile("JO50MC", 50, 100)),
        ],
    );
    assert_eq!(read_tiles(&path).unwrap().len(), 2);
}

/// A file above the largest source file is refused before it is read.
#[test]
fn read_file_refuses_more_than_a_source_file() {
    let tmp = tempfile::tempdir().unwrap();
    let max = 6000 * 10800 * 2;
    let big = tmp.path().join("big.hgt");
    common::sparse(&big, max + 1);
    let e = read_file(&big).unwrap_err();
    assert!(
        matches!(&e, ElevationError::InvalidArgument(m) if m.contains("larger than")),
        "{e}"
    );
    let err = failed(&run(&[
        "compare",
        s(&big),
        s(&big),
        "--out",
        s(&tmp.path().join("d")),
    ]));
    assert!(err.contains("larger than"), "{err}");
    let ok = tmp.path().join("ok.hgt");
    common::sparse(&ok, 4);
    assert_eq!(read_file(&ok).unwrap().len(), 4);
}
