//! Port of `ElevationHorizon.cs`: 360 elevation profiles around one location.

/// Elevation profiles for azimuth 0..359.
#[derive(Clone, Debug, PartialEq)]
pub struct ElevationHorizon {
    pub lat: f64,
    pub lon: f64,
    pub distance_km: f64,
    pub step_width_m: f64,
    pub count: usize,
    pub paths: Vec<Vec<i16>>,
    pub valid: bool,
}

impl ElevationHorizon {
    /// Empty horizon; `count = trunc(distance*1000/step)+1`, 360 empty profiles.
    pub fn new(lat: f64, lon: f64, distance_km: f64, step_width_m: f64) -> Self {
        Self {
            lat,
            lon,
            distance_km,
            step_width_m,
            count: ((distance_km * 1000.0 / step_width_m) as i64 + 1).max(0) as usize,
            paths: vec![Vec::new(); 360],
            valid: true,
        }
    }

    /// Profile for an azimuth (`this[int azimuth]`), `None` outside 0..359.
    pub fn get(&self, azimuth: i32) -> Option<&[i16]> {
        usize::try_from(azimuth)
            .ok()
            .and_then(|a| self.paths.get(a))
            .map(|v| v.as_slice())
    }
}
