//! Aircraft side of AirScout, ported from `AirScout.Core`, `AirScout.Aircrafts` and
//! `AirScout.AircraftPositions`.
#![forbid(unsafe_code)]

mod cache;
mod category;
pub mod check;
pub mod convert;
pub mod crossing;
pub mod db;
pub mod enums;
mod json;
mod plane;
pub mod position_files;
pub mod positions;
pub mod units;
pub mod updater;

pub use cache::{update_if_newer, PlaneInfoCache};
pub use category::PlaneCategory;
pub use json::{EmsInfo, HistJson, PlaneJson, ScheduleInfo};
pub use plane::PlaneInfo;

/// Errors of the aircraft crate.
#[derive(thiserror::Error, Debug)]
pub enum AircraftError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("cannot parse aircraft data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("download failed: {0}")]
    Net(#[from] as_net::NetError),
    #[error("bad archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
}
