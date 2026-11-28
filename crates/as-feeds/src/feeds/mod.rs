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

use crate::{FeedBatch, FeedError, FeedPlane};
use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{value::RawValue, Value};
use std::time::Duration as StdDuration;

/// `System.Net.WebClient` has no timeout setting; its default is 100 s.
pub(crate) const WEBCLIENT_TIMEOUT_S: u64 = 100;

const HTTP_JSON_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const FLEX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const BINCRAFT_COMPRESSED_BYTES: u64 = 16 * 1024 * 1024;
const BINCRAFT_RAW_BYTES: u64 = 32 * 1024 * 1024;
const BINCRAFT_MAX_WINDOW_LOG: u32 = 25;

/// A blocking client with the AirScout-RS User-Agent and `timeout_s`.
pub(crate) fn client(timeout_s: u64, cookies: bool) -> Result<Client, FeedError> {
    client_builder(timeout_s, cookies)
        .build()
        .map_err(|_| FeedError::Http("could not initialize HTTP client".into()))
}

pub(crate) fn client_builder(timeout_s: u64, cookies: bool) -> reqwest::blocking::ClientBuilder {
    Client::builder()
        .timeout(StdDuration::from_secs(timeout_s))
        .user_agent(as_net::user_agent())
        .cookie_store(cookies)
}

/// GET a bounded body. Basic auth is used when both user and password are set.
pub(crate) fn get_bytes(
    client: &Client,
    url: &str,
    auth: Option<(&str, &str)>,
    limit: u64,
) -> Result<Vec<u8>, FeedError> {
    let mut req = client.get(url);
    if let Some((user, pw)) = auth {
        if !user.is_empty() && !pw.is_empty() {
            req = req.basic_auth(user, Some(pw));
        }
    }
    send_bounded(req, limit)
}

pub(crate) fn bincraft_body(
    client: &Client,
    url: &str,
    compressed: bool,
) -> Result<Vec<u8>, FeedError> {
    if compressed {
        let body = get_bytes(client, url, None, BINCRAFT_COMPRESSED_BYTES)?;
        unzstd(&body)
    } else {
        get_bytes(client, url, None, BINCRAFT_RAW_BYTES)
    }
}

pub(crate) fn send(req: reqwest::blocking::RequestBuilder) -> Result<Vec<u8>, FeedError> {
    send_bounded(req, HTTP_JSON_BYTES)
}

fn send_bounded(req: reqwest::blocking::RequestBuilder, limit: u64) -> Result<Vec<u8>, FeedError> {
    let resp = req.send().map_err(net_err)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(FeedError::Http(format!("HTTP {}", status.as_u16())));
    }
    as_net::read_response(resp, limit).map_err(|error| match error {
        as_net::BodyError::TooLarge { limit } => FeedError::TooLarge { limit },
        as_net::BodyError::Read => FeedError::Http("response body read failed".into()),
    })
}

/// GET the body as text (lossy UTF-8) under the ordinary JSON transport cap.
pub(crate) fn http_get(
    url: &str,
    timeout_s: u64,
    auth: Option<(&str, &str)>,
) -> Result<String, FeedError> {
    http_get_bounded(url, timeout_s, auth, HTTP_JSON_BYTES)
}

pub(crate) fn http_get_bounded(
    url: &str,
    timeout_s: u64,
    auth: Option<(&str, &str)>,
    limit: u64,
) -> Result<String, FeedError> {
    let body = get_bytes(&client(timeout_s, false)?, url, auth, limit)?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

fn unzstd(data: &[u8]) -> Result<Vec<u8>, FeedError> {
    let decode_error = || FeedError::Parse("invalid compressed BinCraft response".into());
    let mut decoder = zstd::stream::read::Decoder::new(data).map_err(|_| decode_error())?;
    decoder
        .window_log_max(BINCRAFT_MAX_WINDOW_LOG)
        .map_err(|_| decode_error())?;
    as_net::read_bounded(decoder, BINCRAFT_RAW_BYTES).map_err(|error| match error {
        as_net::BodyError::TooLarge { limit } => FeedError::TooLarge { limit },
        as_net::BodyError::Read => decode_error(),
    })
}

fn net_err(e: reqwest::Error) -> FeedError {
    if e.is_timeout() {
        FeedError::Timeout
    } else {
        FeedError::Http("request failed".into())
    }
}

pub(crate) fn parse_err(e: serde_json::Error) -> FeedError {
    FeedError::Parse(format!(
        "invalid JSON at line {}, column {}",
        e.line(),
        e.column()
    ))
}

/// The envelope borrows raw record slices, so numeric decoding stays local to each row.
pub(crate) fn map_records<'a, T: DeserializeOwned>(
    records: impl IntoIterator<Item = &'a RawValue>,
    mut map: impl FnMut(T) -> Option<FeedPlane>,
) -> Result<FeedBatch, FeedError> {
    let mut batch = FeedBatch::default();
    for record in records {
        if let Ok(dto) = serde_json::from_str::<T>(record.get()) {
            batch.decoded += 1;
            if let Some(plane) = map(dto) {
                batch.planes.push(plane);
                continue;
            }
        }
        batch.rejected += 1;
    }
    if batch.rejected != 0 && batch.planes.is_empty() {
        return Err(FeedError::Parse(format!(
            "no valid aircraft in {} records",
            batch.rejected
        )));
    }
    Ok(batch)
}

