//! Application settings as TOML in the OS config directory. Defaults come from the original
//! `AirScout/Properties/Settings.settings` and `AirScout.Core/Properties/Settings.settings`.
//! Later stages add fields with defaults; unknown fields in a file are ignored.
#![forbid(unsafe_code)]

mod analysis;
mod band_settings;
pub mod cat;
mod gui;
mod network;
mod secret;
mod track;
mod watchlist;
mod web;

pub use analysis::Analysis;
pub use band_settings::{default_band_table, BandSettings};
pub use cat::{Cat, CatEngine, FlowControl, Parity, StopBits};
pub use gui::{
    Area, GuiSettings, InfoWin, LOCATOR_AUTO_LENGTH, LOCATOR_MAX_LENGTH, MAP_PRELOADER_MAX_ZOOM,
};
pub use network::Server;
pub use secret::Secret;
pub use track::{Doppler, DopplerStrategy, Track, TrackDde, TrackFile, TrackSerial, TrackUdp};
pub use watchlist::{Watchlist, WatchlistItem};
pub use web::Web;

use as_elevation::ElevationModel;
use as_geo::band::Band;
use as_geo::latlon::EARTH_RADIUS_KM;
use as_geo::maidenhead;
use chrono::{DateTime, TimeZone, Utc};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors of loading or saving settings.
#[derive(thiserror::Error, Debug)]
pub enum SettingsError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The toml message is not kept: it quotes the broken line or a value, and the file can
    /// hold a credential. `line` and `column` are 1-based; both are 0 when toml gives no place.
    #[error("cannot parse {path} at line {line}, column {column} (the text is not shown: it may hold a password)")]
    Parse {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    #[error("cannot write settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Write `text` to `path` atomically: `<path>.tmp` (created owner-only, mode 0600, on Unix, as
/// the file can hold feed credentials), `sync_all`, then rename. A stale `<path>.tmp` is
/// replaced; the temp file is removed if the rename fails. Creates the parent directory.
pub fn write_private_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // a stale file keeps its old mode when reopened, so remove it first
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let written = opts.open(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 1-based line and column (in characters) of byte `offset` in `text`. An offset past the end
/// or inside a character counts up to the last whole character before it.
fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}

/// Enabled elevation sources (`ElevationModel_*_Enabled` flags).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ElevationEnabled {
    pub globe: bool,
    pub srtm3: bool,
    pub srtm1: bool,
    pub aster3: bool,
    pub aster1: bool,
}

impl Default for ElevationEnabled {
    fn default() -> Self {
        Self {
            globe: true,
            srtm3: false,
            srtm1: false,
            aster3: false,
            aster1: false,
        }
    }
}

impl ElevationEnabled {
    /// Best enabled model: ASTER1 > ASTER3 > SRTM1 > SRTM3 > GLOBE, else `None`
    /// (`SetElevationModel`, MapDlg.cs).
    pub fn effective_model(&self) -> ElevationModel {
        if self.aster1 {
            ElevationModel::Aster1
        } else if self.aster3 {
            ElevationModel::Aster3
        } else if self.srtm1 {
            ElevationModel::Srtm1
        } else if self.srtm3 {
            ElevationModel::Srtm3
        } else if self.globe {
            ElevationModel::Globe
        } else {
            ElevationModel::None
        }
    }

    /// Only `m` enabled (`None`: the defaults, GLOBE).
    pub fn only(m: ElevationModel) -> Self {
        let mut e = Self {
            globe: false,
            srtm3: false,
            srtm1: false,
            aster3: false,
            aster1: false,
        };
        match m {
            ElevationModel::None => return Self::default(),
            ElevationModel::Globe => e.globe = true,
            ElevationModel::Srtm3 => e.srtm3 = true,
            ElevationModel::Srtm1 => e.srtm1 = true,
            ElevationModel::Aster3 => e.aster3 = true,
            ElevationModel::Aster1 => e.aster1 = true,
        }
        e
    }

    pub fn is_enabled(&self, m: ElevationModel) -> bool {
        match m {
            ElevationModel::None => false,
            ElevationModel::Globe => self.globe,
            ElevationModel::Srtm3 => self.srtm3,
            ElevationModel::Srtm1 => self.srtm1,
            ElevationModel::Aster3 => self.aster3,
            ElevationModel::Aster1 => self.aster1,
        }
    }
}

/// Background workers (database updates, calculations).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Background {
    pub update_on_startup: bool,
    pub update_periodically: bool,
    /// Update period [min].
    pub update_period_min: u64,
    pub calculations_enable: bool,
    pub stations_url: String,
    pub aircraft_url: String,
    /// Base URL of the map tile mirror the preloader reads (`<url><z>/<x>/<y>.png`, OpenStreetMap
    /// tiles only); not shown in the Options, like `stations_url`.
    pub map_mirror_url: String,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            update_on_startup: true,
            update_periodically: false,
            update_period_min: 60,
            calculations_enable: true,
            stations_url: as_stations::DEFAULT_UPDATE_URL.into(),
            aircraft_url: as_aircraft::updater::DEFAULT_UPDATE_URL.into(),
            map_mirror_url: as_maps::MIRROR_URL.into(),
        }
    }
}

