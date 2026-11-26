//! Dump1090 JSON feed (`AirScout.PlaneFeeds.Plugin.Dump1090`), dump1090(-mutability)
//! `aircraft.json` format.
//!
//! Faithful to the C# plugin: `flight` is not mapped, the category `if` chain has no
//! `else` before the A5 test so only A5 survives, and `"altitude":"ground"` makes the whole
//! response unreadable (`tests/golden/feed_json.json`).

use super::readsb::file_time;
use super::{clean_hex, convert_to_i32, de_f64, de_string, http_get, parse_err};
use super::{seconds_before, set_str, NO_ALT, WEBCLIENT_TIMEOUT_S};
use crate::{vars, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const DEFAULT_URL: &str = "http://localhost:8080/data/aircraft.json";

#[derive(Deserialize)]
struct List {
    #[serde(deserialize_with = "de_f64")]
    now: Option<f64>,
    #[serde(deserialize_with = "super::required")]
    aircraft: Option<Vec<Aircraft>>,
}

#[derive(Deserialize)]
struct Aircraft {
    #[serde(deserialize_with = "de_string")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "de_f64")]
    altitude: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    speed: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    track: Option<f64>,
    #[serde(default, deserialize_with = "de_string")]
    category: Option<String>,
    #[serde(default, deserialize_with = "de_f64")]
    lat: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    lon: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    seen_pos: Option<f64>,
}

fn map(ac: &Aircraft, filetime: Option<DateTime<Utc>>) -> Option<FeedPlane> {
    let hex = clean_hex(ac.hex.as_deref()?).to_uppercase();
    let time = seconds_before(filetime?, ac.seen_pos?)?;
    // the A1..A4 assignments are overwritten by the final `if A5 .. else 0`; the A388
    // test reads `plane.Type`, which this feed never sets
    let category = if ac.category.as_deref() == Some("A5") {
        3
    } else {
        0
    };
    Some(FeedPlane {
        time: Some(time),
        hex,
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.lon.unwrap_or(f64::NAN),
        alt_ft: ac
            .altitude
            .map_or(NO_ALT, |a| convert_to_i32(a).map_or(0.0, f64::from)),
        track: ac.track.unwrap_or(f64::NAN),
        speed_kts: ac.speed.unwrap_or(f64::NAN),
        category,
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

/// The Dump1090 feed.
#[derive(Debug, Clone)]
pub struct Dump1090 {
    pub url: String,
}

impl Default for Dump1090 {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
        }
    }
}

impl PlaneFeed for Dump1090 {
    fn name(&self) -> &'static str {
        "Dump1090"
    }

    fn info(&self) -> &'static str {
        "Web feed from a local Dump1090 receiver (data/aircraft.json)."
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
