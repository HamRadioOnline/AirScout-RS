//! AirScout Server feed (`AirScout.PlaneFeeds.Plugin.AirScoutServer`): another AirScout
//! instance's `planes.json` (`List<PlaneJSON>`), or the legacy FR24-style dictionary.
//!
//! Faithful to the C# plugin: `Reg` is not mapped; in the legacy branch only keys that
//! start with `"0"` are read. Unlike most feeds, errors are not swallowed in C# either.

use super::{http_get, parse_err, set_str, set_u64};
use crate::{vars, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

pub const DEFAULT_URL: &str = "http://localhost:9880/planes.json";

/// `UNIXTimeToDateTime(int)`.
fn unix_s(t: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(t, 0)
}

/// A `PlaneJSON` property, looked up like Newtonsoft does: exact name first, then
/// case-insensitive. `Ok(None)` if absent.
fn prop<'a>(m: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    m.get(name).or_else(|| {
        m.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    })
}

/// A `double?` property: absent keeps the constructor default 0, null is null.
fn dbl(m: &Map<String, Value>, name: &str) -> Result<Option<f64>, FeedError> {
    match prop(m, name) {
        None => Ok(Some(0.0)),
        Some(v) => super::de_f64(v.clone()).map_err(parse_err),
    }
}

/// A `string` property: absent or null both end up as `""` after `IsNullOrEmpty`.
fn text(m: &Map<String, Value>, name: &str) -> Result<String, FeedError> {
    match prop(m, name) {
        None => Ok(String::new()),
        Some(v) => Ok(super::de_string(v.clone())
            .map_err(parse_err)?
            .unwrap_or_default()),
    }
}

/// An `int?` property; absent keeps `default`.
fn int(m: &Map<String, Value>, name: &str, default: Option<i64>) -> Result<Option<i64>, FeedError> {
    match prop(m, name) {
        None => Ok(default),
        Some(v) => {
            let i = super::de_i64(v.clone()).map_err(parse_err)?;
            match i {
                Some(x) if i32::try_from(x).is_err() => {
                    Err(FeedError::Parse(format!("{name}: {x} is not an Int32")))
                }
                _ => Ok(i),
            }
        }
    }
}

fn plane_json(v: &Value, now: DateTime<Utc>) -> Result<FeedPlane, FeedError> {
    let m = v
        .as_object()
        .ok_or_else(|| FeedError::Parse("plane is not an object".into()))?;
    let nan = |x: Option<f64>| x.unwrap_or(f64::NAN);
    // a missing Time keeps the constructor's DateTime.UtcNow
    let time = int(m, "Time", Some(now.timestamp()))?;
    Ok(FeedPlane {
        hex: text(m, "Hex")?,
        call: text(m, "Call")?,
        lat: nan(dbl(m, "Lat")?),
        lon: nan(dbl(m, "Lon")?),
        alt_ft: nan(dbl(m, "Alt")?),
        track: nan(dbl(m, "Track")?),
        speed_kts: nan(dbl(m, "Speed")?),
        type_code: text(m, "Type")?,
        category: int(m, "Category", Some(0))?.unwrap_or(0) as i32,
        manufacturer: text(m, "Manufacturer")?,
        model: text(m, "Model")?,
        time: time.and_then(unix_s),
        ..FeedPlane::default()
    })
}

/// One FR24 array: `[hex, lat, lon, track, alt, speed, squawk, radar, type, reg, time,
/// from, to, flight, ground, vspeed, call, ...]`. `None` where C# throws and skips it.
fn fr24(v: &Value) -> Option<FeedPlane> {
    let a = v.as_array()?;
    // `!String.IsNullOrEmpty(ac[i]) ? ac[i] : ""` binds only for strings and null
    let s = |i: usize| -> Option<String> {
        match a.get(i)? {
            Value::Null => Some(String::new()),
            Value::String(s) => Some(s.clone()),
            _ => None,
        }
    };
    let d = |i: usize| a.get(i)?.as_f64();
    let time = a.get(10)?.as_f64()?.trunc();
    if time < f64::from(i32::MIN) || time > f64::from(i32::MAX) {
        return None;
    }
    Some(FeedPlane {
        hex: s(0)?,
        call: s(16)?,
        lat: d(1)?,
        lon: d(2)?,
        alt_ft: d(4)?,
        track: d(3)?,
        speed_kts: d(5)?,
        type_code: s(8)?,
        time: unix_s(time as i64),
        ..FeedPlane::default()
    })
}

/// Map one response.
pub fn parse(json: &str, now: DateTime<Utc>) -> Result<Vec<FeedPlane>, FeedError> {
    if json.encode_utf16().count() < 20 {
        return Ok(Vec::new());
    }
    if json.contains("\"full_count\":") && json.contains("\"version\":1") {
        let root: Map<String, Value> = serde_json::from_str(json).map_err(parse_err)?;
        return Ok(root
            .iter()
            .filter(|(k, _)| k.starts_with('0'))
            .filter_map(|(_, v)| fr24(v))
            .collect());
    }
    let list: Vec<Value> = serde_json::from_str(json).map_err(parse_err)?;
    // Newtonsoft fails the whole list on a bad value; the mapping itself cannot throw
    list.iter().map(|v| plane_json(v, now)).collect()
}

/// The AirScout Server feed.
#[derive(Debug, Clone)]
pub struct AirScoutServer {
    pub url: String,
    pub timeout_s: u64,
}

impl Default for AirScoutServer {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            timeout_s: 30,
        }
    }
}

impl PlaneFeed for AirScoutServer {
    fn name(&self) -> &'static str {
        "AirScoutServer"
    }

    fn info(&self) -> &'static str {
        "Web feed from another AirScout instance acting as a server (planes.json)."
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
        t.insert("timeout_s".into(), (self.timeout_s as i64).into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_u64(t, "timeout_s", &mut self.timeout_s)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let now = Utc::now();
        let url = vars::substitute(&self.url, args, now, "")?;
        parse(&http_get(&url, self.timeout_s, None)?, now)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
