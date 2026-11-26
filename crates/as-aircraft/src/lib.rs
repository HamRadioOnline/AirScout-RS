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
mod motion;
mod plane;
pub mod position_files;
pub mod positions;
pub mod units;
pub mod updater;

pub use cache::{update_if_newer, PlaneInfoCache};
pub use category::PlaneCategory;
pub use json::{EmsInfo, HistJson, PlaneJson, ScheduleInfo};
pub use plane::{AltitudeProvenance, PlaneInfo};

/// Errors of the aircraft crate.
#[derive(thiserror::Error, Debug)]
pub enum AircraftError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema version {actual} is newer than supported version {supported}")]
    SchemaAhead { actual: i64, supported: i64 },
    #[error("cannot parse aircraft data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid downloaded archive")]
    ArchiveMalformed,
    #[error("cannot parse downloaded archive at line {line}, column {column}")]
    ArchiveData { line: usize, column: usize },
    #[error("download failed: {0}")]
    Net(#[from] as_net::NetError),
    #[error("bad archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
}

impl From<as_db::DbError> for AircraftError {
    fn from(err: as_db::DbError) -> Self {
        match err {
            as_db::DbError::SchemaAhead { actual, supported } => {
                Self::SchemaAhead { actual, supported }
            }
            as_db::DbError::Sqlite(err) => Self::Db(err),
        }
    }
}
