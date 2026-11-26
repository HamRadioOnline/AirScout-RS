//! Crossing prediction: which aircraft can reflect signals on a propagation path, and when.
//! Port of `AircraftDatabase.GetNearestPlanes` (`AircraftDatabase.cs:2477-2650`) and of the
//! selection logic in `MapDlg.UpdatePlanes` (`MapDlg.cs:5126-5190`).

use crate::units::kts_kmh;
use crate::PlaneInfo;
use as_geo::latlon::{self, GPoint};
use as_geo::propagation::{epsilon_from_heights, theta_from_heights};
use as_propagation::PropagationPath;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// `GetNearestPlanes`: classify every plane within `max_radius_km` of the path midpoint
/// (`< 0`: no limit, `0`: half the path length).
///
/// For each plane four intersections with the path are tried (along the track, both
/// perpendiculars to the path, and the reverse track); the nearest one decides:
/// - near the path (`qrb <= max_dist_km` and before station 2): potential **100** if the plane
///   is above the minimum height, else 50 if `min_h <= max_alt_m && alt_diff > max_delta_m`
///   (never true, logic-findings #25), else 0;
/// - otherwise, if flying towards the path and the minimum height there is `<= max_alt_m`:
///   **75** if high enough, **50** if less than `max_delta_m` too low, else 0. Squint uses the
///   nearest intersection, theta stays 0 (logic-findings #26).
///
/// Every plane within the radius is returned, also with potential 0. The results are new
/// objects (`PlaneInfo::new`, so `From`/`To`/`VSpeed` and the old values are not carried over).
///
/// `planes` is `&mut` on purpose: the original dead-reckons the **caller's** planes to `at` after
/// copying them and then uses the copy's (stale) position (logic-findings #24). The
/// extrapolation therefore has no effect on the result, but a second path in MULTI mode sees the
/// moved positions.
#[allow(clippy::too_many_arguments)]
pub fn get_nearest_planes(
    at: DateTime<Utc>,
    path: &PropagationPath,
    planes: &mut [PlaneInfo],
    max_radius_km: f64,
    max_dist_km: f64,
    max_alt_m: f64,
    max_delta_m: f64,
) -> Vec<PlaneInfo> {
    let mut out = Vec::new();
    let max_radius = if max_radius_km < 0.0 {
        f64::MAX
    } else if max_radius_km == 0.0 {
        path.distance() / 2.0
    } else {
        max_radius_km
    };
    let mid = path.mid_point();
    let pp = &path.params;
    for info in planes.iter_mut() {
        if latlon::distance(mid.lat, mid.lon, info.lat, info.lon) > max_radius {
            continue;
        }
        let mut plane = PlaneInfo::new(
            info.time,
            &info.call,
            &info.reg,
            &info.hex,
            info.lat,
            info.lon,
            info.track,
            info.alt_ft,
            info.speed_kts,
            &info.type_code,
            &info.manufacturer,
            &info.model,
            info.category,
        );
        // dead-reckon the caller's object (not the copy), as the original does
        let hours = (at - info.time)
            .num_nanoseconds()
            .map(|n| n as f64 / 3.6e12)
            .unwrap_or(0.0);
        let np =
            latlon::destination_point(info.lat, info.lon, info.track, info.speed_kmh() * hours);
        info.lat = np.lat;
        info.lon = np.lon;
        info.time = at;

        let i1 = path.intersection_point(plane.lat, plane.lon, plane.track, 0.0);
        let i2 = path.intersection_point(plane.lat, plane.lon, path.bearing12() - 90.0, 0.0);
        let i3 = if i2.is_none() {
            path.intersection_point(plane.lat, plane.lon, path.bearing12() + 90.0, 0.0)
        } else {
            None
        };
        let i4 = if i1.is_none() {
            path.intersection_point(plane.lat, plane.lon, plane.track - 180.0, 0.0)
        } else {
            None
        };
        let mut imin = i1;
        for i in [i2, i3, i4].into_iter().flatten() {
            if imin.is_none_or(|m| i.qrb < m.qrb) {
                imin = Some(i);
            }
        }
        let alt_m = plane.alt_m();
        let (h1, h2, r) = (pp.h1, pp.h2, pp.radius_km);
        match imin {
            Some(m) if m.qrb <= max_dist_km && m.dist1 <= path.distance() => {
                let ip = GPoint::new(m.lat, m.lon);
                plane.int_point = Some(ip);
                plane.int_qrb = m.qrb;
                plane.alt_diff = alt_m - m.min_h;
                let mut ca = latlon::bearing(ip.lat, ip.lon, pp.lat2, pp.lon2) - plane.track;
                if ca < 0.0 {
                    ca += 360.0;
                }
                if ca > 180.0 && ca < 360.0 {
                    ca = 360.0 - ca;
                }
                plane.angle = ca / 180.0 * std::f64::consts::PI;
                plane.eps1 = epsilon_from_heights(h1, m.dist1, alt_m, r);
                plane.eps2 = epsilon_from_heights(h2, m.dist2, alt_m, r);
                plane.theta1 = theta_from_heights(h1, m.dist1, alt_m, r);
                plane.theta2 = theta_from_heights(h2, m.dist2, alt_m, r);
                plane.squint = (theta_from_heights(h1, m.dist1, alt_m, r)
                    - theta_from_heights(h2, m.dist2, alt_m, r))
                .abs();
                plane.potential = if plane.alt_diff > 0.0 {
                    100
                } else if m.min_h <= max_alt_m && plane.alt_diff > max_delta_m {
                    50
                } else {
                    0
                };
            }
            _ => match (i1, imin) {
                (Some(i), Some(m)) if i.min_h <= max_alt_m && i.dist1 <= path.distance() => {
                    plane.int_point = Some(GPoint::new(i.lat, i.lon));
                    plane.int_qrb = i.qrb;
                    plane.alt_diff = alt_m - i.min_h;
                    plane.eps1 = epsilon_from_heights(h1, i.dist1, alt_m, r);
                    plane.eps2 = epsilon_from_heights(h2, i.dist2, alt_m, r);
                    plane.squint = (theta_from_heights(h1, m.dist1, alt_m, r)
                        - theta_from_heights(h2, m.dist2, alt_m, r))
                    .abs();
                    if plane.alt_diff > 0.0 {
                        plane.potential = 75;
                    } else if plane.alt_diff > -max_delta_m {
                        plane.potential = 50;
                    }
                }
                _ => plane.potential = 0,
            },
        }
        out.push(plane);
    }
    out
}

