//! Dump1090 JSON feed (`AirScout.PlaneFeeds.Plugin.Dump1090`), dump1090(-mutability)
//! `aircraft.json` format.

use super::readsb::file_time;
use super::{clean_hex, de_f64, de_string, http_get, parse_err};
use super::{de_altitude, map_records, WireAltitude};
use super::{seconds_before, set_str, WEBCLIENT_TIMEOUT_S};
use crate::{vars, wake, FeedArgs, FeedBatch, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::value::RawValue;

pub const DEFAULT_URL: &str = "http://localhost:8080/data/aircraft.json";

#[derive(Deserialize)]
struct List<'a> {
    #[serde(deserialize_with = "de_f64")]
    now: Option<f64>,
    #[serde(borrow, deserialize_with = "super::required")]
    aircraft: Option<Vec<&'a RawValue>>,
}

#[derive(Deserialize)]
struct Aircraft {
    #[serde(deserialize_with = "de_string")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    flight: Option<String>,
    #[serde(default, deserialize_with = "de_altitude")]
    altitude: WireAltitude,
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
    Some(FeedPlane {
        time: Some(time),
        hex,
        call: ac.flight.as_deref().map(str::trim).unwrap_or("").into(),
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.lon.unwrap_or(f64::NAN),
        alt_ft: ac.altitude.feet()?,
        ground: ac.altitude.ground(),
        track: ac.track.unwrap_or(f64::NAN),
        speed_kts: ac.speed.unwrap_or(f64::NAN),
        category: wake::category(ac.category.as_deref().unwrap_or(""), ""),
        ..FeedPlane::default()
    })
}

/// Map one response.
pub fn parse(json: &str) -> Result<Vec<FeedPlane>, FeedError> {
    Ok(parse_batch(json)?.planes)
}

fn parse_batch(json: &str) -> Result<FeedBatch, FeedError> {
    let list: List<'_> = serde_json::from_str(json).map_err(parse_err)?;
    let ac = list
        .aircraft
        .ok_or_else(|| FeedError::Parse("no aircraft list".into()))?;
    let filetime = file_time(list.now);
    map_records(ac, |a: Aircraft| map(&a, filetime))
}

/// The Dump1090 feed.
#[derive(Clone)]
pub struct Dump1090 {
    pub url: String,
}

impl std::fmt::Debug for Dump1090 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dump1090").finish_non_exhaustive()
    }
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
        Ok(self.get_batch(args)?.planes)
    }

    fn get_batch(&mut self, args: &FeedArgs) -> Result<FeedBatch, FeedError> {
        let url = vars::substitute(&self.url, args, Utc::now(), "")?;
        parse_batch(&http_get(&url, WEBCLIENT_TIMEOUT_S, None)?)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
