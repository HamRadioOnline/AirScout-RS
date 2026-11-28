//! Port of the C# `ElevationTileGenerator`: `.loc` elevation tiles from SRTM3, SRTM1 and
//! GLOBE source files, in the layout of the served tile tree, the square zips and the tile
//! catalogue of that tree, and the SRTM compare, downsample and square preview tools.
//!
//! The tool reads local files only and makes no network request.
#![forbid(unsafe_code)]

pub mod hgt;
pub mod pack;
pub mod preview;
pub mod source;
pub mod tiles;

pub use hgt::{diff_bytes, diff_stats, downsample, histogram_csv, DiffStats};
pub use pack::{catalogue_text, scan_catalogue, write_catalogue, zip_squares, CatFrom, ZipReport};
pub use preview::square_rgba;
pub use source::{identify, Source, SourceKind};
pub use tiles::{
    globe_block_square, globe_indices, globe_tile, globe_tile_index, loc_path, srtm_tiles,
    write_loc, GlobeIndex, Outcome,
};
