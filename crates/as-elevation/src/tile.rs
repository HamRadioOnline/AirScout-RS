//! Port of `ElevationTile.cs` (`ElevationTileDesignator`): one 6-character Maidenhead square
//! (1/24 deg lat x 2/24 deg lon) of elevation values.

use crate::{ElevationError, ELV_MISSING};
use as_geo::latlon::GRect;
use as_geo::maidenhead::{self, PositionInRectangle};
use chrono::{DateTime, Utc};

/// Elevation tile. `elv` is column-major (`elv[col * rows + row]`), matching C# `Elv[col, row]`:
/// columns run along longitude, rows along latitude.
#[derive(Clone, Debug, PartialEq)]
pub struct ElevationTile {
    pub index: String,
    pub min_elv: i32,
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_elv: i32,
    pub max_lat: f64,
    pub max_lon: f64,
    pub rows: usize,
    pub columns: usize,
    pub elv: Vec<i16>,
    pub last_updated: DateTime<Utc>,
    base_lat: f64,
    base_lon: f64,
    step_lat: f64,
    step_lon: f64,
    bounds: GRect,
}

/// JSON shape of a `.loc` file.
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LocJson {
    tile_index: String,
    #[serde(default)]
    min_elv: i32,
    #[serde(default)]
    min_lat: f64,
    #[serde(default)]
    min_lon: f64,
    #[serde(default)]
    max_elv: i32,
    #[serde(default)]
    max_lat: f64,
    #[serde(default)]
    max_lon: f64,
    rows: usize,
    columns: usize,
    elv: Vec<Vec<i16>>,
    #[serde(default = "epoch")]
    last_updated: DateTime<Utc>,
}

fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

impl ElevationTile {
    /// Build a tile; computes base point, step widths and bounds like the C# setters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: &str,
        min_elv: i32,
        min_lat: f64,
        min_lon: f64,
        max_elv: i32,
        max_lat: f64,
        max_lon: f64,
        rows: usize,
        columns: usize,
        elv: Vec<i16>,
        last_updated: DateTime<Utc>,
    ) -> Result<Self, ElevationError> {
        if rows == 0 || columns == 0 || elv.len() != rows * columns {
            return Err(ElevationError::Corrupt(format!(
                "tile {index}: {} values for {rows}x{columns}",
                elv.len()
            )));
        }
        let base_lat = maidenhead::lat_from_loc(index, PositionInRectangle::BottomLeft)?;
        let base_lon = maidenhead::lon_from_loc(index, PositionInRectangle::BottomLeft)?;
        let bounds = maidenhead::bounds_from_loc(index)?;
        Ok(Self {
            index: index.to_string(),
            min_elv,
            min_lat,
            min_lon,
            max_elv,
            max_lat,
            max_lon,
            rows,
            columns,
            elv,
            last_updated,
            base_lat,
            base_lon,
            step_lat: 1.0 / 24.0 / rows as f64,
            step_lon: 2.0 / 24.0 / columns as f64,
            bounds,
        })
    }

    /// Parse a `.loc` JSON file (as served in the original square zips).
    pub fn from_loc_json(json: &str) -> Result<Self, ElevationError> {
        let j: LocJson = serde_json::from_str(json.trim_start_matches('\u{feff}'))?;
        if j.elv.len() != j.columns || j.elv.iter().any(|c| c.len() != j.rows) {
            return Err(ElevationError::Corrupt(format!(
                "tile {}: Elv shape does not match {} rows x {} columns",
                j.tile_index, j.rows, j.columns
            )));
        }
        let elv = j.elv.into_iter().flatten().collect();
        Self::new(
            &j.tile_index,
            j.min_elv,
            j.min_lat,
            j.min_lon,
            j.max_elv,
            j.max_lat,
            j.max_lon,
            j.rows,
            j.columns,
            elv,
            j.last_updated,
        )
    }

    /// `GetElevation`: value of the grid cell containing (lat, lon), [`ELV_MISSING`] outside.
    /// Index computation truncates toward zero exactly like the C# `(int)` cast.
    pub fn get_elevation(&self, lat: f64, lon: f64) -> i16 {
        if lat.is_nan() || lon.is_nan() {
            return ELV_MISSING;
        }
        let i = ((lat - self.base_lat) / self.step_lat) as i64;
        let j = ((lon - self.base_lon) / self.step_lon) as i64;
        if i < 0 || j < 0 || i >= self.rows as i64 || j >= self.columns as i64 {
            return ELV_MISSING;
        }
        self.elv[j as usize * self.rows + i as usize]
    }

    /// Bottom-left latitude.
    pub fn base_lat(&self) -> f64 {
        self.base_lat
    }
    /// Bottom-left longitude.
    pub fn base_lon(&self) -> f64 {
        self.base_lon
    }
    /// Locator bounds of the tile.
    pub fn bounds(&self) -> GRect {
        self.bounds
    }
    /// True if (lat, lon) lies inside the bounds (inclusive, as the C# cache check).
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.bounds.min_lat
            && lat <= self.bounds.max_lat
            && lon >= self.bounds.min_lon
            && lon <= self.bounds.max_lon
    }

    /// Serialise the elevation grid: little-endian i16, column-major.
    pub fn to_blob(&self) -> Vec<u8> {
        self.elv.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Rebuild a tile from [`to_blob`](Self::to_blob) output (metadata defaults to zero).
    pub fn from_blob(
        index: &str,
        rows: usize,
        columns: usize,
        blob: &[u8],
    ) -> Result<Self, ElevationError> {
        if blob.len() != rows * columns * 2 {
            return Err(ElevationError::Corrupt(format!(
                "tile {index}: blob has {} bytes, expected {}",
                blob.len(),
                rows * columns * 2
            )));
        }
        let elv = blob
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        Self::new(index, 0, 0.0, 0.0, 0, 0.0, 0.0, rows, columns, elv, epoch())
    }
}
