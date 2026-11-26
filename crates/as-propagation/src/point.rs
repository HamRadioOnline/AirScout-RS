//! `PropagationPoint.cs`, `IntersectionPoint.cs`.

/// A point on the path with the minimum heights an object needs to be seen from both ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropagationPoint {
    pub lat: f64,
    pub lon: f64,
    /// Minimum height [m] to be seen from station 1.
    pub h1: f64,
    /// Minimum height [m] to be seen from station 2.
    pub h2: f64,
    /// First Fresnel zone radius [m].
    pub f1: f64,
}

/// Where a plane's track crosses the path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntersectionPoint {
    pub lat: f64,
    pub lon: f64,
    /// Distance plane -> intersection [km].
    pub qrb: f64,
    /// Minimum altitude [m] needed at the intersection.
    pub min_h: f64,
    /// Distance station 1 -> intersection [km].
    pub dist1: f64,
    /// Distance station 2 -> intersection [km].
    pub dist2: f64,
}
