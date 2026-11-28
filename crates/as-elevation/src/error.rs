/// Errors of the elevation crate.
#[derive(thiserror::Error, Debug)]
pub enum ElevationError {
    #[error("cannot parse elevation tile: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("cannot parse downloaded elevation tile at line {line}, column {column}")]
    ArchiveTile { line: usize, column: usize },
    #[error("corrupt elevation data: {0}")]
    Corrupt(String),
    #[error("invalid coordinate: lat {lat}, lon {lon}")]
    InvalidCoordinate { lat: f64, lon: f64 },
    #[error("invalid locator: {0}")]
    Locator(#[from] as_geo::maidenhead::LocatorError),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema version {actual} is newer than supported version {supported}")]
    SchemaAhead { actual: i64, supported: i64 },
    #[error("download failed: {0}")]
    Http(String),
    /// HTTP 404: no file on the server (for elevation squares: open sea).
    #[error("not found on server: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<as_db::DbError> for ElevationError {
    fn from(err: as_db::DbError) -> Self {
        match err {
            as_db::DbError::SchemaAhead { actual, supported } => {
                Self::SchemaAhead { actual, supported }
            }
            as_db::DbError::Sqlite(err) => Self::Db(err),
        }
    }
}

impl From<as_net::NetError> for ElevationError {
    fn from(e: as_net::NetError) -> Self {
        match e {
            as_net::NetError::Http(s) => ElevationError::Http(s),
            as_net::NetError::NotFound(s) => ElevationError::NotFound(s),
            as_net::NetError::Io(e) => ElevationError::Io(e),
        }
    }
}
