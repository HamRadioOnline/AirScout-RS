//! OpenSky Network feed (`AirScout.PlaneFeeds.Plugin.OpenSky`): `/api/states/all`,
//! anonymous or with an OAuth2 client-credentials token.
//!
//! Each observation takes its position timestamp from the OpenSky state vector.

use super::{client, epoch_seconds, parse_err, send, set_str};
use crate::vars::{format_double, substitute_supported};
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_aircraft::AltitudeProvenance;
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;
use std::time::{Duration, Instant};

/// `OpenSky.cs:63` at fbadef0 (V1.4.7.0 moved the API to the main host).
pub const DEFAULT_URL: &str = "https://opensky-network.org/api/states/all";
pub const DEFAULT_TOKEN_URL: &str =
    "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token";

/// `WebClient` / `HttpClient` default timeout.
const TIMEOUT_S: u64 = 100;
/// Renew the token this long before it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(30);

/// `ReadPropertyString`: the JSON value converted to a string, `None` for null or a
/// value that does not convert.
fn prop_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(match n.as_f64() {
            Some(x) if !(n.is_i64() || n.is_u64()) => format_double(x),
            _ => n.to_string(),
        }),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.into()),
        _ => None,
    }
}

/// `ReadPropertyDouble`: `double.MinValue` for null, empty, unparsable or `-INF…`.
fn prop_double(v: Option<&Value>) -> f64 {
    let s = match v {
        Some(Value::Number(n)) => return n.as_f64().unwrap_or(f64::MIN),
        Some(Value::String(s)) => s.trim().to_string(),
        _ => return f64::MIN,
    };
    let u = s.to_uppercase();
    if s.is_empty() || u.starts_with("-INF") {
        f64::MIN
    } else if u.starts_with("+INF") {
        f64::MAX
    } else if u.starts_with("NAN") || u.starts_with("NULL") {
        f64::NAN
    } else {
        s.parse().unwrap_or(f64::MIN)
    }
}

/// `ReadPropertyBool`: the value's text equals "true" (case-insensitive).
fn prop_bool(v: Option<&Value>) -> bool {
    prop_string(v).is_some_and(|s| s.to_lowercase() == "true")
}

fn map_state(ac: &Value) -> Option<FeedPlane> {
    let a = ac.as_array()?;
    if a.len() < 17 {
        return None;
    }
    let hex = prop_string(a.first())?.to_uppercase().trim().to_string();
    let call = prop_string(a.get(1))
        .unwrap_or_default()
        .trim()
        .to_uppercase();
    let lon = prop_double(a.get(5));
    let lat = prop_double(a.get(6));
    let alt_ft = prop_double(a.get(13)) * 3.28084;
    let track = prop_double(a.get(10));
    let speed_kts = prop_double(a.get(9)) * 3.6 / 1.852;
    let time = epoch_seconds(prop_double(a.get(3)))?;
    if prop_bool(a.get(8)) || speed_kts <= 0.0 {
        return None;
    }
    Some(FeedPlane {
        time: Some(time),
        hex,
        call,
        lat,
        lon,
        alt_ft,
        // index 13 is `geo_altitude`; index 7 (`baro_altitude`) is not read
        altitude_provenance: if alt_ft.is_finite() && alt_ft != f64::MIN {
            AltitudeProvenance::Geometric
        } else {
            AltitudeProvenance::Unknown
        },
        track,
        speed_kts,
        ..FeedPlane::default()
    })
}

/// Map one `/api/states/all` response.
pub fn parse(json: &str) -> Result<Vec<FeedPlane>, FeedError> {
    if !json.starts_with('{') {
        return Err(FeedError::Parse("expected an OpenSky JSON object".into()));
    }
    let root: Value = serde_json::from_str(json).map_err(parse_err)?;
    Ok(match root.get("states").and_then(Value::as_array) {
        Some(states) => states.iter().filter_map(map_state).collect(),
        None => Vec::new(),
    })
}

/// The OpenSky feed. The OAuth secret lives only in the user's settings.
#[derive(Clone)]
pub struct OpenSky {
    pub url: String,
    pub token_url: String,
    pub oauth_client_id: String,
    pub oauth_secret: String,
    token: Option<(String, Instant)>,
    client: Option<Client>,
}

impl std::fmt::Debug for OpenSky {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenSky").finish_non_exhaustive()
    }
}

impl Default for OpenSky {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            oauth_client_id: String::new(),
            oauth_secret: String::new(),
            token: None,
            client: None,
        }
    }
}

