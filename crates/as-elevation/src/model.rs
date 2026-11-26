//! `ELEVATIONMODEL` and the model-specific constants of `ElevationDatabase.cs`.

/// Value of the "elevation data missing" flag.
pub const ELV_MISSING: i16 = -32768;
/// Value of the "elevation tile missing" flag.
pub const TILE_MISSING: i16 = -32767;

/// Elevation model (`ELEVATIONMODEL`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(i32)]
pub enum ElevationModel {
    None = 0,
    Globe = 1,
    Srtm3 = 2,
    Srtm1 = 3,
    Aster3 = 4,
    Aster1 = 5,
}

impl ElevationModel {
    /// All real models.
    pub const ALL: [ElevationModel; 5] = [
        ElevationModel::Globe,
        ElevationModel::Srtm3,
        ElevationModel::Srtm1,
        ElevationModel::Aster3,
        ElevationModel::Aster1,
    ];

    /// `GetDefaultStepWidth` in m.
    pub fn default_step_width_m(self) -> f64 {
        match self {
            ElevationModel::Srtm1 | ElevationModel::Aster1 => 30.0,
            ElevationModel::Srtm3 | ElevationModel::Aster3 => 90.0,
            _ => 1000.0,
        }
    }

    /// `GetElevationPathAveragePeriod`.
    pub fn average_period(self) -> usize {
        match self {
            ElevationModel::Aster1 | ElevationModel::Aster3 => 5,
            _ => 0,
        }
    }

    /// Directory / URL name.
    pub fn dir_name(self) -> &'static str {
        match self {
            ElevationModel::None => "NONE",
            ElevationModel::Globe => "GLOBE",
            ElevationModel::Srtm3 => "SRTM3",
            ElevationModel::Srtm1 => "SRTM1",
            ElevationModel::Aster3 => "ASTER3",
            ElevationModel::Aster1 => "ASTER1",
        }
    }

    /// The `Elevation_*_Copyright` text of the original settings (`Settings.settings:422, :434,
    /// :455, :610, :635`), verbatim. `None` has none.
    pub fn copyright(self) -> &'static str {
        match self {
            ElevationModel::None => "",
            ElevationModel::Globe => include_str!("copyright/globe.txt"),
            ElevationModel::Srtm3 => include_str!("copyright/srtm3.txt"),
            ElevationModel::Srtm1 => include_str!("copyright/srtm1.txt"),
            ElevationModel::Aster3 => include_str!("copyright/aster3.txt"),
            ElevationModel::Aster1 => include_str!("copyright/aster1.txt"),
        }
    }

    /// Database file name (`globe.db3`, ...).
    pub fn db_file_name(self) -> String {
        format!("{}.db3", self.dir_name().to_lowercase())
    }
}
