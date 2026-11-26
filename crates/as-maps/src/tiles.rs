//! GMap provider `DbId`s and the map preloader's tile list.

use as_geo::latlon::GRect;
use std::f64::consts::PI;

/// `DbId` of GMap's `OpenStreetMapProvider` (the `Type` column of `Tiles`).
///
/// `GMapProvider.cs:236`: `Math.Abs(BitConverter.ToInt32(SHA1(Id.ToByteArray()), 0))`; the value
/// comes from `tests/golden/maps.json`.
pub const DBID_OSM: i32 = 1_492_776_782;
/// `DbId` of GMap's `OpenTopoMapProvider`.
pub const DBID_OPENTOPOMAP: i32 = 1_138_496_160;
/// `DbId` of GMap's `OpenCycleLandscapeMapProvider` ("OCM Landscape").
pub const DBID_OCM_LANDSCAPE: i32 = 1_581_784_214;

/// The airscout.eu tile mirror the C# map preloader downloads from (`<url><z>/<x>/<y>.png`).
pub const MIRROR_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/";

/// The GMap `DbId` for a provider name as `airscout::tiles` names them.
pub fn db_id(provider_name: &str) -> Option<i32> {
    match provider_name {
        "OpenStreetMap" => Some(DBID_OSM),
        "OpenTopoMap" => Some(DBID_OPENTOPOMAP),
        "OCM Landscape" => Some(DBID_OCM_LANDSCAPE),
        _ => None,
    }
}

/// `1 << z` as the C# `int` shift gives it for `z <= 30`.
fn tiles_per_axis(z: u8) -> f64 {
    2f64.powi(i32::from(z))
}

/// `MapPreloader.cs:57-60` before the `(int)` cast.
fn long2tilex_raw(lon: f64, z: u8) -> f64 {
    ((lon + 180.0) / 360.0 * tiles_per_axis(z)).floor()
}

/// `MapPreloader.cs:62-65` before the `(int)` cast.
fn lat2tiley_raw(lat: f64, z: u8) -> f64 {
    let r = lat / 180.0 * PI;
    ((1.0 - (r.tan() + 1.0 / r.cos()).ln() / PI) / 2.0 * tiles_per_axis(z)).floor()
}

/// The C# `(int)` cast as x86-64 does it: NaN, ±Infinity and out-of-range give `int.MinValue`.
fn csharp_int(v: f64) -> i64 {
    if v.is_finite() && v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
        v as i64
    } else {
        i64::from(i32::MIN)
    }
}

/// Tile column of a longitude, the C# formula of `MapPreloader.cs:57-60` unchanged (lon 180
/// gives `2^z`, one past the last column).
pub fn long2tilex(lon: f64, z: u8) -> i64 {
    csharp_int(long2tilex_raw(lon, z))
}

/// Tile row of a latitude, the C# formula of `MapPreloader.cs:62-65` unchanged (lat 90 gives a
/// negative row, lat -90 gives +Infinity and so `int.MinValue`).
pub fn lat2tiley(lat: f64, z: u8) -> i64 {
    csharp_int(lat2tiley_raw(lat, z))
}

/// Inclusive `(xmin, xmax, ymin, ymax)` of `area` at zoom `z` as `MapPreloader.cs:150-153`
/// computes them, each clamped to `0..=2^z-1` (+Infinity to the last row, -Infinity to 0).
/// `None` when the area is empty there, a bound is NaN, or `z > 31`.
fn clamped_bounds(area: &GRect, z: u8) -> Option<(u32, u32, u32, u32)> {
    if z > 31 {
        return None;
    }
    let last = tiles_per_axis(z) - 1.0;
    let clamp = |v: f64| {
        if v.is_nan() {
            None
        } else {
            // In 0..=2^31-1 after the clamp, so the cast is exact.
            Some(v.clamp(0.0, last) as u32)
        }
    };
    let xmin = clamp(long2tilex_raw(area.min_lon, z))?;
    let xmax = clamp(long2tilex_raw(area.max_lon, z))?;
    let ymin = clamp(lat2tiley_raw(area.max_lat, z))?;
    let ymax = clamp(lat2tiley_raw(area.min_lat, z))?;
    (xmin <= xmax && ymin <= ymax).then_some((xmin, xmax, ymin, ymax))
}

/// The tiles the map preloader fetches for `area` from zoom `zmin` to `zmax`, as `(z, x, y)` in
/// the C# loop order of `MapPreloader.cs:148-156` (z ascending, then x, then y).
///
/// Deviation: the C# bounds are clamped to `0..=2^z-1`, so an area at a pole or at ±180° gives
/// only tiles that exist. Empty when `zmax < zmin`.
pub fn preload_tiles(area: &GRect, zmin: u8, zmax: u8) -> Vec<(u8, u32, u32)> {
    let mut tiles = Vec::new();
    for z in zmin..=zmax {
        if let Some((xmin, xmax, ymin, ymax)) = clamped_bounds(area, z) {
            for x in xmin..=xmax {
                tiles.extend((ymin..=ymax).map(|y| (z, x, y)));
            }
        }
    }
    tiles
}

/// `preload_tiles(area, zmin, zmax).len()` without building the list.
pub fn preload_tile_count(area: &GRect, zmin: u8, zmax: u8) -> u64 {
    (zmin..=zmax)
        .filter_map(|z| clamped_bounds(area, z))
        .map(|(xmin, xmax, ymin, ymax)| {
            u64::from(xmax - xmin + 1).saturating_mul(u64::from(ymax - ymin + 1))
        })
        .fold(0, u64::saturating_add)
}
