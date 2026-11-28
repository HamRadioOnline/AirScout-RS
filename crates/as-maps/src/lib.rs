//! Map tiles of AirScout-RS: the GMap `.gmdb` tile store (port of `ScoutBase.Maps.MapDatabase`),
//! the GMap provider `DbId`s and the tile list of the map preloader (`AirScout/MapPreloader.cs`).
//!
//! No network code lives here.
#![forbid(unsafe_code)]

mod store;
mod tiles;

pub use store::{is_png_or_jpeg, MapsError, TileInfo, TileKey, TileStore, MAX_TILE_BYTES};
pub use tiles::{
    db_id, lat2tiley, long2tilex, preload_tile_count, preload_tiles, DBID_OCM_LANDSCAPE,
    DBID_OPENTOPOMAP, DBID_OSM, MIRROR_URL,
};
