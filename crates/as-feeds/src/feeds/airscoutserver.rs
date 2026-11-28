//! AirScout Server feed (`AirScout.PlaneFeeds.Plugin.AirScoutServer`): another AirScout
//! instance's `planes.json` (`List<PlaneJSON>`), or the legacy FR24-style dictionary.

use super::{client_builder, epoch_seconds, map_records, parse_err, send, set_str, set_u64};
use crate::{vars, FeedArgs, FeedBatch, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Utc};
use reqwest::{redirect::Policy, Url};
use serde_json::{value::RawValue, Map, Value};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const DEFAULT_URL: &str = "http://localhost:9880/planes.json";

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

/// An integral property with a separate default for a missing field.
fn int(m: &Map<String, Value>, name: &str, default: Option<i64>) -> Result<Option<i64>, FeedError> {
    match prop(m, name) {
        None => Ok(default),
        Some(v) => super::de_i64(v.clone()).map_err(parse_err),
    }
}

fn plane_json(v: Value, now: DateTime<Utc>) -> Option<FeedPlane> {
    let m = v.as_object()?;
    let nan = |x: Option<f64>| x.unwrap_or(f64::NAN);
    let hex = text(m, "Hex").ok()?;
    let reg = text(m, "Reg").ok()?.trim().to_owned();
    if hex.trim().is_empty() && reg.is_empty() {
        return None;
    }
    // A missing Time retains the constructor default. An explicit null has no timestamp.
    let time = match int(m, "Time", Some(now.timestamp())).ok()? {
        Some(time) => Some(DateTime::from_timestamp(time, 0)?),
        None => None,
    };
    Some(FeedPlane {
        hex,
        call: text(m, "Call").ok()?,
        reg,
        lat: nan(dbl(m, "Lat").ok()?),
        lon: nan(dbl(m, "Lon").ok()?),
        alt_ft: nan(dbl(m, "Alt").ok()?),
        track: nan(dbl(m, "Track").ok()?),
        speed_kts: nan(dbl(m, "Speed").ok()?),
        type_code: text(m, "Type").ok()?,
        category: i32::try_from(int(m, "Category", Some(0)).ok()?.unwrap_or(0)).ok()?,
        manufacturer: text(m, "Manufacturer").ok()?,
        model: text(m, "Model").ok()?,
        time,
        ..FeedPlane::default()
    })
}

/// One FR24 array: `[hex, lat, lon, track, alt, speed, squawk, radar, type, reg, time,
/// from, to, flight, ground, vspeed, call, ...]`. `None` where C# throws and skips it.
fn fr24(v: Value) -> Option<FeedPlane> {
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
    let time = epoch_seconds(d(10)?)?;
    let hex = s(0)?;
    let reg = s(9)?.trim().to_owned();
    if hex.trim().is_empty() && reg.is_empty() {
        return None;
    }
    Some(FeedPlane {
        hex,
        reg,
        call: s(16)?,
        lat: d(1)?,
        lon: d(2)?,
        alt_ft: d(4)?,
        track: d(3)?,
        speed_kts: d(5)?,
        type_code: s(8)?,
        time: Some(time),
        ..FeedPlane::default()
    })
}

/// Map one response.
pub fn parse(json: &str, now: DateTime<Utc>) -> Result<Vec<FeedPlane>, FeedError> {
    Ok(parse_batch(json, now)?.planes)
}

fn parse_batch(json: &str, now: DateTime<Utc>) -> Result<FeedBatch, FeedError> {
    match json.trim_start().as_bytes().first() {
        Some(b'[') => {
            let list: Vec<&RawValue> = serde_json::from_str(json).map_err(parse_err)?;
            map_records(list, |row| plane_json(row, now))
        }
        Some(b'{') => {
            let mut root: BTreeMap<String, &RawValue> =
                serde_json::from_str(json).map_err(parse_err)?;
            let version = root
                .remove("version")
                .and_then(|raw| raw.get().parse::<u64>().ok());
            let full_count = root
                .remove("full_count")
                .and_then(|raw| raw.get().parse::<u64>().ok());
            if version != Some(1) || full_count.is_none() {
                return Err(FeedError::Parse(
                    "unsupported legacy aircraft envelope".into(),
                ));
            }
            root.remove("stats");
            map_records(root.into_values(), fr24)
        }
        _ => Err(FeedError::Parse("invalid aircraft envelope".into())),
    }
}