#[derive(Clone, Copy, Default)]
pub(crate) enum WireAltitude {
    #[default]
    Unavailable,
    Ground,
    Feet(f64),
}

impl WireAltitude {
    pub(crate) fn feet(self) -> Option<f64> {
        match self {
            Self::Unavailable | Self::Ground => Some(NO_ALT),
            Self::Feet(feet) => convert_to_i32(feet).map(f64::from),
        }
    }

    pub(crate) fn ground(self) -> bool {
        matches!(self, Self::Ground)
    }

    pub(crate) fn has_measurement(self) -> bool {
        matches!(self, Self::Feet(_))
    }
}

pub(crate) fn de_altitude<'de, D: Deserializer<'de>>(d: D) -> Result<WireAltitude, D::Error> {
    wire_altitude(d, false)
}

pub(crate) fn de_altitude_lenient<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<WireAltitude, D::Error> {
    wire_altitude(d, true)
}

fn wire_altitude<'de, D: Deserializer<'de>>(
    d: D,
    ignore_bad_text: bool,
) -> Result<WireAltitude, D::Error> {
    use serde::de::Error;
    match Value::deserialize(d)? {
        Value::Null => Ok(WireAltitude::Unavailable),
        Value::Number(n) => n
            .as_f64()
            .map(WireAltitude::Feet)
            .ok_or_else(|| Error::custom("invalid altitude")),
        Value::String(text) => {
            let text = text.trim();
            if text.eq_ignore_ascii_case("ground") {
                return Ok(WireAltitude::Ground);
            }
            if let Some(number) = text.parse::<f64>().ok().filter(|n| n.is_finite()) {
                return Ok(WireAltitude::Feet(number));
            }
            if text.is_empty() || ignore_bad_text {
                return Ok(WireAltitude::Unavailable);
            }
            Err(Error::custom("invalid altitude string"))
        }
        _ => Err(Error::custom("invalid altitude field type")),
    }
}

pub(crate) fn epoch_seconds(value: f64) -> Option<DateTime<Utc>> {
    if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
        return None;
    }
    DateTime::from_timestamp(value as i64, 0)
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
        _ => return Err(serde::de::Error::custom("expected a string")),
    })
}

/// A `double?` DTO property: a number, a numeric string, `""`/null for null. Any other
/// string (e.g. `"ground"`) fails the whole document, as `JsonReaderException` does in C#.
pub(crate) fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(n.as_f64()),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("invalid number string")),
        _ => Err(serde::de::Error::custom("expected a number")),
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
            .ok_or_else(|| serde::de::Error::custom("integer out of range")),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("invalid integer string")),
        _ => Err(serde::de::Error::custom("expected an integer")),
    }
}

/// Check the range of an optional integer before mapping the record.
pub(crate) fn checked_optional_i32(value: Option<i64>) -> Option<Option<i32>> {
    value.map(i32::try_from).transpose().ok()
}

/// `Convert.ToInt32(double)`: round half to even and reject overflow.
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
    if s < 0.0 || !(0.0..(i64::MAX as f64)).contains(&ms) {
        return None;
    }
    t.checked_sub_signed(Duration::try_milliseconds(ms as i64)?)
}

/// `string.Trim().Replace("\"", "")` of the hex field.
pub(crate) fn clean_hex(hex: &str) -> String {
    hex.trim().replace('"', "")
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn finding_375_transport_checks_declared_and_chunked_limits() {
        let client = client(5, false).unwrap();
        let mut server = mockito::Server::new();
        let exact = server.mock("GET", "/exact").with_body("abc").create();
        let declared = server.mock("GET", "/declared").with_body("abcd").create();
        let chunked = server
            .mock("GET", "/chunked")
            .with_chunked_body(|out| out.write_all(b"abcd"))
            .create();
        let send = |suffix: &str| send_bounded(client.get(format!("{}{suffix}", server.url())), 3);
        assert_eq!(send("/exact").unwrap(), b"abc");
        for suffix in ["/declared", "/chunked"] {
            assert!(matches!(
                send(suffix),
                Err(FeedError::TooLarge { limit: 3 })
            ));
        }
        exact.assert();
        declared.assert();
        chunked.assert();
    }

    #[test]
    fn finding_375_zstd_rejects_oversize_window_and_decoded_output() {
        use std::io::Read;
        // Standard frame magic, no content size, 64 MiB window, final empty raw block.
        let oversized_window = [0x28, 0xb5, 0x2f, 0xfd, 0, 0x80, 1, 0, 0];
        assert!(unzstd(&oversized_window).is_err());
        let limit = 32 * 1024 * 1024;
        let valid = zstd::encode_all(std::io::repeat(0).take(limit), 1).unwrap();
        assert_eq!(unzstd(&valid).unwrap().len() as u64, limit);
        let oversized = zstd::encode_all(std::io::repeat(0).take(limit + 1), 1).unwrap();
        assert!(unzstd(&oversized).is_err());
        assert!(unzstd(&valid[..valid.len() - 1]).is_err());
    }
}
