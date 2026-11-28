//! Station database (locations, QRV info), callsign checks and updater, ported from
//! `ScoutBase.Stations` and `ScoutBase.Core/Callsign.cs`.
#![forbid(unsafe_code)]

pub mod callsign;
mod db;
mod location;
mod qrv;
mod updater;

pub use db::{StationDb, StationEditCounts, INTERNAL_TABLES, QRV_USER_EDIT_MARKER};
pub use location::{locations_from_json, GeoSource, Location};
pub use qrv::{default_antenna_gain, default_antenna_height, default_power, qrvs_from_json, Qrv};
pub use updater::{update_stations, UpdateStats, DEFAULT_UPDATE_URL};

/// Errors of the station crate.
#[derive(thiserror::Error, Debug)]
pub enum StationError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema version {actual} is newer than supported version {supported}")]
    SchemaAhead { actual: i64, supported: i64 },
    #[error("cannot parse station data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid downloaded archive")]
    ArchiveMalformed,
    #[error("expanded station archive member exceeds {limit} bytes")]
    ArchiveTooLarge { limit: u64 },
    #[error("invalid station coordinates")]
    InvalidCoordinates,
    #[error("cannot parse downloaded archive at line {line}, column {column}")]
    ArchiveData { line: usize, column: usize },
    #[error("download failed: {0}")]
    Net(#[from] as_net::NetError),
    #[error("bad archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid locator for QRV entry: {0:?} (needs 6 characters)")]
    InvalidLocator(String),
    #[error("cannot clear internal table {0:?}")]
    InternalTable(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<as_db::DbError> for StationError {
    fn from(err: as_db::DbError) -> Self {
        match err {
            as_db::DbError::SchemaAhead { actual, supported } => {
                Self::SchemaAhead { actual, supported }
            }
            as_db::DbError::Sqlite(err) => Self::Db(err),
        }
    }
}

/// Numbers in the server JSON (Newtonsoft `FloatFormatHandling.String`): numbers or "NaN" etc.
pub(crate) fn de_f64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum N {
        F(f64),
        S(String),
    }
    match <N as serde::Deserialize>::deserialize(d)? {
        N::F(f) => Ok(f),
        N::S(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            _ => s.parse().map_err(serde::de::Error::custom),
        },
    }
}

/// Timestamps are stored as nanoseconds since the epoch (like the elevation store).
pub(crate) fn to_ns(t: chrono::DateTime<chrono::Utc>) -> i64 {
    t.timestamp_nanos_opt().unwrap_or(if t.timestamp() < 0 {
        i64::MIN
    } else {
        i64::MAX
    })
}

pub(crate) fn from_ns(ns: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(ns)
}
