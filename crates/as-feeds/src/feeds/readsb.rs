//! Readsb / tar1090 JSON feed (`AirScout.PlaneFeeds.Plugin.Readsb`).
//!
//! Faithful to the C# plugin: callsign, type and category are not mapped, and the DTO
//! declares `alt_baro` as `double?`, so one aircraft reporting `"ground"` makes the whole
//! response unreadable (`tests/golden/feed_json.json`).

use super::{clean_hex, convert_to_i32, de_f64, de_string, http_get, parse_err};
use super::{seconds_before, set_str, unix_ms, NO_ALT, WEBCLIENT_TIMEOUT_S};
use crate::{vars, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const DEFAULT_URL: &str = "http://localhost/tar1090/data/aircraft.json";

#[derive(Deserialize)]
struct List {
    // [DataMember(IsRequired = true)]: must be present, may be null
    #[serde(deserialize_with = "de_f64")]
    now: Option<f64>,
    #[serde(deserialize_with = "super::required")]
    aircraft: Option<Vec<Aircraft>>,
}

#[derive(Deserialize)]
struct Aircraft {
    // required, may be null
    #[serde(deserialize_with = "de_string")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "de_f64")]
    alt_baro: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    gs: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    track: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    lat: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    lon: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    seen_pos: Option<f64>,
}

/// `filetime.AddMilliseconds((long)Now * 1000)`: the cast truncates `now` to whole seconds
/// before the multiplication.
pub(crate) fn file_time(now: Option<f64>) -> Option<DateTime<Utc>> {
    let now = now?;
    if !now.is_finite() || now.abs() > 1e13 {
        return None;
    }
    unix_ms((now.trunc() as i64).checked_mul(1000)?)
}

fn map(ac: &Aircraft, filetime: Option<DateTime<Utc>>) -> Option<FeedPlane> {
    let hex = clean_hex(ac.hex.as_deref()?).to_uppercase();
    let time = seconds_before(filetime?, ac.seen_pos?)?;
    Some(FeedPlane {
        time: Some(time),
        hex,
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.lon.unwrap_or(f64::NAN),
        alt_ft: ac
            .alt_baro
            .map_or(NO_ALT, |a| convert_to_i32(a).map_or(0.0, f64::from)),
        track: ac.track.unwrap_or(f64::NAN),
        speed_kts: ac.gs.unwrap_or(f64::NAN),
        ..FeedPlane::default()
    })
}

/// Map one response.
pub fn parse(json: &str) -> Result<Vec<FeedPlane>, FeedError> {
    let list: List = serde_json::from_str(json).map_err(parse_err)?;
    let ac = list
        .aircraft
        .ok_or_else(|| FeedError::Parse("no aircraft list".into()))?;
    let filetime = file_time(list.now);
    Ok(ac.iter().filter_map(|a| map(a, filetime)).collect())
}

/// The Readsb feed.
#[derive(Debug, Clone)]
pub struct Readsb {
    pub url: String,
}

impl Default for Readsb {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
        }
    }
}

impl PlaneFeed for Readsb {
    fn name(&self) -> &'static str {
        "Readsb"
    }

    fn info(&self) -> &'static str {
        "Web feed from a local Readsb / tar1090 receiver (aircraft.json)."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        None
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let url = vars::substitute(&self.url, args, Utc::now(), "")?;
        parse(&http_get(&url, WEBCLIENT_TIMEOUT_S, None)?)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
