//! Application settings as TOML in the OS config directory. Defaults come from the original
//! `AirScout/Properties/Settings.settings` and `AirScout.Core/Properties/Settings.settings`.
//! Later stages add fields with defaults; unknown fields in a file are ignored.
#![forbid(unsafe_code)]

mod analysis;
mod band_settings;
pub mod cat;
mod gui;
mod network;
mod profile;
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
pub use profile::{canonicalize_feed_table, format_profile, SettingsHost, SettingsVersion};
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

/// Write `text` through an exclusive staging file beside `path`, syncing before
/// replacement. The staging file is owner-only on Unix.
pub fn write_private_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    write_private_atomic_with(path, |file| file.write_all(text.as_bytes()))
}

fn write_private_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::fs;
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let (staging, mut file) = loop {
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let staging = parent.join(format!(
            ".airscout-settings.{}.{serial}.tmp",
            std::process::id()
        ));
        match options.open(&staging) {
            Ok(file) => break (staging, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let written = write(&mut file).and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| fs::rename(&staging, path));
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result
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
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Background")
            .field("update_on_startup", &self.update_on_startup)
            .field("update_periodically", &self.update_periodically)
            .field("update_period_min", &self.update_period_min)
            .field("calculations_enable", &self.calculations_enable)
            .finish_non_exhaustive()
    }
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
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

impl std::fmt::Debug for SpecLab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecLab")
            .field("enabled", &self.enabled)
            .field("f1", &self.f1)
            .field("f2", &self.f2)
            .field("update_s", &self.update_s)
            .finish_non_exhaustive()
    }
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

/// Per-feed settings tables. `Debug` omits arbitrary keys and values.
/// Serialized exactly like a plain TOML table.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FeedSettings(pub toml::Table);

impl std::fmt::Debug for FeedSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedSettings")
            .field("count", &self.0.len())
            .finish_non_exhaustive()
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
    /// Name of the second plane feed slot (`Planes_PlaneFeed2`); "" means none.
    pub feed2: String,
    /// Name of the third plane feed slot (`Planes_PlaneFeed3`); "" means none.
    pub feed3: String,
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
            feed2: String::new(),
            feed3: String::new(),
            planes_interval_s: 60,
            planes_position_ttl_min: 5,
            planes_keep_history: false,
            planes_extended_plausibility: true,
            planes_max_error_dist: 30.0,
            planes_max_delta_m: 1000.0,
            band_settings: default_band_table(),
            feed_settings: FeedSettings::for_host(SettingsHost::Desktop),
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
        let version = SettingsVersion::read(&raw).ok_or_else(|| SettingsError::Parse {
            path: path.into(),
            line: 0,
            column: 0,
        })?;
        if version == SettingsVersion::Legacy && !raw.contains_key("band_settings") {
            settings.band_settings = band_settings::legacy_band_table();
        }
        if !raw.contains_key("feed_settings") {
            settings.feed_settings = FeedSettings::default();
        }
        settings
            .feed_settings
            .resolve(SettingsHost::Desktop, version);
        Ok(settings)
    }

    /// Resolve and save the versioned profile through a private atomic replacement.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let io = |source| SettingsError::Io {
            path: path.into(),
            source,
        };
        let text = format_profile(self, SettingsHost::Desktop)?;
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

    /// The three feed slot names in order (`feed`, `feed2`, `feed3`).
    pub fn feed_slots(&self) -> [&str; 3] {
        [self.feed.as_str(), self.feed2.as_str(), self.feed3.as_str()]
    }

    /// The distinct non-empty feed slots, in order. Empty slots (and the `[none]` sentinel)
    /// are skipped, and a slot equal to an earlier one is ignored: one worker runs per
    /// distinct feed.
    pub fn distinct_feeds(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        self.feed_slots()
            .into_iter()
            .filter(|n| !n.is_empty() && *n != "[none]" && seen.insert(*n))
            .collect()
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

#[cfg(test)]
mod private_save_tests {
    use super::*;
    use std::fs;
    use std::io::{self, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn finding_381_paused_writer_retains_its_exclusive_staging_file() {
        for fail in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("settings.toml");
            let (staged, staged_rx) = mpsc::channel();
            let (finish, finish_rx) = mpsc::channel();

            let writer_path = path.clone();
            let writer = std::thread::spawn(move || {
                write_private_atomic_with(&writer_path, |file| {
                    file.write_all(b"value = 1\n")?;
                    staged.send(()).unwrap();
                    finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(())
                })
            });

            staged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let second = write_private_atomic_with(&path, |file| {
                file.write_all(b"value = 2\n")?;
                if fail {
                    return Err(io::Error::other("synthetic interruption"));
                }
                Ok(())
            });
            assert_eq!(second.is_err(), fail);
            if !fail {
                assert_eq!(fs::read_to_string(&path).unwrap(), "value = 2\n");
            }
            finish.send(()).unwrap();
            writer.join().unwrap().unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "value = 1\n");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
