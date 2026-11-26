//! Port of `AirScout.Core/BandSettings.cs`.

use as_geo::band::Band;
use std::collections::BTreeMap;

/// Per-band path settings (`BandSetting`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BandSettings {
    /// Effective earth radius factor (refraction).
    pub k_factor: f64,
    /// Required free fraction of the first Fresnel zone.
    pub f1_clearance: f64,
    /// Extra clearance above ground [m].
    pub ground_clearance: f64,
    /// Max. distance of a plane from the path [km].
    pub max_distance: f64,
    /// Max. antenna squint angle [deg]; 0 in the generated table.
    pub max_squint: f64,
    /// Max. antenna elevation [deg]; 0 in the generated table.
    pub max_elevation: f64,
}

impl Default for BandSettings {
    /// The C# `BandSetting()` constructor, used when a band has no row in the table.
    fn default() -> Self {
        Self {
            k_factor: 1.33,
            f1_clearance: 0.6,
            ground_clearance: 0.0,
            max_distance: 10.0,
            max_squint: f64::MAX,
            max_elevation: f64::MAX,
        }
    }
}

/// `Path_Default_*` values of `AirScout.Core/Properties/Settings.settings`.
const PATH_DEFAULT_K_FACTOR: f64 = 1.33;
const PATH_DEFAULT_F1_CLEARANCE: f64 = 0.6;
const PATH_DEFAULT_GROUND_CLEARANCE: f64 = 0.0;
const PATH_DEFAULT_MAX_DISTANCE: f64 = 10.0;
const PATH_DEFAULT_MAX_SQUINT: f64 = 0.0;
const PATH_DEFAULT_MAX_ELEVATION: f64 = 0.0;

/// `BandSettings.GenerateDefault`: one row per real band, with the V1.3.0.4 adjustments for the
/// lower bands.
pub fn default_band_table() -> BTreeMap<Band, BandSettings> {
    Band::all_except_none_and_all()
        .iter()
        .map(|&b| {
            let (k_factor, f1_clearance) = match b {
                Band::B50M | Band::B70M => (1.6, 0.1),
                Band::B144M => (1.5, 0.2),
                Band::B432M => (1.4, 0.4),
                _ => (PATH_DEFAULT_K_FACTOR, PATH_DEFAULT_F1_CLEARANCE),
            };
            (
                b,
                BandSettings {
                    k_factor,
                    f1_clearance,
                    ground_clearance: PATH_DEFAULT_GROUND_CLEARANCE,
                    max_distance: PATH_DEFAULT_MAX_DISTANCE,
                    max_squint: PATH_DEFAULT_MAX_SQUINT,
                    max_elevation: PATH_DEFAULT_MAX_ELEVATION,
                },
            )
        })
        .collect()
}
