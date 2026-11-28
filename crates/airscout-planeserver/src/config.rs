//! The planeserver config file (`planeserver.toml`). Values and names follow the C# PlaneServer
//! settings (`Settings.settings`), with the app's feed settings and disclaimer list.

use as_settings::{format_profile, FeedSettings, SettingsHost, SettingsVersion};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 9872;
/// At most this many feeds run at once.
pub const MAX_FEEDS: usize = 3;
/// Answer of `/` (`Settings.settings:20`).
pub const WELCOME: &str = "<HTML><BODY> Welcome to AirScout PlaneServer</BODY></HTML>";

/// Errors of loading or saving the config. `Display` names the path and never the file text:
/// the file can hold feed credentials.
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse {path} (the text is not shown: it may hold a password)")]
    Parse { path: PathBuf },
    #[error("cannot serialize the config for {path}")]
    Serialize { path: PathBuf },
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlaneServerConfig {
    pub server_active: bool,
    pub bind: IpAddr,
    pub port: u16,
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
    pub min_alt_m: i32,
    pub max_alt_m: i32,
    pub lifetime_min: i64,
    pub interval_s: u64,
    pub feeds: Vec<String>,
    pub feed_settings: FeedSettings,
    pub feeds_disclaimer_accepted: Vec<String>,
    pub extended_plausibility: bool,
    pub max_error_dist: f64,
    pub data_dir: Option<PathBuf>,
    pub start_in_tray: bool,
}

impl Default for PlaneServerConfig {
    fn default() -> Self {
        Self {
            server_active: true,
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
            min_lat: 35.0,
            max_lat: 60.0,
            min_lon: -15.0,
            max_lon: 30.0,
            min_alt_m: 5000,
            max_alt_m: 20000,
            lifetime_min: 5,
            interval_s: 60,
            feeds: Vec::new(),
            feed_settings: FeedSettings::for_host(SettingsHost::PlaneServer),
            feeds_disclaimer_accepted: Vec::new(),
            extended_plausibility: true,
            max_error_dist: 30.0,
            data_dir: None,
            start_in_tray: false,
        }
    }
}

// `FeedSettings` hides arbitrary keys and values in diagnostics.
impl std::fmt::Debug for PlaneServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaneServerConfig")
            .field("server_active", &self.server_active)
            .field("bind", &self.bind)
            .field("port", &self.port)
            .field("min_lat", &self.min_lat)
            .field("max_lat", &self.max_lat)
            .field("min_lon", &self.min_lon)
            .field("max_lon", &self.max_lon)
            .field("min_alt_m", &self.min_alt_m)
            .field("max_alt_m", &self.max_alt_m)
            .field("lifetime_min", &self.lifetime_min)
            .field("interval_s", &self.interval_s)
            .field("feeds", &self.feeds)
            .field("feed_settings", &self.feed_settings)
            .field("feeds_disclaimer_accepted", &self.feeds_disclaimer_accepted)
            .field("extended_plausibility", &self.extended_plausibility)
            .field("max_error_dist", &self.max_error_dist)
            .field("data_dir", &self.data_dir)
            .field("start_in_tray", &self.start_in_tray)
            .finish()
    }
}

/// `planeserver.toml` next to the app's `settings.toml`.
pub fn default_path() -> PathBuf {
    as_settings::Settings::default_path().with_file_name("planeserver.toml")
}

/// A feed that needs the disclaimer before it may start: not enabled by default, or one that
/// carries a disclaimer text.
pub fn feed_needs_disclaimer(f: &dyn as_feeds::PlaneFeed) -> bool {
    f.needs_disclaimer()
}

/// Reset a non-finite value to `default`, with a warning.
fn finite_or(v: &mut f64, default: f64, name: &str, warnings: &mut Vec<String>) {
    if !v.is_finite() {
        *v = default;
        warnings.push(format!("{name} is not a finite number, reset to {default}"));
    }
}

fn clamp_f64(v: &mut f64, lo: f64, hi: f64, name: &str, warnings: &mut Vec<String>) {
    let c = v.clamp(lo, hi);
    if c != *v {
        warnings.push(format!("{name} {v} is outside {lo}..{hi}, set to {c}"));
        *v = c;
    }
}

