//! `ElvMinMaxInfo` and `ElevationTileFindMinMaxInfo` / `GetMaxElvLoc` (ElevationDatabase.cs): the
//! minimum and maximum of a tile and where they are, without loading the elevation grid.

use crate::{ElevationError, ElevationModel, ElevationStore};
use as_geo::maidenhead;
use rusqlite::{params, OptionalExtension};

/// Minimum and maximum elevation of a tile with their positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElvMinMaxInfo {
    pub min_elv: i32,
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_elv: i32,
    pub max_lat: f64,
    pub max_lon: f64,
}

impl ElevationStore {
    /// `ElevationTileFindMinMaxInfo(tileindex, model)`: `None` if the tile is not stored.
    pub fn min_max_info(
        &self,
        model: ElevationModel,
        index: &str,
    ) -> Result<Option<ElvMinMaxInfo>, ElevationError> {
        self.with_conn(model, |conn| {
            Ok(conn
                .query_row(
                    "SELECT MinElv, MinLat, MinLon, MaxElv, MaxLat, MaxLon FROM Elevation
                     WHERE TileIndex = ?1",
                    params![index],
                    |r| {
                        Ok(ElvMinMaxInfo {
                            min_elv: r.get(0)?,
                            min_lat: r.get(1)?,
                            min_lon: r.get(2)?,
                            max_elv: r.get(3)?,
                            max_lat: r.get(4)?,
                            max_lon: r.get(5)?,
                        })
                    },
                )
                .optional()?)
        })
    }

    /// `GetMaxElvLoc(loc, model)`: `None` for an invalid locator, one that is not 6 characters
    /// long, or a tile that is not stored. The locator is upper-cased.
    pub fn max_elv_loc(
        &self,
        model: ElevationModel,
        loc: &str,
    ) -> Result<Option<ElvMinMaxInfo>, ElevationError> {
        if !maidenhead::check(loc) || loc.len() != 6 {
            return Ok(None);
        }
        self.min_max_info(model, &loc.to_uppercase())
    }
}
