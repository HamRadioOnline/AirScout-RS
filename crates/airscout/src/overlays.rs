//! GUI-free geometry for the map overlays: distance rings, Maidenhead locator grid, propagation
//! path polylines and antenna beam wedges. Ports of `MapDlg.CreateDistances`,
//! `InitializeLocators`/`CreateLocators`, `DrawPath` and `SupportFunctions.GetBeamWidthFromGain`.
//! The map layer only has to draw what is returned here.

use as_geo::latlon::{destination_point, midpoint};
use as_geo::latlon::{GPoint, GRect};
use as_geo::maidenhead::loc_from_lat_lon;
use as_propagation::{PropagationPath, PropagationPoint};

/// Maximum number of locator cells returned by [`locator_grid`] (protects the frame rate; the
/// original has no cap).
pub const MAX_GRID_CELLS: usize = 2000;

/// Distance rings around the station every 100 km up to 1000 km (`CreateDistances`), each a
/// closed ring of 361 points (bearings 0..=360, so the first and last point coincide).
pub fn distance_rings(lat: f64, lon: f64) -> Vec<Vec<GPoint>> {
    (1..=10)
        .map(|k| {
            let dist = f64::from(k) * 100.0;
            (0..=360)
                .map(|b| destination_point(lat, lon, f64::from(b), dist))
                .collect()
        })
        .collect()
}

/// One cell of the locator grid.
#[derive(Clone, Debug, PartialEq)]
pub struct LocatorCell {
    pub loc: String,
    pub rect: GRect,
}

/// `(lat step, lon step, locator precision)` of one grid level.
type Level = (f64, f64, i32);

const SUB: f64 = 1.0 / 24.0;
const EXT: f64 = SUB / 10.0;
const EXT_SUB: f64 = EXT / 24.0;

/// Coarse levels of the zoom bands, coarsest first (the bold level of `InitializeLocators`).
const COARSE: [Level; 4] = [
    (10.0, 20.0, 1),
    (1.0, 2.0, 2),
    (SUB, 2.0 * SUB, 3),
    (EXT, 2.0 * EXT, 4),
];

/// Band index into [`COARSE`] and the optional fine level for a zoom level, as in
/// `InitializeLocators`. The original supports zoom 1..=20 only; deeper zoom keeps the deepest
/// band.
fn grid_levels(zoom: i32) -> Option<(usize, Option<Level>)> {
    match zoom {
        ..=0 => None,
        1..=5 => Some((0, None)),
        6..=9 => Some((1, None)),
        10..=13 => Some((1, Some((SUB, 2.0 * SUB, 3)))),
        14..=17 => Some((2, Some((EXT, 2.0 * EXT, 4)))),
        _ => Some((3, Some((EXT_SUB, 2.0 * EXT_SUB, 5)))),
    }
}

/// Grid index range `[first, last)` covering `[lo, hi]` of an axis starting at `origin`.
fn index_range(lo: f64, hi: f64, origin: f64, step: f64, max: f64) -> (i64, i64) {
    let n_max = (max / step).round() as i64;
    let first = (((lo - origin) / step).floor() as i64).clamp(0, n_max);
    let last = (((hi - origin) / step).ceil() as i64).clamp(0, n_max);
    (first, last.max(first))
}

/// A view that wraps the antimeridian (`min_lon > max_lon`) is split into its two halves.
fn view_parts(view: &GRect) -> Vec<GRect> {
    if view.min_lon > view.max_lon {
        vec![
            GRect::new(view.min_lat, view.min_lon, view.max_lat, 180.0),
            GRect::new(view.min_lat, -180.0, view.max_lat, view.max_lon),
        ]
    } else {
        vec![*view]
    }
}

fn cell_count(parts: &[GRect], (step_lat, step_lon, _): Level) -> usize {
    parts
        .iter()
        .map(|v| {
            let (la0, la1) = index_range(v.min_lat, v.max_lat, -90.0, step_lat, 180.0);
            let (lo0, lo1) = index_range(v.min_lon, v.max_lon, -180.0, step_lon, 360.0);
            ((la1 - la0) * (lo1 - lo0)) as usize
        })
        .sum()
}

