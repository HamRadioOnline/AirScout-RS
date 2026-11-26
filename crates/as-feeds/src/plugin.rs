//! The plugin contract: `IPlaneFeedPlugin`, `PlaneFeedPluginArgs`, `PlaneFeedPluginPlaneInfo`.

use as_aircraft::AltitudeProvenance;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Errors of a feed. HTTP, parsing and configuration boundaries report safe categories.
#[derive(thiserror::Error, Debug)]
pub enum FeedError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("request timed out")]
    Timeout,
    #[error("cannot parse feed data: {0}")]
    Parse(String),
    #[error("feed body exceeds {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration error: {0}")]
    Config(String),
}

/// `PlaneFeedPluginArgs`. Altitudes in metres.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeedArgs {
    pub app_dir: PathBuf,
    pub data_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub log_dir: PathBuf,
    pub database_dir: PathBuf,
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
    pub min_alt_m: i32,
    pub max_alt_m: i32,
    /// `MyLat`/`MyLon`: the query centre for `%MYLAT%`/`%MYLON%`. These values leave the
    /// machine (the ADSBOne default URL sends them to api.adsb.one). C# passes the user's
    /// station; callers should pass a point derived from the covered area unless the user
    /// agreed to share the station position (the CLI passes the area centre).
    pub my_lat: f64,
    pub my_lon: f64,
    pub dx_lat: f64,
    pub dx_lon: f64,
    pub keep_history: bool,
    /// Address of the host process’s own aircraft HTTP listener, if enabled.
    pub airscout_listener: Option<std::net::SocketAddr>,
}

/// `PlaneFeedPluginPlaneInfo`: one aircraft as a feed reports it. Altitude in ft, speed in
/// kts. Unset numbers are `f64::MIN`, an unset time is `None`, as in C#.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedPlane {
    pub time: Option<DateTime<Utc>>,
    pub components_valid_until: Option<DateTime<Utc>>,
    pub hex: String,
    pub call: String,
    pub reg: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub altitude_provenance: AltitudeProvenance,
    pub track: f64,
    pub speed_kts: f64,
    pub type_code: String,
    pub manufacturer: String,
    pub model: String,
    pub country: String,
    pub category: i32,
    pub from: String,
    pub to: String,
    pub vspeed: i32,
    pub ground: bool,
}

impl Default for FeedPlane {
    fn default() -> Self {
        Self {
            time: None,
            components_valid_until: None,
            hex: String::new(),
            call: String::new(),
            reg: String::new(),
            lat: f64::MIN,
            lon: f64::MIN,
            alt_ft: f64::MIN,
            altitude_provenance: AltitudeProvenance::Unknown,
            track: f64::MIN,
            speed_kts: f64::MIN,
            type_code: String::new(),
            manufacturer: String::new(),
            model: String::new(),
            country: String::new(),
            category: 0,
            from: String::new(),
            to: String::new(),
            vspeed: 0,
            ground: false,
        }
    }
}

/// Mapped observations and count-only diagnostics for one successful request.
#[derive(Debug, Default)]
pub struct FeedBatch {
    pub planes: Vec<FeedPlane>,
    pub decoded: usize,
    pub rejected: usize,
}

/// `IPlaneFeedPlugin`. Feeds run on the feed worker thread and use blocking I/O.
pub trait PlaneFeed: Send {
    /// Unique name, also the key of the feed's settings table.
    fn name(&self) -> &'static str;
    /// One-paragraph description for the UI.
    fn info(&self) -> &'static str;
    /// Text the user must accept before the feed may run, if any.
    fn disclaimer(&self) -> Option<&'static str>;
    /// False for feeds that need credentials or use non-public endpoints.
    fn enabled_by_default(&self) -> bool;
    /// Current settings as a TOML table (written back to `Settings::feed_settings`).
    fn settings(&self) -> toml::Table;
    /// Keys of [`Self::settings`] that hold credentials. The UI masks them; they never go into
    /// logs or status lines.
    fn secret_keys(&self) -> &'static [&'static str] {
        &[]
    }
    /// Load settings; missing keys keep their defaults.
    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError>;
    /// Height policy for a feed that reports individual component deadlines.
    fn component_feed_geometric_only(&self) -> Option<bool> {
        None
    }
    /// Called once before the first `get_planes`.
    fn start(&mut self, args: &FeedArgs) -> Result<(), FeedError>;
    /// One request: the aircraft currently reported.
    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError>;
    /// One request with count-only diagnostics from adapters that can isolate bad records.
    fn get_batch(&mut self, args: &FeedArgs) -> Result<FeedBatch, FeedError> {
        let planes = self.get_planes(args)?;
        Ok(FeedBatch {
            decoded: planes.len(),
            planes,
            rejected: 0,
        })
    }
    /// Called once when the worker stops.
    fn stop(&mut self, args: &FeedArgs) -> Result<(), FeedError>;
}
