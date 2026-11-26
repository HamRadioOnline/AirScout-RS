//! `maptile-gen` end to end: export, import and combine on temporary files with tiny synthetic
//! PNGs.

use as_maps::{TileKey, TileStore, DBID_OPENTOPOMAP, DBID_OSM, MAX_TILE_BYTES};
use assert_cmd::Command;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

fn bin() -> Command {
    Command::cargo_bin("maptile-gen").unwrap()
}

fn key(x: u32, y: u32, zoom: u8, db_id: i32) -> TileKey {
    TileKey { x, y, zoom, db_id }
}

/// A local time in July (no DST change near it).
fn local(h: u32, m: u32, s: u32, nano: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2019, 7, 14)
        .unwrap()
        .and_hms_nano_opt(h, m, s, nano)
        .unwrap()
}

/// Unix seconds of a local time.
fn local_secs(t: NaiveDateTime) -> u64 {
    Local
        .from_local_datetime(&t)
        .single()
        .unwrap()
        .timestamp()
        .try_into()
        .unwrap()
}

fn mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn set_mtime(path: &Path, secs: u64) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

fn encode(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(color);
    enc.set_depth(depth);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(data).unwrap();
    wr.finish().unwrap();
    out
}

/// A 1 x 1 RGB PNG of one colour.
fn tile_png(rgb: [u8; 3]) -> Vec<u8> {
    encode(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &rgb)
}

/// Decoded as 8-bit RGB(A): width, height, channels, pixels.
fn decode(path: &Path) -> (u32, u32, usize, Vec<u8>) {
    let mut d = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, info.color_type.samples(), buf)
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn all_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(all_files(&p));
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

#[test]
fn export_writes_zxy_tree_with_cache_time() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    let (a, b) = (tile_png([1, 2, 3]), tile_png([4, 5, 6]));
    let (ta, tb) = (local(10, 11, 12, 500_000_000), local(13, 14, 15, 0));
    let store = TileStore::open(&db).unwrap();
    store
        .insert(key(1100, 700, 11, DBID_OSM), Some(ta), &a)
        .unwrap();
    store
        .insert(key(1101, 700, 11, DBID_OSM), Some(tb), &b)
        .unwrap();
    drop(store);

    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let fa = out.join("11").join("1100").join("700.png");
    let fb = out.join("11").join("1101").join("700.png");
    assert_eq!(all_files(&out), vec![fa.clone(), fb.clone()]);
    assert_eq!(fs::read(&fa).unwrap(), a);
    assert_eq!(fs::read(&fb).unwrap(), b);
    assert_eq!(mtime_secs(&fa), local_secs(ta));
    assert_eq!(mtime_secs(&fb), local_secs(tb));
    let text = stdout(&o);
    assert!(
        text.contains(&format!("Saving [1 of 2] {}", fa.display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("Saving [2 of 2] {}", fb.display())),
        "{text}"
    );
    assert!(text.contains("written 2, unchanged 0, skipped 0"), "{text}");
    assert_eq!(text.lines().last(), Some("Ready."));
}

#[test]
fn export_skips_same_time_and_rewrites_other_time() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    let (a, b) = (tile_png([1, 2, 3]), tile_png([4, 5, 6]));
    let (ta, tb) = (local(10, 11, 12, 700_000_000), local(13, 14, 15, 0));
    let store = TileStore::open(&db).unwrap();
    store.insert(key(3, 4, 5, DBID_OSM), Some(ta), &a).unwrap();
    store.insert(key(3, 5, 5, DBID_OSM), Some(tb), &b).unwrap();
    drop(store);
    let fa = out.join("5").join("3").join("4.png");
    let fb = out.join("5").join("3").join("5.png");
    fs::create_dir_all(fa.parent().unwrap()).unwrap();
    fs::write(&fa, b"old a").unwrap();
    fs::write(&fb, b"old b").unwrap();
    // The same time to the whole second (the file system may keep less than the 0.7 s).
    set_mtime(&fa, local_secs(ta));
    set_mtime(&fb, local_secs(tb) - 3600);

    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(fs::read(&fa).unwrap(), b"old a");
    assert_eq!(mtime_secs(&fa), local_secs(ta));
    assert_eq!(fs::read(&fb).unwrap(), b);
    assert_eq!(mtime_secs(&fb), local_secs(tb));
    assert!(stdout(&o).contains("written 1, unchanged 1, skipped 0"));
    assert_eq!(all_files(&out), vec![fa, fb]);
}

