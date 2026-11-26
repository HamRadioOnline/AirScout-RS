//! Port of `PropagationPath.cs` (`PropagationPathDesignator`).

use crate::{IntersectionPoint, PropagationPoint};
use as_geo::latlon::{self, GPoint};
use as_geo::propagation::{f1_radius, height_from_epsilon};
use chrono::{DateTime, Utc};

/// Inputs of a propagation path calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathParams {
    pub lat1: f64,
    pub lon1: f64,
    /// Height of station 1 [m] above sea level (the app passes ground elevation + antenna height).
    pub h1: f64,
    pub lat2: f64,
    pub lon2: f64,
    pub h2: f64,
    pub qrg_ghz: f64,
    /// Equivalent earth radius [km] (6371 * k-factor).
    pub radius_km: f64,
    pub f1_clearance: f64,
    pub step_width_m: f64,
    /// Local obstruction elevation angle [rad] at station 1 (`f64::MIN` = none).
    pub local_obstruction: f64,
}

/// Result of a propagation path calculation: minimum elevation angles at both ends.
#[derive(Clone, Debug, PartialEq)]
pub struct PropagationPath {
    pub params: PathParams,
    /// Minimum elevation angle [rad] at station 1 (terrain + Fresnel clearance).
    pub eps1_min: f64,
    /// Minimum elevation angle [rad] at station 2.
    pub eps2_min: f64,
    pub valid: bool,
    pub local_obstructed: bool,
    pub last_updated: DateTime<Utc>,
}

impl PropagationPath {
    /// Constructor; sets `local_obstructed` like the C# constructors.
    pub fn new(params: PathParams, eps1_min: f64, eps2_min: f64, valid: bool) -> Self {
        Self {
            params,
            eps1_min,
            eps2_min,
            valid,
            local_obstructed: eps1_min < params.local_obstruction,
            last_updated: Utc::now(),
        }
    }

    /// Path distance [km].
    pub fn distance(&self) -> f64 {
        let p = &self.params;
        latlon::distance(p.lat1, p.lon1, p.lat2, p.lon2)
    }
    /// Bearing 1 -> 2 [deg].
    pub fn bearing12(&self) -> f64 {
        let p = &self.params;
        latlon::bearing(p.lat1, p.lon1, p.lat2, p.lon2)
    }
    /// Bearing 2 -> 1 [deg].
    pub fn bearing21(&self) -> f64 {
        let p = &self.params;
        latlon::bearing(p.lat2, p.lon2, p.lat1, p.lon1)
    }

    fn eps1_effective(&self) -> f64 {
        self.eps1_min.max(self.params.local_obstruction)
    }

    /// `GetInfoPoints`: one point per km, `trunc(distance) + 2` points.
    pub fn info_points(&self) -> Option<Vec<PropagationPoint>> {
        if self.params.step_width_m <= 0.0 {
            return None;
        }
        let n = self.distance() as usize + 2;
        (0..n).map(|i| self.info_point(i as f64)).collect()
    }

    /// `GetInfoPoint`. Reproduces `logic-findings.md` #1 (F1 radius uses the full distance as
    /// the first leg).
    pub fn info_point(&self, dist: f64) -> Option<PropagationPoint> {
        if self.params.step_width_m <= 0.0 {
            return None;
        }
        let p = &self.params;
        let distance = self.distance();
        let g = latlon::destination_point(p.lat1, p.lon1, self.bearing12(), dist);
        Some(PropagationPoint {
            lat: g.lat,
            lon: g.lon,
            h1: height_from_epsilon(p.h1, dist, self.eps1_effective(), p.radius_km),
            h2: height_from_epsilon(p.h2, distance - dist, self.eps2_min, p.radius_km),
            f1: f1_radius(p.qrg_ghz, distance, distance - dist),
        })
    }

    /// `GetMidPoint`.
    pub fn mid_point(&self) -> GPoint {
        let p = &self.params;
        latlon::midpoint(p.lat1, p.lon1, p.lat2, p.lon2)
    }

    /// `GetIntersectionPoint`: where a plane at (lat, lon) flying `bearing` crosses the path.
    /// `max_distance`: < 0 no limit, 0 = half the path length, > 0 limit [km].
    /// Reproduces `logic-findings.md` #2 (planes *closer* than `max_distance > 0` are rejected).
    pub fn intersection_point(
        &self,
        lat: f64,
        lon: f64,
        bearing: f64,
        max_distance: f64,
    ) -> Option<IntersectionPoint> {
        let p = &self.params;
        let x = latlon::intersection_point(p.lat1, p.lon1, self.bearing12(), lat, lon, bearing)?;
        let dist1 = latlon::distance(p.lat1, p.lon1, x.lat, x.lon);
        let dist2 = latlon::distance(p.lat2, p.lon2, x.lat, x.lon);
        let min_h = height_from_epsilon(p.h1, dist1, self.eps1_effective(), p.radius_km)
            .max(height_from_epsilon(p.h2, dist2, self.eps2_min, p.radius_km));
        let qrb = latlon::distance(x.lat, x.lon, lat, lon);
        let ip = IntersectionPoint {
            lat: x.lat,
            lon: x.lon,
            qrb,
            min_h,
            dist1,
            dist2,
        };
        if max_distance < 0.0 {
            return Some(ip);
        }
        if max_distance == 0.0 && qrb > self.distance() / 2.0 {
            return None;
        }
        if max_distance > 0.0 && qrb < max_distance {
            return None;
        }
        Some(ip)
    }
}