/// SpecLab (spectrum) integration.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpecLab {
    pub enabled: bool,
    pub url: String,
    pub file_name: String,
    pub f1: u32,
    pub f2: u32,
    /// Update interval [s].
    pub update_s: u64,
}

impl Default for SpecLab {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "http://localhost:10080/".into(),
            file_name: "_fft_expt.json".into(),
            f1: 400,
            f2: 1600,
            update_s: 1,
        }
    }
}

/// Time source: system clock (online) or a fixed offline time.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TimeSettings {
    pub online: bool,
    pub offline: DateTime<Utc>,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self {
            online: true,
            offline: Utc.with_ymd_and_hms(2015, 2, 25, 0, 0, 0).unwrap(),
        }
    }
}

/// Per-feed settings tables. They hold feed credentials, so `Debug` prints only the feed names.
/// Serialized exactly like a plain TOML table.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FeedSettings(pub toml::Table);

impl std::fmt::Debug for FeedSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

impl std::ops::Deref for FeedSettings {
    type Target = toml::Table;
    fn deref(&self) -> &toml::Table {
        &self.0
    }
}

impl std::ops::DerefMut for FeedSettings {
    fn deref_mut(&mut self) -> &mut toml::Table {
        &mut self.0
    }
}

/// Settings used by the core (stage 1).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub my_call: String,
    pub my_lat: f64,
    pub my_lon: f64,
    /// Antenna height above ground [m].
    pub my_height_m: f64,
    pub dx_call: String,
    pub dx_lat: f64,
    pub dx_lon: f64,
    /// Antenna height above ground [m].
    pub dx_height_m: f64,
    pub band: Band,
    pub elevation_model: ElevationModel,
    /// `Path_NearFieldSuppression` [m].
    pub path_near_field_suppression_m: f64,
    /// `Path_MaxLength` [km].
    pub path_max_length_km: f64,
    /// `Path_BestCaseElevation`.
    pub path_best_case_elevation: bool,
    /// `Planes_MinAlt` [m].
    pub planes_min_alt_m: i32,
    /// `Planes_MaxAlt` [m].
    pub planes_max_alt_m: i32,
    /// Name of the active plane feed. Default is the key-free OpenSky feed.
    pub feed: String,
    /// `Planes_Interval` [s]: pause between two feed requests.
    pub planes_interval_s: u64,
    /// `Planes_Position_TTL` [min]: how long a reported position stays valid.
    pub planes_position_ttl_min: i64,
    /// `Planes_KeepHistory`: store every position in the position database.
    pub planes_keep_history: bool,
    /// `Planes_ExtendedPlausibilityCheck_Enabled`.
    pub planes_extended_plausibility: bool,
    /// `Planes_ExtendedPlausibilityCheck_MaxErrorDist` [km].
    pub planes_max_error_dist: f64,
    /// `Planes_MaxDelta` [m]: altitude tolerance for crossing prediction.
    pub planes_max_delta_m: f64,
    /// `Path_Band_Settings`.
    pub band_settings: BTreeMap<Band, BandSettings>,
    /// Per-feed settings, keyed by feed name. Each feed reads its own table.
    pub feed_settings: FeedSettings,
    /// GUI settings (stage 3a).
    pub gui: GuiSettings,
    /// Map area of interest (stage 3a).
    pub area: Area,
    /// Enabled elevation sources (stage 3b). `elevation_model` is kept for file
    /// compatibility: a stage-3a file without this table enables that model alone.
    pub elevation_enabled: ElevationEnabled,
    pub watchlist: Watchlist,
    pub watchlist_max_count: usize,
    /// `Location_RestrictToAreaOfInterest`.
    pub location_restrict_to_area: bool,
    pub background: Background,
    pub speclab: SpecLab,
    pub time: TimeSettings,
    /// History and crossing analysis (stage 3c).
    pub analysis: Analysis,
    pub track: Track,
    pub doppler: Doppler,
    /// Win-Test UDP server (stage 4a).
    pub server: Server,
    /// `Watchlist_SyncWithKST` (`Settings.settings:383`).
    pub watchlist_sync_with_kst: bool,
    /// CAT rig control (stage 4b).
    pub cat: Cat,
    /// Names of feeds whose disclaimer the user accepted.
    pub feeds_disclaimer_accepted: Vec<String>,
    /// Embedded web server (stage 4d).
    pub web: Web,
    /// Show the first-run wizard. A file lacking the key reads `false`, so an existing
    /// installation never sees the wizard; `Settings::default()` sets `true`.
    #[serde(default)]
    pub first_run: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            my_call: "DL2ALF".into(),
            my_lat: 50.937067,
            my_lon: 10.68327,
            my_height_m: 14.0,
            dx_call: "GB3MHZ".into(),
            dx_lat: 52.05626084,
            dx_lon: 1.28022909,
            dx_height_m: 30.0,
            band: Band::B1_2G,
            elevation_model: ElevationModel::None,
            path_near_field_suppression_m: 100.0,
            path_max_length_km: 1000.0,
            path_best_case_elevation: true,
            planes_min_alt_m: 5000,
            planes_max_alt_m: 12200,
            feed: "OpenSky".into(),
            planes_interval_s: 60,
            planes_position_ttl_min: 5,
            planes_keep_history: false,
            planes_extended_plausibility: true,
            planes_max_error_dist: 30.0,
            planes_max_delta_m: 1000.0,
            band_settings: default_band_table(),
            feed_settings: FeedSettings::default(),
            gui: GuiSettings::default(),
            area: Area::default(),
            elevation_enabled: ElevationEnabled::default(),
            watchlist: Watchlist::default(),
            watchlist_max_count: 1000,
            location_restrict_to_area: true,
            background: Background::default(),
            speclab: SpecLab::default(),
            time: TimeSettings::default(),
            analysis: Analysis::default(),
            track: Track::default(),
            doppler: Doppler::default(),
            server: Server::default(),
            watchlist_sync_with_kst: false,
            cat: Cat::default(),
            feeds_disclaimer_accepted: Vec::new(),
            web: Web::default(),
            first_run: true,
        }
    }
}