#[test]
fn export_skips_tile_without_data() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    let a = tile_png([1, 2, 3]);
    let store = TileStore::open(&db).unwrap();
    store
        .insert(key(1, 1, 3, DBID_OSM), Some(local(9, 0, 0, 0)), &a)
        .unwrap();
    // No CacheTime: written, the mtime is left as the file system sets it.
    store.insert(key(2, 1, 3, DBID_OSM), None, &a).unwrap();
    drop(store);
    let c = rusqlite::Connection::open(&db).unwrap();
    // A Tiles row without a TilesData row, and one that is no tile key.
    c.execute_batch(&format!(
        "INSERT INTO Tiles (X, Y, Zoom, Type, CacheTime) VALUES (5, 6, 7, {DBID_OSM}, NULL);
         INSERT INTO Tiles (X, Y, Zoom, Type, CacheTime) VALUES (-1, 6, 7, {DBID_OSM}, NULL);"
    ))
    .unwrap();
    drop(c);

    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let f1 = out.join("3").join("1").join("1.png");
    let f2 = out.join("3").join("2").join("1.png");
    assert_eq!(all_files(&out), vec![f1, f2.clone()]);
    assert_eq!(fs::read(&f2).unwrap(), a);
    let text = stdout(&o);
    assert!(text.contains("no data"), "{text}");
    assert!(text.contains("written 2, unchanged 0, skipped 2"), "{text}");
    assert!(text.contains("1 row is no tile"), "{text}");
    assert_eq!(text.lines().last(), Some("Ready."));

    // Again: the tile without CacheTime is written again, the other one is unchanged.
    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(stdout(&o).contains("written 1, unchanged 1, skipped 2"));
}

fn mixed_db(path: &Path) {
    let store = TileStore::open(path).unwrap();
    let t = Some(local(8, 0, 0, 0));
    store
        .insert(key(1, 1, 2, DBID_OSM), t, &tile_png([1, 1, 1]))
        .unwrap();
    store
        .insert(key(2, 1, 2, DBID_OSM), t, &tile_png([2, 2, 2]))
        .unwrap();
    store
        .insert(key(1, 1, 2, DBID_OPENTOPOMAP), t, &tile_png([3, 3, 3]))
        .unwrap();
}

#[test]
fn export_refuses_mixed_types_without_flag() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    mixed_db(&db);
    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(!o.status.success());
    let err = stderr(&o);
    assert!(err.contains("--type"), "{err}");
    assert!(err.contains("1138496160 (OpenTopoMap): 1 tile"), "{err}");
    assert!(err.contains("1492776782 (OpenStreetMap): 2 tiles"), "{err}");
    assert!(!out.exists());
}

/// A foreign `CacheTime` that is no file time (after year 9999, before 1601, where Windows
/// `SystemTime` ends): the file is written without setting its mtime, never a panic.
#[test]
fn export_cache_time_out_of_range() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    let a = tile_png([1, 2, 3]);
    let store = TileStore::open(&db).unwrap();
    store.insert(key(1, 1, 3, DBID_OSM), None, &a).unwrap();
    store.insert(key(2, 1, 3, DBID_OSM), None, &a).unwrap();
    drop(store);
    let c = rusqlite::Connection::open(&db).unwrap();
    // Julian day 13000000.0 is about year 30900.
    c.execute_batch(
        "UPDATE Tiles SET CacheTime = 13000000.0 WHERE X = 1;
         UPDATE Tiles SET CacheTime = '1500-01-01 00:00:00' WHERE X = 2;",
    )
    .unwrap();
    drop(c);
    // The store reads them as no time (as-maps gate); `file_time` has its own unit test.
    let list = TileStore::open_read_only(&db).unwrap().list().unwrap();
    assert!(list.iter().all(|t| t.cache_time.is_none()), "{list:?}");

    let o = bin()
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(text.contains("written 2, unchanged 0, skipped 0"), "{text}");
    for x in ["1", "2"] {
        let f = out.join("3").join(x).join("1.png");
        assert_eq!(fs::read(&f).unwrap(), a);
        // Not dated to the cache time: the mtime is from this run.
        assert!(mtime_secs(&f) > local_secs(local(0, 0, 0, 0)));
    }
}

/// Julian day 97466824.49 is in the last hours of chrono's last year; west of UTC its local
/// time plus one hour (the DST-gap fallback) would overflow. Export must not panic.
#[test]
fn export_cache_time_at_the_end_of_chrono_west_of_utc() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    let out = dir.path().join("tree");
    let a = tile_png([1, 2, 3]);
    TileStore::open(&db)
        .unwrap()
        .insert(key(1, 1, 3, DBID_OSM), None, &a)
        .unwrap();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("UPDATE Tiles SET CacheTime = 97466824.49")
        .unwrap();
    let o = bin()
        .env("TZ", "America/New_York")
        .args(["export", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("written 1, unchanged 0, skipped 0"));
    assert_eq!(fs::read(out.join("3").join("1").join("1.png")).unwrap(), a);
}

