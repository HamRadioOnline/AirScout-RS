//! Versioned settings resolution shared by the desktop and PlaneServer.

use crate::FeedSettings;
use serde::Serialize;
use toml::{Table, Value};

const VERSION_KEY: &str = "settings_version";
const CURRENT_VERSION: i64 = 1;
const LEGACY_AIRSCOUT_URL: &str = "http://localhost:9880/planes.json";
const DESKTOP_AIRSCOUT_URL: &str = "http://localhost:9872/planes.json";

/// Settings profiles have different defaults for the AirScoutServer input.
#[derive(Clone, Copy)]
pub enum SettingsHost {
    Desktop,
    PlaneServer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsVersion {
    Legacy,
    Current,
}

impl SettingsVersion {
    /// A missing stamp identifies a pre-versioning file.
    pub fn read(table: &Table) -> Option<Self> {
        match table.get(VERSION_KEY) {
            None => Some(Self::Legacy),
            Some(Value::Integer(CURRENT_VERSION)) => Some(Self::Current),
            _ => None,
        }
    }
}

impl FeedSettings {
    pub fn for_host(host: SettingsHost) -> Self {
        let mut settings = Self::default();
        settings.resolve(host, SettingsVersion::Current);
        settings
    }

    pub fn resolve(&mut self, host: SettingsHost, version: SettingsVersion) {
        resolve_feed_settings(&mut self.0, host, version);
    }
}

fn resolve_feed_settings(tables: &mut Table, host: SettingsHost, version: SettingsVersion) {
    let feed_url = match (version, host) {
        (SettingsVersion::Legacy, _) => LEGACY_AIRSCOUT_URL,
        (SettingsVersion::Current, SettingsHost::Desktop) => DESKTOP_AIRSCOUT_URL,
        (SettingsVersion::Current, SettingsHost::PlaneServer) => "",
    };
    let server = tables
        .entry("AirScoutServer")
        .or_insert_with(|| Value::Table(Table::new()));
    if let Some(table) = server.as_table_mut() {
        table.entry("url").or_insert_with(|| feed_url.into());
    }
    if let Some(table) = tables.get_mut("OpenSky").and_then(Value::as_table_mut) {
        canonicalize_feed_table("OpenSky", table);
    }
}

/// Apply the same authority migration to edited tables and to profiles.
pub fn canonicalize_feed_table(feed: &str, table: &mut Table) {
    if feed != "OpenSky" {
        return;
    }
    let Some(Value::String(url)) = table.get_mut("url") else {
        return;
    };
    if let Some(range) = legacy_opensky_host(url) {
        url.replace_range(range, "opensky-network.org");
    }
}

/// Locate the authority in the input bytes so template tokens and existing escapes survive.
fn legacy_opensky_host(url: &str) -> Option<std::ops::Range<usize>> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority_end = rest.find(['/', '?', '#'])?;
    if !rest[authority_end..].starts_with("/api/") {
        return None;
    }
    let authority = &rest[..authority_end];
    let host_start = authority.rfind('@').map_or(0, |at| at + 1);
    let host_port = &authority[host_start..];
    let host = host_port.strip_suffix(":443").unwrap_or(host_port);
    if !host.eq_ignore_ascii_case("api.opensky-network.org") {
        return None;
    }
    let offset = scheme.len() + 3 + host_start;
    Some(offset..offset + host.len())
}

/// Serialize resolved settings through the same versioning boundary for both hosts.
pub fn format_profile(
    profile: &impl Serialize,
    host: SettingsHost,
) -> Result<String, toml::ser::Error> {
    let mut table = Table::try_from(profile)?;
    table.insert(VERSION_KEY.into(), CURRENT_VERSION.into());
    if let Some(feeds) = table.get_mut("feed_settings").and_then(Value::as_table_mut) {
        resolve_feed_settings(feeds, host, SettingsVersion::Current);
    }
    toml::to_string_pretty(&table)
}
