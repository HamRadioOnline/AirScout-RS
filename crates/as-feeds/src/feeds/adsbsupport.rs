//! ADSB SUPPORT feed (`AirScout.PlaneFeeds.Plugin.ADSBSupport`): `{planes:[...]}` with
//! Basic auth. The password (and an optional session token) must come from the service
//! operator and are entered by the user; this port does not fetch keys from anywhere.
//!
//! Faithful to the C# plugin: the report time is replaced with the local clock, the hex is
//! not upper-cased, `onGround` is ignored. Not ported: the 30 s sleep in `Start`.

use super::{client, de_f32, de_f64, de_i64, de_string, parse_err, send, set_bool, set_str};
use super::{set_u64, NO_ALT};
use crate::{vars, wake, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::Utc;
use serde::Deserialize;

pub const DEFAULT_URL: &str = "https://airscout.hamradio4.me/planes?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%&age=300";

#[derive(Deserialize)]
struct List {
    #[serde(deserialize_with = "super::required")]
    planes: Option<Vec<Aircraft>>,
}

#[derive(Deserialize)]
struct Aircraft {
    #[serde(default, deserialize_with = "de_string")]
    hex: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    callsign: Option<String>,
    #[serde(default, deserialize_with = "de_i64")]
    altitude_barometric: Option<i64>,
    #[serde(default, deserialize_with = "de_i64")]
    altitude_gps: Option<i64>,
    #[serde(default, deserialize_with = "de_f64")]
    latitude: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    longitude: Option<f64>,
    #[serde(rename = "receivedAt", default, deserialize_with = "de_i64")]
    received_at: Option<i64>,
    #[serde(default, deserialize_with = "de_f32")]
    angle: Option<f32>,
    #[serde(default, deserialize_with = "de_f32")]
    speed: Option<f32>,
    #[serde(rename = "verticalRate", default, deserialize_with = "de_i64")]
    vertical_rate: Option<i64>,
    #[serde(default, deserialize_with = "de_string")]
    category: Option<String>,
}

fn i32_of(x: Option<i64>) -> Result<Option<i32>, FeedError> {
    x.map(|v| i32::try_from(v).map_err(|_| FeedError::Parse(format!("{v} is not an Int32"))))
        .transpose()
}

/// Map one response.
pub fn parse(json: &str, use_geo_alt: bool) -> Result<Vec<FeedPlane>, FeedError> {
    let list: List = serde_json::from_str(json).map_err(parse_err)?;
    let planes = list
        .planes
        .ok_or_else(|| FeedError::Parse("no plane list".into()))?;
    let mut out = Vec::new();
    for ac in &planes {
        let alt = i32_of(if use_geo_alt {
            ac.altitude_gps
        } else {
            ac.altitude_barometric
        })?;
        let vsi = i32_of(ac.vertical_rate)?;
        let Some(hex) = ac.hex.as_deref() else {
            continue;
        };
        if ac.received_at.is_none() {
            continue;
        }
        out.push(FeedPlane {
            hex: super::clean_hex(hex),
            lat: ac.latitude.unwrap_or(f64::NAN),
            lon: ac.longitude.unwrap_or(f64::NAN),
            alt_ft: alt.map_or(NO_ALT, f64::from),
            call: ac.callsign.clone().unwrap_or_default(),
            track: ac.angle.map_or(f64::NAN, f64::from),
            speed_kts: ac.speed.map_or(f64::NAN, f64::from),
            // receivedAt is only checked for null, then replaced by the local clock
            time: Some(Utc::now()),
            // Type is never set, so the A388 rule cannot apply
            category: wake::category(ac.category.as_deref().unwrap_or(""), ""),
            vspeed: vsi.unwrap_or(i32::MIN),
            ..FeedPlane::default()
        });
    }
    Ok(out)
}

/// The ADSBSupport feed. Password and token are never logged or shown in errors.
#[derive(Clone)]
pub struct AdsbSupport {
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub use_geo_alt: bool,
    pub timeout_s: u64,
}

impl std::fmt::Debug for AdsbSupport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdsbSupport")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("use_geo_alt", &self.use_geo_alt)
            .field("timeout_s", &self.timeout_s)
            .finish_non_exhaustive()
    }
}

impl Default for AdsbSupport {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            username: "airscout".into(),
            password: String::new(),
            token: String::new(),
            use_geo_alt: false,
            timeout_s: 30,
        }
    }
}

impl PlaneFeed for AdsbSupport {
    fn name(&self) -> &'static str {
        "ADSBSupport"
    }

    fn info(&self) -> &'static str {
        "Web feed from ADSB SUPPORT ApS, Denmark (https://www.adsbsupport.com/)."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        Some(
            "This feed needs credentials (password, and possibly a session token) from the \
             service operator. Enter them in the feed settings; AirScout-RS does not obtain \
             them for you.",
        )
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn secret_keys(&self) -> &'static [&'static str] {
        &["username", "password", "token"]
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t.insert("username".into(), self.username.clone().into());
        t.insert("password".into(), self.password.clone().into());
        t.insert("token".into(), self.token.clone().into());
        t.insert("use_geo_alt".into(), self.use_geo_alt.into());
        t.insert("timeout_s".into(), (self.timeout_s as i64).into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_str(t, "username", &mut self.username)?;
        set_str(t, "password", &mut self.password)?;
        set_str(t, "token", &mut self.token)?;
        set_bool(t, "use_geo_alt", &mut self.use_geo_alt)?;
        set_u64(t, "timeout_s", &mut self.timeout_s)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        if self.password.is_empty() {
            return Err(FeedError::Config(
                "ADSBSupport needs a password from the service operator".into(),
            ));
        }
        let url = vars::substitute(&self.url, args, Utc::now(), "")?;
        let mut req = client(self.timeout_s, false)?
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::ACCEPT, "application/json");
        if !self.token.is_empty() {
            req = req.header("token", &self.token);
        }
        let body = send(req)?;
        parse(&String::from_utf8_lossy(&body), self.use_geo_alt)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