#[test]
fn export_type_filter() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("maptiles.gmdb");
    mixed_db(&db);
    let topo = dir.path().join("topo");
    let o = bin()
        .args(["export", "--type", "OpenTopoMap", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&topo)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let f = topo.join("2").join("1").join("1.png");
    assert_eq!(all_files(&topo), vec![f.clone()]);
    assert_eq!(fs::read(&f).unwrap(), tile_png([3, 3, 3]));

    let osm = dir.path().join("osm");
    let o = bin()
        .args(["export", "--type", "1492776782", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(&osm)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(all_files(&osm).len(), 2);
    assert_eq!(
        fs::read(osm.join("2").join("1").join("1.png")).unwrap(),
        tile_png([1, 1, 1])
    );

    let o = bin()
        .args(["export", "--type", "NoSuchMap", "--db"])
        .arg(&db)
        .arg("--out")
        .arg(dir.path().join("none"))
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("OpenStreetMap"), "{}", stderr(&o));
}

#[test]
fn import_gmdb_then_export_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.gmdb");
    let dst = dir.path().join("dst.gmdb");
    let (a, b, c) = (
        tile_png([1, 2, 3]),
        tile_png([4, 5, 6]),
        tile_png([7, 8, 9]),
    );
    let (ta, tb) = (local(10, 11, 12, 123_456_700), local(13, 14, 15, 0));
    let store = TileStore::open(&src).unwrap();
    store.insert(key(1, 2, 4, DBID_OSM), Some(ta), &a).unwrap();
    store.insert(key(2, 2, 4, DBID_OSM), Some(tb), &b).unwrap();
    store.insert(key(3, 2, 4, DBID_OSM), None, &c).unwrap();
    store
        .insert(key(4, 2, 4, DBID_OSM), None, b"<html>404</html>")
        .unwrap();
    store
        .insert(key(1, 2, 4, DBID_OPENTOPOMAP), Some(ta), &c)
        .unwrap();
    // Keys no tile can have: zoom above 30, x outside 2^z.
    store
        .insert(key(4_000_000_000, 0, 200, DBID_OSM), None, &a)
        .unwrap();
    store.insert(key(16, 0, 4, DBID_OSM), None, &a).unwrap();
    let src_list: Vec<_> = store
        .list()
        .unwrap()
        .into_iter()
        .filter(|t| t.key.db_id == DBID_OSM && t.key.x < 4 && t.key.zoom == 4)
        .map(|t| (t.key, t.cache_time))
        .collect();
    drop(store);

    let import = || {
        bin()
            .args(["import", "--type", "OpenStreetMap", "--db"])
            .arg(&dst)
            .arg(&src)
            .output()
            .unwrap()
    };
    let o = import();
    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(text.contains("imported 3, kept 0, skipped 3"), "{text}");
    assert!(text.contains("not a PNG or JPEG"), "{text}");
    assert!(text.contains("zoom above 30"), "{text}");
    assert!(text.contains("x is not below 2^4"), "{text}");
    let o = import();
    assert!(stdout(&o).contains("imported 0, kept 3, skipped 3"));

    let store = TileStore::open_read_only(&dst).unwrap();
    let dst_list: Vec<_> = store
        .list()
        .unwrap()
        .into_iter()
        .map(|t| (t.key, t.cache_time))
        .collect();
    assert_eq!(dst_list, src_list);
    drop(store);

    let out = dir.path().join("tree");
    let o = bin()
        .args(["export", "--db"])
        .arg(&dst)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let fa = out.join("4").join("1").join("2.png");
    assert_eq!(fs::read(&fa).unwrap(), a);
    assert_eq!(mtime_secs(&fa), local_secs(ta));
    assert_eq!(fs::read(out.join("4").join("2").join("2.png")).unwrap(), b);
    assert_eq!(fs::read(out.join("4").join("3").join("2.png")).unwrap(), c);

    // A source that is not a GMap database is an error and creates no target.
    let foreign = dir.path().join("foreign.gmdb");
    fs::write(&foreign, b"not sqlite").unwrap();
    let fresh = dir.path().join("fresh.gmdb");
    let o = bin()
        .args(["import", "--db"])
        .arg(&fresh)
        .arg(&foreign)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("not a GMap tile database"),
        "{}",
        stderr(&o)
    );
    assert!(!fresh.exists());
}

