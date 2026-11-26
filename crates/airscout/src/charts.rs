//! Path profile chart series (port of `MapDlg.UpdateCharts`, MapDlg.cs:4602-4658).
//!
//! x = km from the first station (My), y = metres. GUI-free; plotted by a later task.

use as_geo::propagation::{epsilon_from_heights, height_from_epsilon};
use as_propagation::PropagationPath;

/// Series for the path profile chart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileSeries {
    /// Ground elevation along the path.
    pub elevation: Vec<[f64; 2]>,
    /// Minimum plane height seen from station 1.
    pub min_h1: Vec<[f64; 2]>,
    /// Minimum plane height seen from station 2.
    pub min_h2: Vec<[f64; 2]>,
    /// Constant maximum altitude line.
    pub max_h: Vec<[f64; 2]>,
    /// Area between `[x, max_alt]` (upper) and the second value (lower bound).
    pub min_h_area: Vec<([f64; 2], f64)>,
    /// Line-of-sight between the antennas.
    pub los: Vec<[f64; 2]>,
    /// Elevation axis maximum: peak elevation + 10 %.
    pub elevation_y_max: f64,
}

/// `GetMinH`: lower bound of the usable plane height band.
fn min_h(max_alt: f64, h1: f64, h2: f64) -> f64 {
    let max = h1.max(h2);
    if max <= max_alt {
        max
    } else {
        max_alt
    }
}

/// Builds the profile series. `terrain` holds one ground elevation [m] per entry of
/// `p.info_points()` (same length and order). On a length mismatch or without info points
/// an empty series is returned.
///
/// `params.h1/h2` are antenna heights above sea level (ground included), so the C#
/// `GetElevation(..) + AntennaHeight` terms are already folded in.
pub fn profile(p: &PropagationPath, terrain: &[f64], max_alt_m: f64) -> ProfileSeries {
    let Some(points) = p.info_points() else {
        return ProfileSeries::default();
    };
    if points.len() != terrain.len() {
        return ProfileSeries::default();
    }
    let r = p.params.radius_km;
    let (h1, h2) = (p.params.h1, p.params.h2);
    let eps_los = epsilon_from_heights(h1, p.distance(), h2, r);
    let mut s = ProfileSeries::default();
    let mut maxelv = f64::MIN;
    for (i, (pt, &elv)) in points.iter().zip(terrain).enumerate() {
        let x = i as f64;
        s.elevation.push([x, elv]);
        s.min_h1.push([x, pt.h1]);
        s.min_h2.push([x, pt.h2]);
        s.max_h.push([x, max_alt_m]);
        s.min_h_area
            .push(([x, max_alt_m], min_h(max_alt_m, pt.h1, pt.h2)));
        s.los.push([x, height_from_epsilon(h1, x, eps_los, r)]);
        maxelv = maxelv.max(elv);
    }
    if !points.is_empty() {
        s.elevation_y_max = maxelv + maxelv * 0.1;
    }
    s
}