fn own_planes_url(url: &Url, listener: SocketAddr) -> bool {
    if url.port_or_known_default() != Some(listener.port())
        || !url.path().eq_ignore_ascii_case("/planes.json")
    {
        return false;
    }

    let local_ip = |ip: IpAddr| {
        let ip = ip.to_canonical();
        let bind = listener.ip().to_canonical();
        if ip == bind {
            return true;
        }
        let same_family = bind.is_ipv4() == ip.is_ipv4();
        if ip.is_unspecified() {
            return same_family && bind.is_loopback();
        }
        bind.is_unspecified()
            && (same_family || bind.is_ipv6())
            && (ip.is_loopback() || TcpListener::bind((ip, 0)).is_ok())
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse() {
        return local_ip(ip);
    }
    (host, listener.port())
        .to_socket_addrs()
        .is_ok_and(|mut addresses| addresses.any(|addr| local_ip(addr.ip())))
}

fn own_feed_error() -> FeedError {
    FeedError::Config("AirScoutServer feed targets this process’s planes endpoint".into())
}

/// The AirScout Server feed.
#[derive(Clone)]
pub struct AirScoutServer {
    pub url: String,
    pub timeout_s: u64,
}

impl std::fmt::Debug for AirScoutServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirScoutServer")
            .field("timeout_s", &self.timeout_s)
            .finish_non_exhaustive()
    }
}

impl Default for AirScoutServer {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            timeout_s: 30,
        }
    }
}

impl AirScoutServer {
    fn request_url(&self, args: &FeedArgs, now: DateTime<Utc>) -> Result<Url, FeedError> {
        if self.url.is_empty() {
            return Err(FeedError::Config(
                "AirScoutServer feed URL is unconfigured".into(),
            ));
        }
        let expanded = vars::substitute(&self.url, args, now, "")?;
        let url =
            Url::parse(&expanded).map_err(|_| FeedError::Config("invalid feed URL".into()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(FeedError::Config(
                "AirScoutServer requires an HTTP feed URL".into(),
            ));
        }
        if args
            .airscout_listener
            .is_some_and(|listener| own_planes_url(&url, listener))
        {
            return Err(own_feed_error());
        }
        Ok(url)
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

    fn start(&mut self, args: &FeedArgs) -> Result<(), FeedError> {
        self.request_url(args, Utc::now())?;
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        Ok(self.get_batch(args)?.planes)
    }

    fn get_batch(&mut self, args: &FeedArgs) -> Result<FeedBatch, FeedError> {
        let now = Utc::now();
        let url = self.request_url(args, now)?;
        let mut builder = client_builder(self.timeout_s, false);
        let blocked = Arc::new(AtomicBool::new(false));
        if let Some(listener) = args.airscout_listener {
            let rejected = blocked.clone();
            let follow = Policy::limited(10);
            builder = builder.redirect(Policy::custom(move |attempt| {
                if own_planes_url(attempt.url(), listener) {
                    rejected.store(true, Ordering::Relaxed);
                    return attempt.error("redirect targets this process’s planes endpoint");
                }
                follow.redirect(attempt)
            }));
        }
        let client = builder
            .build()
            .map_err(|_| FeedError::Http("could not initialize HTTP client".into()))?;
        let body = send(client.get(url));
        if blocked.load(Ordering::Relaxed) {
            return Err(own_feed_error());
        }
        parse_batch(&String::from_utf8_lossy(&body?), now)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