impl OpenSky {
    fn client(&mut self) -> Result<Client, FeedError> {
        if self.client.is_none() {
            self.client = Some(client(TIMEOUT_S, false)?);
        }
        self.client
            .clone()
            .ok_or_else(|| FeedError::Http("no HTTP client".into()))
    }

    /// A valid access token, fetched with the client credentials when none is cached.
    fn access_token(&mut self, c: &Client) -> Result<String, FeedError> {
        if let Some((t, until)) = &self.token {
            if Instant::now() < *until {
                return Ok(t.clone());
            }
        }
        let body = send(
            c.post(&self.token_url)
                .basic_auth(&self.oauth_client_id, Some(&self.oauth_secret))
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body("grant_type=client_credentials"),
        )?;
        let v: Value = serde_json::from_slice(&body).map_err(parse_err)?;
        let token = v
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| FeedError::Parse("token response without access_token".into()))?
            .to_string();
        let life = Duration::from_secs(v.get("expires_in").and_then(Value::as_u64).unwrap_or(0));
        // an absurd expires_in that overflows the clock: use the token once, fetch a new one
        // next time
        self.token = Instant::now()
            .checked_add(life.saturating_sub(TOKEN_MARGIN))
            .map(|until| (token.clone(), until));
        Ok(token)
    }
}

fn is_stock_endpoint(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    let authority = authority.strip_suffix(":443").unwrap_or(authority);
    scheme.eq_ignore_ascii_case("https")
        && authority.eq_ignore_ascii_case("opensky-network.org")
        && path == "api/states/all"
}

fn request_url(template: &str, args: &FeedArgs) -> String {
    let url = substitute_supported(template, args, Utc::now());
    if !is_stock_endpoint(&url) {
        return url;
    }
    format!(
        "{url}?lamin={}&lomin={}&lamax={}&lomax={}",
        format_double(args.min_lat),
        format_double(args.min_lon),
        format_double(args.max_lat),
        format_double(args.max_lon)
    )
}

impl PlaneFeed for OpenSky {
    fn name(&self) -> &'static str {
        "OpenSky"
    }

    fn info(&self) -> &'static str {
        "Web feed from the OpenSky Network (anonymous, or with an API client id and secret)."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        None
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn secret_keys(&self) -> &'static [&'static str] {
        &["oauth_client_id", "oauth_secret"]
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t.insert("token_url".into(), self.token_url.clone().into());
        t.insert(
            "oauth_client_id".into(),
            self.oauth_client_id.clone().into(),
        );
        t.insert("oauth_secret".into(), self.oauth_secret.clone().into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_str(t, "token_url", &mut self.token_url)?;
        set_str(t, "oauth_client_id", &mut self.oauth_client_id)?;
        set_str(t, "oauth_secret", &mut self.oauth_secret)?;
        self.token = None;
        Ok(())
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let c = self.client()?;
        let mut req = c.get(request_url(&self.url, args));
        if !self.oauth_client_id.is_empty() && !self.oauth_secret.is_empty() {
            req = req.bearer_auth(self.access_token(&c)?);
        }
        let body = send(req)?;
        parse(&String::from_utf8_lossy(&body))
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_64_stock_bbox_applies_only_to_the_exact_canonical_endpoint() {
        let args = FeedArgs {
            min_lat: 47.5,
            min_lon: -11.25,
            max_lat: 54.0,
            max_lon: 8.5,
            ..Default::default()
        };
        for stock in [
            DEFAULT_URL,
            "HTTPS://OPENSKY-NETWORK.ORG:443/api/states/all",
        ] {
            assert_eq!(
                request_url(stock, &args),
                format!("{stock}?lamin=47.5&lomin=-11.25&lamax=54&lomax=8.5")
            );
        }
        for custom in [
            "http://opensky-network.org/api/states/all",
            "https://opensky-network.org:444/api/states/all",
            "https://u@opensky-network.org/api/states/all",
            "https://opensky-network.org/api/states/all?",
            "https://opensky-network.org/api/states/all#",
            "https://opensky-network.org/api/../api/states/all",
            "https://opensky-network.org/api/states/all/extension",
            "https://localhost/api/states/all",
        ] {
            assert_eq!(request_url(custom, &args), custom);
        }
        assert_eq!(
            request_url("http://localhost/?x=%2f%MINLAT%&y=%UNKNOWN%", &args),
            "http://localhost/?x=%2f47.5&y=%UNKNOWN%"
        );
    }
}
