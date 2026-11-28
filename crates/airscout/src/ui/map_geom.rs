//! Pure geometry and selection helpers of the map (no painting).

use crate::app::PathEntry;
use crate::overlays::{self, PathOverlay};
use as_aircraft::PlaneInfo;
use as_geo::latlon::{GPoint, GRect};
use eframe::egui::Pos2;

/// Margin around the path when zooming to fit, as a fraction of the box.
const FIT_MARGIN: f64 = 0.1;
/// Radius around a plane icon that counts as a hit [px].
pub const HIT_SLACK_PX: f32 = 4.0;
/// Upper bound of points per beam edge after thinning.
const WEDGE_MAX_POINTS: usize = 400;

/// Plane whose icon (radius `size / 2` plus [`HIT_SLACK_PX`]) is under `pointer`; the nearest
/// one if icons overlap.
pub fn hit_plane(
    planes: &[PlaneInfo],
    project: impl Fn(&PlaneInfo) -> Pos2,
    size: impl Fn(&PlaneInfo) -> f32,
    pointer: Pos2,
) -> Option<&PlaneInfo> {
    planes
        .iter()
        .filter_map(|p| {
            let d = project(p).distance(pointer);
            (d <= size(p) / 2.0 + HIT_SLACK_PX).then_some((d, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| p)
}

/// Click on a plane: select it, or deselect it if it already is the selected one.
pub fn toggle_selection(selected: &mut Option<String>, hex: &str) {
    if selected.as_deref() == Some(hex) {
        *selected = None;
    } else {
        *selected = Some(hex.to_string());
    }
}

/// Keep at most [`WEDGE_MAX_POINTS`] points of a beam polygon (the edges are straight enough).
pub fn thin(pts: Vec<GPoint>) -> Vec<GPoint> {
    let stride = pts.len().div_ceil(WEDGE_MAX_POINTS).max(1);
    let last = pts.last().copied();
    let mut out: Vec<GPoint> = pts.into_iter().step_by(stride).collect();
    if let (Some(l), Some(o)) = (last, out.last()) {
        if *o != l {
            out.push(l);
        }
    }
    out
}

/// Smallest box around `pts` (empty input gives a point at 0, 0).
pub fn bbox_of(pts: &[GPoint]) -> GRect {
    let mut r = GRect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        r.min_lat = r.min_lat.min(p.lat);
        r.max_lat = r.max_lat.max(p.lat);
        r.min_lon = r.min_lon.min(p.lon);
        r.max_lon = r.max_lon.max(p.lon);
    }
    if pts.is_empty() {
        GRect::new(0.0, 0.0, 0.0, 0.0)
    } else {
        r
    }
}

/// Mercator y in world fractions: 0 at lat +85.05, 1 at lat -85.05.
fn merc_y(lat: f64) -> f64 {
    let lat = lat.clamp(-85.05112878, 85.05112878).to_radians();
    0.5 - (lat.tan() + 1.0 / lat.cos()).ln() / (2.0 * std::f64::consts::PI)
}

fn merc_lat(y: f64) -> f64 {
    let n = std::f64::consts::PI * (1.0 - 2.0 * y);
    n.sinh().atan().to_degrees()
}

/// Centre and (fractional) zoom at which `bbox` plus a margin fills a map of `w` x `h` px
/// (`SetZoomToFitRect`). Zoom is clamped to 1..=18.
pub fn fit_view(bbox: GRect, w: f32, h: f32) -> (GPoint, f64) {
    let dx = ((bbox.max_lon - bbox.min_lon) / 360.0 * (1.0 + 2.0 * FIT_MARGIN)).max(1e-9);
    let (y0, y1) = (merc_y(bbox.max_lat), merc_y(bbox.min_lat));
    let dy = ((y1 - y0) * (1.0 + 2.0 * FIT_MARGIN)).max(1e-9);
    let zx = (f64::from(w.max(1.0)) / (256.0 * dx)).log2();
    let zy = (f64::from(h.max(1.0)) / (256.0 * dy)).log2();
    let centre = GPoint::new(
        merc_lat((y0 + y1) / 2.0),
        (bbox.min_lon + bbox.max_lon) / 2.0,
    );
    (centre, zx.min(zy).clamp(1.0, 18.0))
}

pub fn contains(outer: &GRect, inner: &GRect) -> bool {
    outer.min_lat <= inner.min_lat
        && outer.max_lat >= inner.max_lat
        && outer.min_lon <= inner.min_lon
        && outer.max_lon >= inner.max_lon
}

/// `view` grown by half its size on every side (clamped to the world).
pub fn pad(view: &GRect) -> GRect {
    let dlat = (view.max_lat - view.min_lat) / 2.0;
    let dlon = (view.max_lon - view.min_lon) / 2.0;
    GRect::new(
        (view.min_lat - dlat).max(-90.0),
        (view.min_lon - dlon).max(-180.0),
        (view.max_lat + dlat).min(90.0),
        (view.max_lon + dlon).min(180.0),
    )
}

/// Bounding box of My, DX and the drawn line of every path; `None` without paths.
pub fn paths_bbox(entries: &[PathEntry], overlays: &[PathOverlay]) -> Option<GRect> {
    if entries.is_empty() {
        return None;
    }
    let mut pts = Vec::new();
    for e in entries {
        let par = &e.path.params;
        pts.push(GPoint::new(par.lat1, par.lon1));
        pts.push(GPoint::new(par.lat2, par.lon2));
    }
    pts.extend(
        overlays
            .iter()
            .flat_map(|o| o.full.iter().flatten().copied()),
    );
    Some(bbox_of(&pts))
}

/// Beam wedges of one path: at My with the gain of My, at DX with the gain of DX. The original
/// does not say whether the gain is dBd or dBi; the QRV gain is used as is.
pub fn beam_wedges(e: &PathEntry) -> Vec<Vec<GPoint>> {
    let p = &e.path;
    let par = &p.params;
    let len = p.distance();
    vec![
        thin(overlays::beam_wedge(
            par.lat1,
            par.lon1,
            p.bearing12(),
            overlays::beam_width_deg(e.my_gain_dbi),
            len,
        )),
        thin(overlays::beam_wedge(
            par.lat2,
            par.lon2,
            p.bearing21(),
            overlays::beam_width_deg(e.dx_gain_dbi),
            len,
        )),
    ]
}
