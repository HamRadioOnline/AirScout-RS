//! GUI-free per-tick pipeline: port of `MapDlg.UpdatePlanes` (4956-5229), `DrawPlanes` and the
//! alarm check. Input: the current plane reports and propagation paths; output: everything the
//! map, the plane list and the path chart show.

use as_aircraft::crossing::{get_nearest_planes, merge_active, nearest_plane};
use as_aircraft::enums::PathMode;
use as_aircraft::PlaneInfo;
use as_geo::latlon;
use as_propagation::PropagationPath;
use as_settings::Settings;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

pub struct TickInput<'a> {
    /// `CurrentTime`: single analysis instant for cache reads, crossing maths and tooltips.
    pub at: DateTime<Utc>,
    pub paths: &'a [PropagationPath],
    pub planes: Vec<PlaneInfo>,
    pub settings: &'a Settings,
    /// `PathMode`: the path chart is only filled in `Single` mode, and `per_path_potential`
    /// feeds the watchlist in `Multi` mode.
    pub mode: PathMode,
}

/// A plane in the path chart (distance from My over altitude).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartPlane {
    pub dist_km: f64,
    pub alt_m: f64,
    pub call: String,
}

#[derive(Clone, Debug, Default)]
pub struct TickResult {
    /// `ActivePlanes`, keyed by hex.
    pub active: BTreeMap<String, PlaneInfo>,
    /// `NearestPlane` of the **last** path only (the original resets it per path).
    pub nearest: Option<PlaneInfo>,
    /// Highest potential of the **last** path among planes passing the filter.
    pub highest_potential: i32,
    /// Highest potential per path (aligned with `TickInput::paths`) among the planes passing the
    /// altitude and category filter: the value C# stores in the watchlist item tooltip
    /// (`MapDlg.cs:5093-5229`, MULTI mode).
    pub per_path_potential: Vec<i32>,
    /// Active planes that pass the altitude and category filter.
    pub shown: Vec<PlaneInfo>,
    /// First shown plane with potential 100 or 75 closer than the alarm distance (only if the
    /// alarm is activated).
    pub alarm: Option<PlaneInfo>,
    /// Hex of every shown plane that meets the alarm condition (empty if the alarm is off).
    pub alarm_hexes: Vec<String>,
    pub chart_hi: Vec<ChartPlane>,
    pub chart_lo: Vec<ChartPlane>,
}

pub fn tick(input: TickInput) -> TickResult {
    let s = input.settings;
    let gui = &s.gui;
    let band = s.current_band_settings();
    let max_distance = band.max_distance;
    let max_squint = if band.max_squint.is_finite() && band.max_squint > 0.0 {
        Some(band.max_squint.to_radians())
    } else {
        None
    };
    let max_elevation = if band.max_elevation.is_finite() && band.max_elevation > 0.0 {
        Some(band.max_elevation.to_radians())
    } else {
        None
    };
    let floor_m = f64::from(s.planes_min_alt_m.max(gui.planes_filter_min_alt_m));
    let passes = |p: &PlaneInfo| {
        p.alt_m() >= floor_m
            && p.category >= gui.planes_filter_min_category
            && max_squint.is_none_or(|m| p.squint <= m)
            && max_elevation.is_none_or(|m| p.theta1.abs() <= m && p.theta2.abs() <= m)
    };
    let better = |a: &PlaneInfo, b: &PlaneInfo| {
        a.potential > b.potential || (a.potential == b.potential && a.int_qrb < b.int_qrb)
    };
    let planes = input.planes;
    let mut res = TickResult::default();

    // UpdatePlanes
    for path in input.paths {
        let found = get_nearest_planes(
            input.at,
            path,
            &planes,
            gui.planes_filter_max_circumcircle_km,
            max_distance,
            f64::from(s.planes_max_alt_m),
            s.planes_max_delta_m,
        );
        let mut per_path = 0;
        for p in found.iter().filter(|p| passes(p)) {
            if p.potential > per_path {
                per_path = p.potential;
            }
            if p.potential > res.highest_potential {
                res.highest_potential = p.potential;
            }
        }
        res.per_path_potential.push(per_path);
        if let Some(n) = nearest_plane(&found, max_distance)
            .filter(|p| passes(p))
            .cloned()
        {
            let replace = res.nearest.as_ref().is_none_or(|r| better(&n, r));
            if replace {
                res.nearest = Some(n);
            }
        }
        merge_active(&mut res.active, found);
    }

    // DrawPlanes.
    let single = input.mode == PathMode::Single;
    for p in res.active.values() {
        if !passes(p) {
            continue;
        }
        if gui.alarm_activate
            && (p.potential == 100 || p.potential == 75)
            && p.forward_qrb.is_some_and(|d| d < gui.alarm_distance_km)
        {
            if res.alarm.is_none() {
                res.alarm = Some(p.clone());
            }
            res.alarm_hexes.push(p.hex.clone());
        }
        if single {
            if let Some(ip) = p.int_point.as_ref().filter(|_| p.int_qrb <= max_distance) {
                let cp = ChartPlane {
                    dist_km: latlon::distance(s.my_lat, s.my_lon, ip.lat, ip.lon),
                    alt_m: p.alt_m(),
                    call: p.call.clone(),
                };
                if p.alt_diff > 0.0 {
                    res.chart_hi.push(cp);
                } else {
                    res.chart_lo.push(cp);
                }
            }
        }
        res.shown.push(p.clone());
    }
    res
}
