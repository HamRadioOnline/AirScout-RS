//! OpenSky Network feed (`AirScout.PlaneFeeds.Plugin.OpenSky`): `/api/states/all`,
//! anonymous or with an OAuth2 client-credentials token.
//!
//! Faithful to the C# plugin: the configured URL is requested as is, so the covered area is
//! never sent (the substituted URL is computed and not used); `Type` is read from the
//! longitude field; a null callsign drops the state. Unlike C#, the token is cached until
//! it expires instead of being fetched for every request.

use super::{client, parse_err, send, set_str};
use crate::vars::format_double;
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::DateTime;
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

/// `ReadPropertyInt`.
fn prop_int(v: Option<&Value>) -> Option<i32> {
    let d = prop_double(v);
    (d != f64::MIN && d >= f64::from(i32::MIN) && d <= f64::from(i32::MAX)).then_some(d as i32)
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
    let call = prop_string(a.get(1))?.to_uppercase().trim().to_string();
    let lon = prop_double(a.get(5));
    let lat = prop_double(a.get(6));
    let alt_ft = prop_double(a.get(13)) * 3.28084;
    let track = prop_double(a.get(10));
    let speed_kts = prop_double(a.get(9)) * 3.6 / 1.852;
    let time = DateTime::from_timestamp(i64::from(prop_int(a.get(3))?), 0)?;
    let type_code = prop_string(a.get(5)).unwrap_or_default();
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
        track,
        speed_kts,
        type_code,
        ..FeedPlane::default()
    })
}

/// Map one `/api/states/all` response.
pub fn parse(json: &str) -> Result<Vec<FeedPlane>, FeedError> {
    if !json.starts_with('{') {
        let head: String = json.chars().take(80).collect();
        return Err(FeedError::Parse(format!("Response: {head}")));
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
        f.debug_struct("OpenSky")
            .field("url", &self.url)
            .field("token_url", &self.token_url)
            .field("oauth_client_id", &self.oauth_client_id)
            .finish_non_exhaustive()
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
        // `OpenSky.cs:193-195` (`Load`) at fbadef0, "correct URL with V1.4.7.0 automatically":
        // a saved URL on the old API host is rewritten; it runs after the older V1.4.2.1 and
        // V1.4.4.0 URL fixes, so it decides the final value (the `:187` reset reads legacy XML
        // only; see #340). C# then calls `Save(true)`; a plugin cannot save here, so the migrated
        // value lives in the plugin and the next load migrates again.
        self.url = self.url.replace(
            "https://api.opensky-network.org/api/",
            "https://opensky-network.org/api/",
        );
        set_str(t, "token_url", &mut self.token_url)?;
        set_str(t, "oauth_client_id", &mut self.oauth_client_id)?;
        set_str(t, "oauth_secret", &mut self.oauth_secret)?;
        self.token = None;
        Ok(())
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, _args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let c = self.client()?;
        let mut req = c.get(&self.url);
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