#[test]
fn import_dir_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    let db = dir.path().join("maptiles.gmdb");
    let write = |rel: &[&str], bytes: &[u8]| {
        let p = rel.iter().fold(root.clone(), |p, s| p.join(s));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        p
    };
    let a = tile_png([1, 2, 3]);
    let b = tile_png([4, 5, 6]);
    let fa = write(&["11", "1100", "700.png"], &a);
    write(&["12", "5", "6.png"], &b);
    let ta = local(10, 11, 12, 0);
    set_mtime(&fa, local_secs(ta));
    // Skipped: over MAX_TILE_BYTES, names that are no numbers, not an image, zoom above 30,
    // x and y outside 2^z.
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(MAX_TILE_BYTES + 1, 0);
    write(&["11", "1100", "702.png"], &big);
    write(&["a", "b", "c.png"], &a);
    write(&["11", "1100", "703.png"], b"<html>404</html>");
    write(&["31", "0", "0.png"], &a);
    write(&["2", "4", "0.png"], &a);
    write(&["2", "0", "4.png"], &a);

    let import = || {
        bin()
            .args(["import", "--db"])
            .arg(&db)
            .arg(&root)
            .output()
            .unwrap()
    };
    let o = import();
    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(text.contains("imported 2, kept 0, skipped 6"), "{text}");
    for name in ["702.png", "a", "703.png", "31", "4", "4.png"] {
        assert!(
            text.lines()
                .any(|l| l.starts_with("skipped ") && l.contains(name)),
            "{name}: {text}"
        );
    }
    let store = TileStore::open_read_only(&db).unwrap();
    assert_eq!(store.types().unwrap(), vec![(DBID_OSM, 2)]);
    let k = key(1100, 700, 11, DBID_OSM);
    assert_eq!(store.get(k).unwrap(), Some(a));
    let info = store
        .list()
        .unwrap()
        .into_iter()
        .find(|t| t.key == k)
        .unwrap();
    assert_eq!(local_secs(info.cache_time.unwrap()), local_secs(ta));
    assert_eq!(store.get(key(5, 6, 12, DBID_OSM)).unwrap(), Some(b));
    drop(store);

    let o = import();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("imported 0, kept 2, skipped 6"));
}

#[test]
fn combine_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("zoom");
    fs::create_dir_all(src.join("sub.png")).unwrap();
    fs::write(src.join("sub.png").join("x.png"), tile_png([0, 0, 0])).unwrap();
    fs::write(src.join("notes.txt"), b"not a png").unwrap();
    // a.png: 4 x 5 RGB, blue.
    let a: Vec<u8> = [0u8, 0, 255].repeat(4 * 5);
    fs::write(
        src.join("a.png"),
        encode(4, 5, png::ColorType::Rgb, png::BitDepth::Eight, &a),
    )
    .unwrap();
    // b.png: 2 x 3 RGBA, 16 bit, red; its first pixel is transparent.
    let mut b: Vec<u8> = [0xffu8, 0xff, 0, 0, 0, 0, 0xff, 0xff].repeat(2 * 3);
    b[6..8].copy_from_slice(&[0, 0]);
    fs::write(
        src.join("b.png"),
        encode(2, 3, png::ColorType::Rgba, png::BitDepth::Sixteen, &b),
    )
    .unwrap();
    let out = dir.path().join("zoom-combined.png");

    let o = bin()
        .arg("combine")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!dir.path().join("zoom-combined.png.part").exists());
    let (w, h, ch, px) = decode(&out);
    assert_eq!((w, h), (6, 5));
    let at = |x: usize, y: usize| px[(y * 6 + x) * ch..][..3].to_vec();
    const GREY: [u8; 3] = [171, 171, 171];
    for y in 0..5 {
        for x in 0..4 {
            assert_eq!(at(x, y), [0, 0, 255], "({x}, {y})");
        }
    }
    assert_eq!(at(4, 0), GREY);
    assert_eq!(at(5, 0), [255, 0, 0]);
    assert_eq!(at(4, 2), [255, 0, 0]);
    for (x, y) in [(4, 3), (5, 3), (4, 4), (5, 4)] {
        assert_eq!(at(x, y), GREY, "({x}, {y})");
    }
    if ch == 4 {
        assert!(px.chunks(4).all(|p| p[3] == 255));
    }
}

#[test]
fn combine_refuses_existing_out() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("zoom");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.png"), tile_png([1, 2, 3])).unwrap();
    let out = dir.path().join("out.png");
    fs::write(&out, b"keep").unwrap();
    let run = |overwrite: bool| {
        let mut c = bin();
        c.arg("combine").arg(&src).arg("--out").arg(&out);
        if overwrite {
            c.arg("--overwrite");
        }
        c.output().unwrap()
    };
    let o = run(false);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("--overwrite"), "{}", stderr(&o));
    assert_eq!(fs::read(&out).unwrap(), b"keep");
    let o = run(true);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(decode(&out).3, vec![1, 2, 3]);
}

#[test]
fn combine_refuses_a_dir_without_png() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("empty");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("notes.txt"), b"no png").unwrap();
    let out = dir.path().join("out.png");
    let o = bin()
        .arg("combine")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no PNG files"), "{}", stderr(&o));
    assert!(!out.exists());
}