/// `MapDlg.UpdatePlanes` "NearestPlane": among planes with `int_qrb < max_dist_km`, the highest
/// potential, then the smallest `int_qrb` (first one wins ties).
pub fn nearest_plane(planes: &[PlaneInfo], max_dist_km: f64) -> Option<&PlaneInfo> {
    let mut best: Option<&PlaneInfo> = None;
    for p in planes.iter().filter(|p| p.int_qrb < max_dist_km) {
        best = match best {
            None => Some(p),
            Some(b) if p.potential > b.potential => Some(p),
            Some(b) if p.potential == b.potential && p.int_qrb < b.int_qrb => Some(p),
            keep => keep,
        };
    }
    best
}

/// `MapDlg.UpdatePlanes` "ActivePlanes": add new planes; replace a known one if the new result
/// has an intersection and a higher potential, or the same potential and a smaller `int_qrb`.
pub fn merge_active(active: &mut BTreeMap<String, PlaneInfo>, found: Vec<PlaneInfo>) {
    for p in found {
        match active.get(&p.hex) {
            None => {
                active.insert(p.hex.clone(), p);
            }
            Some(a) => {
                let update = if p.int_point.is_some() && p.potential > a.potential {
                    true
                } else {
                    p.potential == a.potential && p.int_qrb < a.int_qrb
                };
                if update {
                    active.insert(p.hex.clone(), p);
                }
            }
        }
    }
}

/// Minutes until the plane reaches its intersection point, truncated: `int_qrb / speed_kmh * 60`
/// (`MapDlg.cs:4870`); 0 if the speed is not positive.
pub fn minutes_to_crossing(p: &PlaneInfo) -> i32 {
    if p.speed_kts > 0.0 {
        (p.int_qrb / kts_kmh(p.speed_kts) * 60.0) as i32
    } else {
        0
    }
}
