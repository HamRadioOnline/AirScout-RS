//! The tile generator against the C# `ElevationTileGenerator` vectors
//! (`tests/golden/elevation_tilegen.json`, synthetic sources).

mod common;

use as_elevation::ElevationTile;
use as_golden::Case;
use chrono::{DateTime, Utc};
use common::{fnv, golden, source_case, Synth};
use elevation_tile_gen::source::{identify, Source};
use elevation_tile_gen::tiles::{globe_indices, globe_tile, globe_tile_index, srtm_tiles, Outcome};
use serde_json::Value;

/// The member names of a `.loc` text (lines `  "Name": ...` at two-space indent).
fn members(json: &str) -> Vec<String> {
    json.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("  \"")?;
            let end = rest.find("\": ")?;
            let name = &rest[..end];
            (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then(|| name.to_string())
        })
        .collect()
}

fn bits(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

/// One written tile against its golden case.
fn assert_tile(t: &ElevationTile, c: &Case) {
    let r = &c.result;
    let ctx = format!("{} {}", c.s("source"), c.s("tile"));
    assert_eq!(t.index, c.s("tile"), "{ctx}");
    for (key, x) in [
        ("min_lat", t.min_lat),
        ("min_lon", t.min_lon),
        ("max_lat", t.max_lat),
        ("max_lon", t.max_lon),
    ] {
        assert_eq!(bits(x), r[format!("{key}_bits")], "{ctx} {key}");
    }
    assert_eq!(i64::from(t.min_elv), r["min_elv"], "{ctx} min_elv");
    assert_eq!(i64::from(t.max_elv), r["max_elv"], "{ctx} max_elv");
    assert_eq!(t.rows as i64, r["rows"], "{ctx} rows");
    assert_eq!(t.columns as i64, r["columns"], "{ctx} columns");
    assert_eq!(t.elv.len() as i64, r["elv_len"], "{ctx} elv_len");
    assert_eq!(fnv(&t.elv), r["elv_fnv"], "{ctx} elv_fnv");
    let json = t.to_loc_json();
    let expected: Vec<String> = r["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .filter(|m| m != "Bounds")
        .collect();
    assert_eq!(members(&json), expected, "{ctx} members");
    let last = r["last_updated"].as_str().unwrap();
    assert!(
        json.contains(&format!("  \"LastUpdated\": \"{last}\"\r\n")),
        "{ctx} LastUpdated"
    );
}

/// The golden `last_updated` (the same for every written case).
fn golden_now(cases: &[&Case]) -> DateTime<Utc> {
    let s = cases
        .iter()
        .find_map(|c| c.result.get("last_updated").and_then(Value::as_str))
        .expect("a written case");
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// The tile cases of one source, in golden (C# loop) order.
fn tile_cases<'a>(g: &'a as_golden::Golden, func: &'a str, source: &str) -> Vec<&'a Case> {
    g.by_fn(func).filter(|c| c.s("source") == source).collect()
}

/// Source names that have tile cases of `func` (the others feed downsample and compare).
fn tile_sources(g: &as_golden::Golden, func: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for c in g.by_fn(func) {
        let s = c.s("source");
        if !names.contains(&s) {
            names.push(s);
        }
    }
    names
}

/// Write the synthetic source of a golden case to `dir`, identify it and read it back.
fn load_source(dir: &std::path::Path, c: &Case) -> (Source, Vec<u8>) {
    let path = Synth::from_case(c).write(dir);
    let src = identify(&path).unwrap().expect("a source");
    let data = std::fs::read(&path).unwrap();
    (src, data)
}

#[test]
fn srtm_tiles_match_csharp() {
    let g = golden();
    let names = tile_sources(&g, "srtm_tile");
    assert_eq!(names.len(), 3, "{names:?}");
    for name in names {
        let sc = source_case(&g, &name);
        let tmp = tempfile::tempdir().unwrap();
        let (src, data) = load_source(tmp.path(), sc);
        let cases = tile_cases(&g, "srtm_tile", &name);
        assert_eq!(cases.len(), 288, "{name}");
        let out = srtm_tiles(&src, &data, golden_now(&cases), &|_| false).unwrap();
        // The tile order is unaffected by the fix and still matches C#.
        let indices: Vec<&str> = out.iter().map(Outcome::index).collect();
        let expected: Vec<String> = cases.iter().map(|c| c.s("tile")).collect();
        assert_eq!(indices, expected, "{name} order");
        // The per-tile C# equality (elv, elv_fnv, min/max and the `written` decision) is
        // deliberately not asserted for this group: the #294 one-row-south fix changes 863/864
        // cases and their values are opaque FNV hashes. Documented exception, owner-approved
        // 2026-11-26 — see "Area 4 implementation records" in docs/logic-fixes.md. GLOBE keeps
        // asserting C# in `globe_tiles_match_csharp`.
    }
}

#[test]
fn globe_tiles_match_csharp() {
    let g = golden();
    let names = tile_sources(&g, "globe_tile");
    assert_eq!(names, ["C10G", "G10G"]);
    let (mut written, mut not_written) = (0, 0);
    for name in names {
        let sc = source_case(&g, &name);
        let tmp = tempfile::tempdir().unwrap();
        let (src, data) = load_source(tmp.path(), sc);
        let cases = tile_cases(&g, "globe_tile", &name);
        assert_eq!(cases.len(), 576, "{name}");
        let now = golden_now(&cases);
        // the golden block: the (i0, j0, i1, j1) of the first case
        let first = globe_indices(&src)
            .find(|&gi| globe_tile_index(&src, gi) == cases[0].s("tile"))
            .expect("first golden tile");
        let block: Vec<_> = globe_indices(&src)
            .filter(|gi| (gi.i0, gi.j0, gi.i1, gi.j1) == (first.i0, first.j0, first.i1, first.j1))
            .collect();
        assert_eq!(block.len(), 576, "{name}");
        let indices: Vec<String> = block.iter().map(|&gi| globe_tile_index(&src, gi)).collect();
        let expected: Vec<String> = cases.iter().map(|c| c.s("tile")).collect();
        assert_eq!(indices, expected, "{name} order");
        for (&gi, c) in block.iter().zip(&cases) {
            match globe_tile(&src, &data, gi, now).unwrap() {
                Some(t) => {
                    assert!(c.result["written"].as_bool().unwrap(), "{name} {}", t.index);
                    assert_tile(&t, c);
                    written += 1;
                }
                None => {
                    assert!(!c.result["written"].as_bool().unwrap(), "{name} {gi:?}");
                    not_written += 1;
                }
            }
        }
    }
    assert!(written > 0 && not_written > 0, "{written} {not_written}");
}
