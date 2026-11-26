//! ADS-B One web feed (`AirScout.PlaneFeeds.Plugin.ADSBOne`), readsb v2 API format.

use super::{clean_hex, de_f64, de_string, http_get, parse_err, seconds_before, set_str};
use super::{unix_ms, NO_ALT, WEBCLIENT_TIMEOUT_S};
use crate::{vars, wake, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::Utc;
use serde::Deserialize;

/// `ADSBOne.cs:41` at fbadef0: the radius is fixed at 250 instead of `%MAXDISTKM%`.
pub const DEFAULT_URL: &str = "https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/250";

#[derive(Deserialize)]
struct List {
    #[serde(deserialize_with = "super::required")]
    ac: Option<Vec<Aircraft>>,
    #[serde(default, deserialize_with = "super::de_i64")]
    now: Option<i64>,
}

#[derive(Deserialize)]
struct Aircraft {
    // required, may be null
    #[serde(deserialize_with = "de_string")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    flight: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    r: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    t: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    desc: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    alt_baro: Option<String>,
    #[serde(default, deserialize_with = "de_f64")]
    gs: Option<f64>,
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

/// The ADS-B One feed.
#[derive(Debug, Clone)]
pub struct AdsbOne {
    pub url: String,
}

impl Default for AdsbOne {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
        }
    }
}

/// `"AIRBUS A-380-800"` → (`"Airbus"`, `"A-380-800"`): the first word keeps its first
/// character and lowercases the rest. Without a space the whole text is the model.
/// `None` when the first word is empty (C# throws on `a[0][0]` and drops the plane).
fn split_desc(desc: &str) -> Option<(String, String)> {
    if !desc.contains(' ') {
        return Some((String::new(), desc.to_string()));
    }
    let mut words = desc.split(' ');
    let first = words.next().unwrap_or_default();
    let mut chars = first.chars();
    let head = chars.next()?;
    let manufacturer = format!("{head}{}", chars.as_str().to_lowercase());
    Some((manufacturer, words.collect::<Vec<_>>().join(" ")))
}

/// `Convert.ToInt32(string)`: an integer with optional sign and surrounding white space,
/// anything else (e.g. `"ground"`) becomes 0.
fn alt(s: Option<&str>) -> f64 {
    match s {
        None => NO_ALT,
        Some(s) => s.trim().parse::<i32>().map_or(0.0, f64::from),
    }
}

fn map(ac: &Aircraft, filetime: Option<chrono::DateTime<Utc>>) -> Option<FeedPlane> {
    let hex = clean_hex(ac.hex.as_deref()?).to_uppercase();
    // C# skips the plane without seen_pos; without "now" AddSeconds on MinValue throws
    let time = seconds_before(filetime?, ac.seen_pos?)?;
    let type_code = ac.t.clone().unwrap_or_default();
    let (manufacturer, model) = match ac.desc.as_deref() {
        Some(d) => split_desc(d)?,
        None => (String::new(), String::new()),
    };
    Some(FeedPlane {
        time: Some(time),
        hex,
        call: ac
            .flight
            .as_deref()
            .map_or(String::new(), |c| c.trim().to_uppercase()),
        reg: ac.r.clone().unwrap_or_default(),
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.lon.unwrap_or(f64::NAN),
        alt_ft: alt(ac.alt_baro.as_deref()),
        track: ac.track.unwrap_or(f64::NAN),
        speed_kts: ac.gs.unwrap_or(f64::NAN),
        category: wake::category(ac.category.as_deref().unwrap_or(""), &type_code),
        type_code,
        manufacturer,
        model,
        ..FeedPlane::default()
    })
}

/// Map one response. Exposed for tests of other front ends.
pub fn parse(json: &str) -> Result<Vec<FeedPlane>, FeedError> {
    let list: List = serde_json::from_str(json).map_err(parse_err)?;
    let ac = list
        .ac
        .ok_or_else(|| FeedError::Parse("no aircraft list".into()))?;
    let filetime = list.now.and_then(unix_ms);
    Ok(ac.iter().filter_map(|a| map(a, filetime)).collect())
}

impl PlaneFeed for AdsbOne {
    fn name(&self) -> &'static str {
        "ADSBOne"
    }

    fn info(&self) -> &'static str {
        "Web feed from ADS-B One (api.adsb.one): aircraft around your position."
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