impl Settings {
    /// Remember `call` as most recently used: front insert, no duplicates, at most 10.
    pub fn my_calls_push(&mut self, call: &str) {
        let calls = &mut self.gui.my_calls;
        calls.retain(|c| c != call);
        calls.insert(0, call.to_owned());
        calls.truncate(10);
    }

    /// `<config dir>/settings.toml`, e.g. `~/Library/Application Support/eu.airscout.AirScout-RS/`.
    /// Falls back to the current directory if the OS has no config directory.
    pub fn default_path() -> PathBuf {
        let p = directories::ProjectDirs::from("eu", "airscout", "AirScout-RS")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_default()
            .join("settings.toml");
        // absolute once (the fallback is relative to the cwd)
        std::path::absolute(&p).unwrap_or(p)
    }

    /// Load `path`; a missing file gives the defaults. A file that cannot be parsed is an error
    /// and is left untouched.
    pub fn load_or_default(path: &Path) -> Result<Settings, SettingsError> {
        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(source) => {
                return Err(SettingsError::Io {
                    path: path.into(),
                    source,
                })
            }
        };
        let parse_err = |e: toml::de::Error| {
            let (line, column) = e.span().map_or((0, 0), |r| line_column(&s, r.start));
            SettingsError::Parse {
                path: path.into(),
                line,
                column,
            }
        };
        let mut settings: Settings = toml::from_str(&s).map_err(parse_err)?;
        // Stage 3a computed everything with its single `elevation_model` (`None` meant GLOBE);
        // a file without `elevation_enabled` keeps that model as the only enabled one.
        let raw: toml::Table = toml::from_str(&s).map_err(parse_err)?;
        if !raw.contains_key("elevation_enabled") {
            settings.elevation_enabled = ElevationEnabled::only(settings.elevation_model);
        }
        Ok(settings)
    }

    /// Save atomically: write `<path>.tmp`, then rename over `path`.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let io = |source| SettingsError::Io {
            path: path.into(),
            source,
        };
        let text = toml::to_string_pretty(self)?;
        write_private_atomic(path, &text).map_err(io)
    }

    /// Band settings of the current band; the C# `BandSetting()` defaults if it has no row.
    pub fn current_band_settings(&self) -> BandSettings {
        self.band_settings
            .get(&self.band)
            .copied()
            .unwrap_or_default()
    }

    /// Equivalent earth radius [km] for the current band.
    pub fn radius_km(&self) -> f64 {
        EARTH_RADIUS_KM * self.current_band_settings().k_factor
    }

    /// 6-character locator of my position (`None` if the position is invalid).
    pub fn my_loc(&self) -> Option<String> {
        maidenhead::loc_from_lat_lon(self.my_lat, self.my_lon, false, 3, true)
            .ok()
            .flatten()
    }

    /// 6-character locator of the DX position (`None` if the position is invalid).
    pub fn dx_loc(&self) -> Option<String> {
        maidenhead::loc_from_lat_lon(self.dx_lat, self.dx_lon, false, 3, true)
            .ok()
            .flatten()
    }
}
