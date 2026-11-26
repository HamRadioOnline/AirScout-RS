//! Elevation model of AirScout-RS, ported from `ScoutBase.Elevation`.
#![forbid(unsafe_code)]

mod cache;
mod catalogue;
mod error;
mod horizon;
mod loc_json;
mod minmax;
mod model;
mod obstruction;
mod palette;
mod path;
mod prepare;
mod store;
mod tile;
mod updater;

pub use catalogue::{
    get_locs_from_rect, read_files_cat, read_files_cat_where, read_files_zip, user_agent, Capped,
    Catalogue, DEFAULT_BASE_URL, MAX_CATALOGUE_BYTES,
};
pub use error::ElevationError;
pub use horizon::ElevationHorizon;
pub use loc_json::{framework_number_text, loc_date_text};
pub use minmax::ElvMinMaxInfo;
pub use model::{ElevationModel, ELV_MISSING, TILE_MISSING};
pub use obstruction::LocalObstruction;
pub use palette::{elevation_color, palette_color};
pub use path::{Coverage, ElevationPath};
pub use prepare::{check_area, download_area, prepare_area, AreaStatus};
pub use store::{DbStatus, ElevationStore, DEFAULT_CACHE_BUDGET_BYTES};
pub use tile::ElevationTile;
pub use updater::{update_square, UpdateProgress};
