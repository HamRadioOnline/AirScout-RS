//! Provider DbIds and the map preloader's tile list against `tests/golden/maps.json`.

use as_geo::latlon::GRect;
use as_maps::{
    db_id, lat2tiley, long2tilex, preload_tile_count, preload_tiles, DBID_OCM_LANDSCAPE,
    DBID_OPENTOPOMAP, DBID_OSM,
};
use serde_json::Value;

fn golden() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/maps.json");
    let text = std::fs::read_to_string(path).expect("read maps.json");
    serde_json::from_str(&text).expect("parse maps.json")
}

/// Golden areas are `[min_lat, max_lat, min_lon, max_lon]`.
fn area(v: &Value) -> GRect {
    let a: Vec<f64> = v
        .as_array()
        .expect("area array")
        .iter()
        .map(|n| n.as_f64().expect("area number"))
        .collect();
    GRect::new(a[0], a[2], a[1], a[3])
}

fn int(v: &Value, k: &str) -> i64 {
    v[k].as_i64()
        .unwrap_or_else(|| panic!("{k} not an integer in {v}"))
}

#[test]
fn db_ids_match_golden() {
    let g = golden();
    let ids = g["db_ids"].as_array().expect("db_ids");
    assert_eq!(ids.len(), 3);
    for e in ids {
        let name = e["name"].as_str().expect("name");
        let id = i32::try_from(int(e, "db_id")).expect("db_id fits i32");
        assert_eq!(db_id(name), Some(id), "{name}");
    }
    let by_name = |n: &str| db_id(n).expect(n);
    assert_eq!(DBID_OSM, by_name("OpenStreetMap"));
    assert_eq!(DBID_OPENTOPOMAP, by_name("OpenTopoMap"));
    assert_eq!(DBID_OCM_LANDSCAPE, by_name("OCM Landscape"));
    assert_eq!(DBID_OSM, 1_492_776_782);
    assert_eq!(db_id("Bing"), None);
}

#[test]
fn ranges_match_golden_where_valid() {
    let g = golden();
    let ranges = g["ranges"].as_array().expect("ranges");
    assert_eq!(ranges.len(), 60);
    let mut valid = 0;
    for r in ranges {
        let a = area(&r["area"]);
        let z = u8::try_from(int(r, "z")).expect("z");
        // The raw C# bounds (non-finite gives int.MinValue) hold for every entry.
        let (xmin, xmax) = (long2tilex(a.min_lon, z), long2tilex(a.max_lon, z));
        let (ymin, ymax) = (lat2tiley(a.max_lat, z), lat2tiley(a.min_lat, z));
        assert_eq!(
            [xmin, xmax, ymin, ymax],
            [
                int(r, "xmin"),
                int(r, "xmax"),
                int(r, "ymin"),
                int(r, "ymax")
            ],
            "{r}"
        );
        let last = (1i64 << z) - 1;
        let inside = |v: i64| (0..=last).contains(&v);
        if [xmin, xmax, ymin, ymax].into_iter().all(inside) {
            valid += 1;
            let total = u64::try_from(int(r, "total")).expect("total");
            assert_eq!(preload_tiles(&a, z, z).len() as u64, total, "{r}");
            assert_eq!(preload_tile_count(&a, z, z), total, "{r}");
        }
    }
    // The default area and the small area are in range at every zoom; the others touch ±180° or a pole.
    assert_eq!(valid, 24);
}

#[test]
fn preload_tiles_clamped_at_poles() {
    // North: lat 90 gives a negative ymin, lon 180 gives xmax = 2^z (one past the last column).
    let north = GRect::new(80.0, -180.0, 90.0, 180.0);
    let tiles = preload_tiles(&north, 3, 3);
    let expected: Vec<(u8, u32, u32)> = (0..8).map(|x| (3, x, 0)).collect();
    assert_eq!(tiles, expected);
    assert_eq!(preload_tile_count(&north, 3, 3), 8);

    // South: lat -90 gives ymax = +Infinity (C# int.MinValue, zero tiles); clamped to the last row.
    let south = GRect::new(-90.0, -10.0, -80.0, 10.0);
    let tiles = preload_tiles(&south, 3, 3);
    assert_eq!(tiles, vec![(3, 3, 7), (3, 4, 7)]);
    assert_eq!(preload_tile_count(&south, 3, 3), 2);

    for a in [north, south] {
        let tiles = preload_tiles(&a, 3, 3);
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|&(_, x, y)| x < 8 && y < 8));
        let mut dedup = tiles.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), tiles.len());
    }
}

#[test]
fn whole_world_count_is_every_tile() {
    let world = GRect::new(-90.0, -180.0, 90.0, 180.0);
    // sum of 4^z for z = 0..=11
    assert_eq!(preload_tile_count(&world, 0, 11), 5_592_405);
    assert_eq!(preload_tiles(&world, 0, 4).len(), 1 + 4 + 16 + 64 + 256);
}

#[test]
fn preload_order_is_z_x_y() {
    let a = GRect::new(50.0, 10.0, 51.0, 11.0);
    let tiles = preload_tiles(&a, 5, 6);
    assert!(!tiles.is_empty());
    let mut sorted = tiles.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, tiles);
    assert_eq!(tiles.first().map(|t| t.0), Some(5));
    assert_eq!(tiles.last().map(|t| t.0), Some(6));
    assert_eq!(preload_tile_count(&a, 5, 6), tiles.len() as u64);
}

#[test]
fn empty_when_max_below_min() {
    let a = GRect::new(35.0, -15.0, 60.0, 30.0);
    assert!(preload_tiles(&a, 7, 6).is_empty());
    assert_eq!(preload_tile_count(&a, 7, 6), 0);
}

#[test]
fn nan_area_gives_no_tiles() {
    let a = GRect::new(f64::NAN, 10.0, 51.0, 11.0);
    assert!(preload_tiles(&a, 5, 6).is_empty());
    assert_eq!(preload_tile_count(&a, 5, 6), 0);
    assert_eq!(lat2tiley(f64::NAN, 5), i64::from(i32::MIN));
}