fn level_cells(parts: &[GRect], level: Level, small: bool, out: &mut Vec<LocatorCell>) {
    let (step_lat, step_lon, precision) = level;
    for v in parts {
        let (la0, la1) = index_range(v.min_lat, v.max_lat, -90.0, step_lat, 180.0);
        let (lo0, lo1) = index_range(v.min_lon, v.max_lon, -180.0, step_lon, 360.0);
        for lo in lo0..lo1 {
            for la in la0..la1 {
                let min_lat = -90.0 + la as f64 * step_lat;
                let min_lon = -180.0 + lo as f64 * step_lon;
                let center = (min_lat + step_lat / 2.0, min_lon + step_lon / 2.0);
                if let Ok(Some(loc)) = loc_from_lat_lon(center.0, center.1, small, precision, false)
                {
                    out.push(LocatorCell {
                        loc,
                        rect: GRect::new(min_lat, min_lon, min_lat + step_lat, min_lon + step_lon),
                    });
                }
            }
        }
    }
}

/// Locator cells covering `view` for a map zoom level. The coarse level (drawn bold in the
/// original) comes first, the fine level (thin) second. Zoom below 1 gives no grid. A view with
/// `min_lon > max_lon` is treated as wrapping the antimeridian.
///
/// Cap: the result never exceeds [`MAX_GRID_CELLS`] and never goes blank. If both levels
/// together exceed it, the fine level is dropped; if the coarse level alone still exceeds it,
/// the cell size of the next coarser zoom band is used, repeatedly, until it fits (the coarsest
/// band has at most 324 cells over the whole world).
pub fn locator_grid(view: GRect, zoom: f64, small_letters: bool) -> Vec<LocatorCell> {
    let Some((band, fine)) = grid_levels(zoom.floor() as i32) else {
        return Vec::new();
    };
    let parts = view_parts(&view);
    let mut cells = Vec::new();
    let coarse_n = cell_count(&parts, COARSE[band]);
    if coarse_n <= MAX_GRID_CELLS {
        level_cells(&parts, COARSE[band], small_letters, &mut cells);
        if let Some(fine) = fine {
            if coarse_n + cell_count(&parts, fine) <= MAX_GRID_CELLS {
                level_cells(&parts, fine, small_letters, &mut cells);
            }
        }
        return cells;
    }
    for b in (0..band).rev() {
        if cell_count(&parts, COARSE[b]) <= MAX_GRID_CELLS {
            level_cells(&parts, COARSE[b], small_letters, &mut cells);
            break;
        }
    }
    cells
}

/// Propagation path as polylines. Each `Vec<Vec<GPoint>>` is split at the antimeridian.
#[derive(Clone, Debug, PartialEq)]
pub struct PathOverlay {
    /// Whole path (black if `valid`, red otherwise).
    pub full: Vec<Vec<GPoint>>,
    pub valid: bool,
    /// Part of the path where a plane between 0 and `max_alt_m` is visible from both ends
    /// (magenta).
    pub visible: Vec<Vec<GPoint>>,
    /// First, middle and last point of the visible part (red, blue, yellow small markers);
    /// all `None` unless the visible part has at least two points.
    pub hot_markers: [Option<GPoint>; 3],
    /// Path midpoint (blue marker).
    pub mid: GPoint,
}

