//! Crossing prediction: which aircraft can reflect signals on a propagation path, and when.
//! Port of `AircraftDatabase.GetNearestPlanes` (`AircraftDatabase.cs:2477-2650`) and of the
//! selection logic in `MapDlg.UpdatePlanes` (`MapDlg.cs:5126-5190`).

use crate::units::kts_kmh;
use crate::PlaneInfo;
use as_geo::latlon::{self, GPoint};
use as_geo::propagation::{epsilon_from_heights, height_from_epsilon, theta_from_heights};
use as_propagation::{IntersectionPoint, PropagationPath};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// `GetNearestPlanes`: classify every plane within `max_radius_km` of the path midpoint
/// (`< 0`: no limit, `0`: half the path length).
///
/// Every plane within the midpoint radius is returned, also with potential 0. `int_point`/`int_qrb`
/// are the finite-path proximity point and lateral distance. `forward_qrb` is present only for a
/// genuine forward crossing within half the path length and positive speed.
#[allow(clippy::too_many_arguments)]
pub fn get_nearest_planes(
    at: DateTime<Utc>,
    path: &PropagationPath,
    planes: &[PlaneInfo],
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
    for info in planes {
        if info.time > at
            || info
                .components_valid_until
                .is_some_and(|expiry| at > expiry)
        {
            continue;
        }
        let hours = (at - info.time)
            .num_nanoseconds()
            .map(|n| n as f64 / 3.6e12)
            .unwrap_or(0.0);
        let distance = info.speed_kmh() * hours;
        let (lat, lon, track) =
            crate::motion::advance_with_tangent(info.lat, info.lon, info.track, distance);
        if latlon::distance(mid.lat, mid.lon, lat, lon) > max_radius {
            continue;
        }
        let mut plane = PlaneInfo::new(
            at,
            &info.call,
            &info.reg,
            &info.hex,
            lat,
            lon,
            track,
            info.alt_ft,
            info.speed_kts,
            &info.type_code,
            &info.manufacturer,
            &info.model,
            info.category,
        );
        plane.altitude_provenance = info.altitude_provenance;
        plane.components_valid_until = info.components_valid_until;
        let near = proximity_point(path, plane.lat, plane.lon);
        let mut forward = forward_crossing(path, &plane);
        let path_bearing = path.bearing12();
        let collinear = heading_delta_deg(plane.track, path_bearing) < 1e-9
            || heading_delta_deg(plane.track, path_bearing + 180.0) < 1e-9;
        if forward.is_none() && near.qrb <= 1e-9 && !collinear {
            forward = Some(IntersectionPoint {
                lat: near.lat,
                lon: near.lon,
                qrb: 0.0,
                min_h: near.min_h,
                dist1: near.dist1,
                dist2: near.dist2,
            });
        }
        plane.int_point = Some(GPoint::new(near.lat, near.lon));
        plane.int_qrb = near.qrb;
        plane.forward_qrb = forward
            .as_ref()
            .filter(|_| plane.speed_kts > 0.0)
            .map(|f| f.qrb);
        let alt_m = plane.alt_m();
        let (h1, h2, r) = (pp.h1, pp.h2, pp.radius_km);
        if near.qrb <= max_dist_km {
            plane.alt_diff = alt_m - near.min_h;
            plane.angle = crossing_angle(near.lat, near.lon, pp.lat2, pp.lon2, plane.track);
            set_angles(&mut plane, near.dist1, near.dist2, alt_m, h1, h2, r);
            plane.potential = if plane.alt_diff > 0.0 {
                100
            } else if near.min_h <= max_alt_m && plane.alt_diff > -max_delta_m {
                50
            } else {
                0
            };
        } else if let Some(f) = forward.filter(|f| f.min_h <= max_alt_m) {
            plane.alt_diff = alt_m - f.min_h;
            plane.angle = crossing_angle(f.lat, f.lon, pp.lat2, pp.lon2, plane.track);
            set_angles(&mut plane, f.dist1, f.dist2, alt_m, h1, h2, r);
            plane.potential = if plane.alt_diff > 0.0 {
                75
            } else if plane.alt_diff > -max_delta_m {
                50
            } else {
                0
            };
        } else {
            plane.potential = 0;
        }
        out.push(plane);
    }
    out
}

