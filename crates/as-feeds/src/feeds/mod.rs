//! The feed plugins and what they share: HTTP GET, settings tables, and JSON values read
//! the way Newtonsoft.Json reads them into the C# DTOs.

pub mod adsbone;
pub mod adsbsupport;
pub mod airscoutserver;
pub mod bincraft;
pub mod bincraft_parse;
pub mod dump1090;
pub mod flexjson;
pub mod opensky;
pub mod publicbincraft;
pub mod readsb;
pub mod rtl1090;
pub mod vrs;

use crate::FeedError;
use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::time::Duration as StdDuration;

/// `System.Net.WebClient` has no timeout setting; its default is 100 s.
pub(crate) const WEBCLIENT_TIMEOUT_S: u64 = 100;

/// A blocking client with the honest AirScout-RS User-Agent and `timeout_s`.
pub(crate) fn client(timeout_s: u64, cookies: bool) -> Result<Client, FeedError> {
    Client::builder()
        .timeout(StdDuration::from_secs(timeout_s))
        .user_agent(as_net::user_agent())
        .cookie_store(cookies)
        .build()
        .map_err(|e| FeedError::Http(e.without_url().to_string()))
}

/// GET `url` with `client`. Basic auth only when both user and password are set. Errors
/// never contain the URL (it may carry credentials) or the credentials.
pub(crate) fn get_bytes(
    client: &Client,
    url: &str,
    auth: Option<(&str, &str)>,
) -> Result<Vec<u8>, FeedError> {
    let mut req = client.get(url);
    if let Some((user, pw)) = auth {
        if !user.is_empty() && !pw.is_empty() {
            req = req.basic_auth(user, Some(pw));
        }
    }
    send(req)
}

/// Send `req`; a non-2xx status is an [`FeedError::Http`] naming only status and host.
pub(crate) fn send(req: reqwest::blocking::RequestBuilder) -> Result<Vec<u8>, FeedError> {
    let resp = req.send().map_err(net_err)?;
    let status = resp.status();
    if !status.is_success() {
        let host = resp.url().host_str().unwrap_or("?").to_string();
        return Err(FeedError::Http(format!("{status} from {host}")));
    }
    Ok(resp.bytes().map_err(net_err)?.to_vec())
}

/// GET `url` with a fresh client and return the body as text (lossy UTF-8).
pub(crate) fn http_get(
    url: &str,
    timeout_s: u64,
    auth: Option<(&str, &str)>,
) -> Result<String, FeedError> {
    let body = get_bytes(&client(timeout_s, false)?, url, auth)?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Undo zstd compression; a bad frame is a parse error.
pub(crate) fn unzstd(data: &[u8]) -> Result<Vec<u8>, FeedError> {
    zstd::decode_all(data).map_err(|e| FeedError::Parse(format!("zstd: {e}")))
}

fn net_err(e: reqwest::Error) -> FeedError {
    if e.is_timeout() {
        FeedError::Timeout
    } else {
        FeedError::Http(e.without_url().to_string())
    }
}

pub(crate) fn parse_err(e: impl std::fmt::Display) -> FeedError {
    FeedError::Parse(e.to_string())
}

// ------------------------------------------------------------------ settings tables

/// Read one optional setting into `dst`; a wrong type is a configuration error.
pub(crate) fn set_str(t: &toml::Table, key: &str, dst: &mut String) -> Result<(), FeedError> {
    match t.get(key) {
        None => Ok(()),
        Some(toml::Value::String(s)) => {
            dst.clone_from(s);
            Ok(())
        }
        Some(_) => Err(FeedError::Config(format!("{key} must be a string"))),
    }
}

pub(crate) fn set_bool(t: &toml::Table, key: &str, dst: &mut bool) -> Result<(), FeedError> {
    match t.get(key) {
        None => Ok(()),
        Some(toml::Value::Boolean(b)) => {
            *dst = *b;
            Ok(())
        }
        Some(_) => Err(FeedError::Config(format!("{key} must be true or false"))),
    }
}

pub(crate) fn set_u64(t: &toml::Table, key: &str, dst: &mut u64) -> Result<(), FeedError> {
    match t.get(key) {
        None => Ok(()),
        Some(toml::Value::Integer(i)) if *i > 0 => {
            *dst = *i as u64;
            Ok(())
        }
        Some(_) => Err(FeedError::Config(format!(
            "{key} must be a positive number"
        ))),
    }
}

// ------------------------------------------------------------------ Newtonsoft-like values

/// A `[DataMember(IsRequired = true)]` property: must be present, may be null.
pub(crate) fn required<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

/// A `string` DTO property: Newtonsoft also accepts a number or bool and keeps its text.
pub(crate) fn de_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => None,
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if b { "True" } else { "False" }.into()),
        v => {
            return Err(serde::de::Error::custom(format!(
                "expected a string, got {v}"
            )))
        }
    })
}

/// A `double?` DTO property: a number, a numeric string, `""`/null for null. Any other
/// string (e.g. `"ground"`) fails the whole document, as `JsonReaderException` does in C#.
pub(crate) fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(n.as_f64()),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) => s.trim().parse::<f64>().map(Some).map_err(|_| {
            serde::de::Error::custom(format!("could not convert string to double: {s}"))
        }),
        v => Err(serde::de::Error::custom(format!(
            "expected a number, got {v}"
        ))),
    }
}

/// A `float?` DTO property (VRS `Spd`, `Trak`): read as double, stored as float.
pub(crate) fn de_f32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f32>, D::Error> {
    Ok(de_f64(d)?.map(|x| x as f32))
}

/// An `int?`/`long?` DTO property: an integer number or an integer string; a fraction or
/// an out-of-range value fails the document.
pub(crate) fn de_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom(format!("{n} is not a valid integer"))),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom(format!("{s} is not a valid integer"))),
        v => Err(serde::de::Error::custom(format!(
            "expected an integer, got {v}"
        ))),
    }
}

/// `Convert.ToInt32(double)`: round half to even; `None` on overflow (C# throws).
pub(crate) fn convert_to_i32(x: f64) -> Option<i32> {
    let r = x.round_ties_even();
    if r.is_nan() || r < f64::from(i32::MIN) || r > f64::from(i32::MAX) {
        None
    } else {
        Some(r as i32)
    }
}

/// `int.MinValue` as the C# feeds use it for "no altitude".
pub(crate) const NO_ALT: f64 = i32::MIN as f64;

/// Unix epoch plus `ms` milliseconds.
pub(crate) fn unix_ms(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

/// `t.AddSeconds(-s)` of .NET Framework: the offset is rounded to whole milliseconds.
pub(crate) fn seconds_before(t: DateTime<Utc>, s: f64) -> Option<DateTime<Utc>> {
    let ms = (s * 1000.0).round();
    if !ms.is_finite() || ms.abs() > 1e15 {
        return None;
    }
    t.checked_sub_signed(Duration::milliseconds(ms as i64))
}

/// `string.Trim().Replace("\"", "")` of the hex field.
pub(crate) fn clean_hex(hex: &str) -> String {
    hex.trim().replace('"', "")
}