/// Splits a polyline wherever it jumps across lon +-180 and closes the gap with interpolated
/// points on the antimeridian, so a map does not draw a line across the whole world.
pub fn split_antimeridian(points: &[GPoint]) -> Vec<Vec<GPoint>> {
    let mut out: Vec<Vec<GPoint>> = Vec::new();
    let mut cur: Vec<GPoint> = Vec::new();
    for &p in points {
        if let Some(&prev) = cur.last() {
            if (p.lon - prev.lon).abs() > 180.0 {
                let (edge_prev, edge_next) = if prev.lon > 0.0 {
                    (180.0, -180.0)
                } else {
                    (-180.0, 180.0)
                };
                // unwrap the longitude of p to the side of prev, then interpolate
                let p_lon = if prev.lon > 0.0 {
                    p.lon + 360.0
                } else {
                    p.lon - 360.0
                };
                let t = (edge_prev - prev.lon) / (p_lon - prev.lon);
                let lat = prev.lat + t * (p.lat - prev.lat);
                cur.push(GPoint::new(lat, edge_prev));
                out.push(std::mem::take(&mut cur));
                cur.push(GPoint::new(lat, edge_next));
            }
        }
        cur.push(p);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `DrawPath`. `max_alt_m` is `Planes_MaxAlt`.
///
/// The last point is the exact DX location. A path without length or without info points
/// yields empty polylines and no hot markers.
pub fn path_overlay(p: &PropagationPath, max_alt_m: f64) -> PathOverlay {
    let par = &p.params;
    let mid = midpoint(par.lat1, par.lon1, par.lat2, par.lon2);
    match p.info_points() {
        Some(pts) if p.distance() > 0.0 => overlay_from_points(
            &pts,
            GPoint::new(par.lat2, par.lon2),
            p.valid,
            mid,
            max_alt_m,
        ),
        _ => PathOverlay {
            full: Vec::new(),
            valid: p.valid,
            visible: Vec::new(),
            hot_markers: [None; 3],
            mid,
        },
    }
}

/// Core of [`path_overlay`] on explicit info points; `dx` replaces the last point.
pub fn overlay_from_points(
    pts: &[PropagationPoint],
    dx: GPoint,
    valid: bool,
    mid: GPoint,
    max_alt_m: f64,
) -> PathOverlay {
    if pts.len() < 2 {
        return PathOverlay {
            full: Vec::new(),
            valid,
            visible: Vec::new(),
            hot_markers: [None; 3],
            mid,
        };
    }
    let last = pts.len() - 1;
    let pos = |i: usize| {
        if i == last {
            dx
        } else {
            GPoint::new(pts[i].lat, pts[i].lon)
        }
    };
    let h = |i: usize| pts[i].h1.max(pts[i].h2);

    let full: Vec<GPoint> = (0..pts.len()).map(pos).collect();
    let visible: Vec<GPoint> = (0..pts.len())
        .filter(|&i| h(i) > 0.0 && h(i) < max_alt_m)
        .map(pos)
        .collect();

    // Hot markers use `max < max_alt` only (no `> 0` test), as in the original.
    let mut hot = (0..pts.len()).filter(|&i| h(i) < max_alt_m);
    let first_hot = hot.next();
    let last_hot = hot.next_back();
    let hot_markers = match (first_hot, last_hot) {
        (Some(i1), Some(i3)) => {
            // markers use the raw info points, not the exact DX location
            let a = GPoint::new(pts[i1].lat, pts[i1].lon);
            let c = GPoint::new(pts[i3].lat, pts[i3].lon);
            [Some(a), Some(midpoint(a.lat, a.lon, c.lat, c.lon)), Some(c)]
        }
        _ => [None; 3],
    };

    PathOverlay {
        full: split_antimeridian(&full),
        valid,
        visible: split_antimeridian(&visible),
        hot_markers,
        mid,
    }
}

/// 3 dB beamwidth in degrees from antenna gain (`GetBeamWidthFromGain`).
pub fn beam_width_deg(gain_dbi: f64) -> f64 {
    (41253.0 / 10f64.powf((gain_dbi + 2.15) / 10.0)).sqrt()
}

/// Beam wedge polygon from (lat, lon) along `bearing` with total `width_deg` and radius
/// `length_km`: left edge outward in 1 km steps, arc in 1 degree steps, right edge back in.
/// Not split at the antimeridian (it is a filled polygon).
pub fn beam_wedge(lat: f64, lon: f64, bearing: f64, width_deg: f64, length_km: f64) -> Vec<GPoint> {
    let left = (bearing - width_deg / 2.0).rem_euclid(360.0);
    let right = (bearing + width_deg / 2.0).rem_euclid(360.0);
    let n = length_km.max(0.0).round() as usize;
    let mut pts = Vec::with_capacity(2 * n + width_deg.max(0.0).ceil() as usize + 1);
    for i in 0..n {
        pts.push(destination_point(lat, lon, left, i as f64));
    }
    let arcs = width_deg.max(0.0).ceil() as usize;
    for i in 0..arcs {
        let a = (left + i as f64).rem_euclid(360.0);
        pts.push(destination_point(lat, lon, a, length_km));
    }
    for i in (0..n).rev() {
        pts.push(destination_point(lat, lon, right, i as f64));
    }
    pts
}