/// `MapDlg.UpdatePlanes` "NearestPlane": among planes with `int_qrb <= max_dist_km`, the highest
/// potential, then the smallest `int_qrb` (first one wins ties).
pub fn nearest_plane(planes: &[PlaneInfo], max_dist_km: f64) -> Option<&PlaneInfo> {
    let mut best: Option<&PlaneInfo> = None;
    for p in planes
        .iter()
        .filter(|p| p.int_point.is_some() && p.int_qrb <= max_dist_km)
    {
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
/// (`MapDlg.cs:4870`), if there is a forward crossing and positive speed.
pub fn minutes_to_crossing(p: &PlaneInfo) -> Option<i32> {
    p.forward_qrb
        .filter(|_| p.speed_kts > 0.0)
        .map(|qrb| (qrb / kts_kmh(p.speed_kts) * 60.0) as i32)
}

fn set_angles(plane: &mut PlaneInfo, dist1: f64, dist2: f64, alt_m: f64, h1: f64, h2: f64, r: f64) {
    plane.eps1 = epsilon_from_heights(h1, dist1, alt_m, r);
    plane.eps2 = epsilon_from_heights(h2, dist2, alt_m, r);
    plane.theta1 = theta_from_heights(h1, dist1, alt_m, r);
    plane.theta2 = theta_from_heights(h2, dist2, alt_m, r);
    plane.squint = (plane.theta1 - plane.theta2).abs();
}

fn crossing_angle(lat: f64, lon: f64, lat2: f64, lon2: f64, track: f64) -> f64 {
    let mut ca = latlon::bearing(lat, lon, lat2, lon2) - track;
    if ca < 0.0 {
        ca += 360.0;
    }
    if ca > 180.0 && ca < 360.0 {
        ca = 360.0 - ca;
    }
    ca / 180.0 * std::f64::consts::PI
}

fn forward_crossing(path: &PropagationPath, plane: &PlaneInfo) -> Option<IntersectionPoint> {
    let max_forward = path.distance() / 2.0;
    let mut best = None;
    for i in [
        path.intersection_point(plane.lat, plane.lon, plane.track, -1.0),
        path.intersection_point(plane.lat, plane.lon, plane.track - 180.0, -1.0),
    ]
    .into_iter()
    .flatten()
    {
        if !on_path_segment(i.dist1, i.dist2, path.distance()) || i.qrb > max_forward {
            continue;
        }
        if i.qrb > 1e-9 {
            let b = latlon::bearing(plane.lat, plane.lon, i.lat, i.lon);
            if heading_delta_deg(b, plane.track) > 90.0 {
                continue;
            }
        }
        if best.is_none_or(|b: IntersectionPoint| i.qrb < b.qrb) {
            best = Some(i);
        }
    }
    best
}

fn on_path_segment(dist1: f64, dist2: f64, total: f64) -> bool {
    let tol = total * 1e-9 + 1e-6;
    dist1 >= -tol && dist2 >= -tol && (dist1 + dist2 - total).abs() <= tol
}

fn heading_delta_deg(a: f64, b: f64) -> f64 {
    ((a - b + 540.0) % 360.0 - 180.0).abs()
}

fn proximity_point(path: &PropagationPath, lat: f64, lon: f64) -> IntersectionPoint {
    let p = &path.params;
    let a = to_vec(p.lat1, p.lon1);
    let b = to_vec(p.lat2, p.lon2);
    let q = to_vec(lat, lon);
    let total = path.distance();
    let (point, dist1, dist2) = if total == 0.0 {
        (GPoint::new(p.lat1, p.lon1), 0.0, 0.0)
    } else {
        let n = a.cross(b);
        if n.norm() <= 1e-15 {
            (GPoint::new(p.lat1, p.lon1), 0.0, total)
        } else {
            let n = n.normalized();
            let proj = q.sub(n.scale(q.dot(n)));
            let mut best: Option<(GPoint, f64, f64, f64)> = None;
            if proj.norm() > 1e-15 {
                let c1 = proj.normalized();
                let c2 = c1.scale(-1.0);
                for c in [c1, c2] {
                    let d1 = angle(a, c) * as_geo::latlon::EARTH_RADIUS_KM;
                    let d2 = angle(c, b) * as_geo::latlon::EARTH_RADIUS_KM;
                    if !on_path_segment(d1, d2, total) {
                        continue;
                    }
                    let gp = to_point(c);
                    let qrb = latlon::distance(lat, lon, gp.lat, gp.lon);
                    if best.is_none_or(|(_, _, _, best_qrb)| qrb < best_qrb) {
                        best = Some((gp, d1, d2, qrb));
                    }
                }
            }
            if let Some((gp, d1, d2, _)) = best {
                (gp, d1, d2)
            } else {
                let d_a = latlon::distance(lat, lon, p.lat1, p.lon1);
                let d_b = latlon::distance(lat, lon, p.lat2, p.lon2);
                if d_a <= d_b {
                    (GPoint::new(p.lat1, p.lon1), 0.0, total)
                } else {
                    (GPoint::new(p.lat2, p.lon2), total, 0.0)
                }
            }
        }
    };
    let eps1 = path.eps1_min.max(path.params.local_obstruction);
    let min_h = height_from_epsilon(p.h1, dist1, eps1, p.radius_km).max(height_from_epsilon(
        p.h2,
        dist2,
        path.eps2_min,
        p.radius_km,
    ));
    IntersectionPoint {
        lat: point.lat,
        lon: point.lon,
        qrb: latlon::distance(lat, lon, point.lat, point.lon),
        min_h,
        dist1,
        dist2,
    }
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn normalized(self) -> Self {
        let n = self.norm();
        if n == 0.0 {
            self
        } else {
            self.scale(1.0 / n)
        }
    }
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
    fn scale(self, k: f64) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
            z: self.z * k,
        }
    }
}

fn to_vec(lat: f64, lon: f64) -> Vec3 {
    let lat = lat.to_radians();
    let lon = lon.to_radians();
    let clat = lat.cos();
    Vec3 {
        x: clat * lon.cos(),
        y: clat * lon.sin(),
        z: lat.sin(),
    }
}

fn to_point(v: Vec3) -> GPoint {
    let n = v.normalized();
    GPoint::new(n.z.asin().to_degrees(), n.y.atan2(n.x).to_degrees())
}

fn angle(a: Vec3, b: Vec3) -> f64 {
    a.dot(b).clamp(-1.0, 1.0).acos()
}