impl PlaneServerConfig {
    /// Load `path`; a missing file gives the defaults. A file that cannot be parsed is an error
    /// and is left untouched.
    pub fn load_or_default(path: &Path) -> Result<PlaneServerConfig, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Io {
                    path: path.into(),
                    source,
                })
            }
        };
        let parse = |_| ConfigError::Parse { path: path.into() };
        let raw: toml::Table = toml::from_str(&text).map_err(parse)?;
        let version =
            SettingsVersion::read(&raw).ok_or_else(|| ConfigError::Parse { path: path.into() })?;
        let has_feeds = raw.contains_key("feed_settings");
        let mut config: Self = raw.try_into().map_err(parse)?;
        if !has_feeds {
            config.feed_settings = FeedSettings::default();
        }
        config
            .feed_settings
            .resolve(SettingsHost::PlaneServer, version);
        Ok(config)
    }

    /// Save atomically and owner-only on Unix (see `as_settings::write_private_atomic`).
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let io = |source| ConfigError::Io {
            path: path.into(),
            source,
        };
        let text = format_profile(self, SettingsHost::PlaneServer)
            .map_err(|_| ConfigError::Serialize { path: path.into() })?;
        as_settings::write_private_atomic(path, &text).map_err(io)
    }

    /// Bring the file values into range; one warning line per change. Runs on the file values
    /// only; command line overrides come after it.
    pub fn validate(&mut self) -> Vec<String> {
        let d = Self::default();
        let mut w = Vec::new();
        for (v, def, name) in [
            (&mut self.min_lat, d.min_lat, "min_lat"),
            (&mut self.max_lat, d.max_lat, "max_lat"),
            (&mut self.min_lon, d.min_lon, "min_lon"),
            (&mut self.max_lon, d.max_lon, "max_lon"),
            (&mut self.max_error_dist, d.max_error_dist, "max_error_dist"),
        ] {
            finite_or(v, def, name, &mut w);
        }
        clamp_f64(&mut self.min_lat, -90.0, 90.0, "min_lat", &mut w);
        clamp_f64(&mut self.max_lat, -90.0, 90.0, "max_lat", &mut w);
        clamp_f64(&mut self.min_lon, -180.0, 180.0, "min_lon", &mut w);
        clamp_f64(&mut self.max_lon, -180.0, 180.0, "max_lon", &mut w);
        if self.min_lat > self.max_lat {
            std::mem::swap(&mut self.min_lat, &mut self.max_lat);
            w.push("min_lat is above max_lat, swapped".to_owned());
        }
        if self.min_lon > self.max_lon {
            std::mem::swap(&mut self.min_lon, &mut self.max_lon);
            w.push("min_lon is above max_lon, swapped".to_owned());
        }
        if self.min_alt_m > self.max_alt_m {
            std::mem::swap(&mut self.min_alt_m, &mut self.max_alt_m);
            w.push("min_alt_m is above max_alt_m, swapped".to_owned());
        }
        if self.port == 0 {
            self.port = DEFAULT_PORT;
            w.push(format!("port 0 is not allowed, set to {DEFAULT_PORT}"));
        }
        if self.feeds.len() > MAX_FEEDS {
            self.feeds.truncate(MAX_FEEDS);
            w.push(format!(
                "more than {MAX_FEEDS} feeds, keeping the first {MAX_FEEDS}"
            ));
        }
        if self.interval_s < 1 {
            self.interval_s = 1;
            w.push("interval_s is below 1, set to 1".to_owned());
        }
        let l = self.lifetime_min.clamp(1, 1440);
        if l != self.lifetime_min {
            w.push(format!(
                "lifetime_min {} is outside 1..1440, set to {l}",
                self.lifetime_min
            ));
            self.lifetime_min = l;
        }
        w
    }

    /// `data_dir`, or the data dir of the app (`eu.airscout.AirScout-RS`; `./data` if the OS has
    /// none). Same logic as `data_dir` in the `airscout` crate.
    pub fn data_dir(&self) -> PathBuf {
        if let Some(d) = &self.data_dir {
            return d.clone();
        }
        directories::ProjectDirs::from("eu", "airscout", "AirScout-RS")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| {
                let rel = PathBuf::from("data");
                std::path::absolute(&rel).unwrap_or(rel)
            })
    }
}
