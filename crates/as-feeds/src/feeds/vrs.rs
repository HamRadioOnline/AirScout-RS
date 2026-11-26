//! Virtual Radar Server feed (`AirScout.PlaneFeeds.Plugin.VRSWebServer`):
//! `AircraftList.json` with optional Basic auth, a second server for failover or load
//! sharing, and the `lastDv` data version fed back through `%LASTDV%`.
//!
//! Faithful to the C# plugin: the hex is not upper-cased; without load sharing the second
//! URL is tried with the first URL's credentials; load sharing compares sums of the
//! millisecond *component* of each request time (`TimeSpan.Milliseconds`).

use super::{clean_hex, de_f32, de_f64, de_i64, de_string, http_get, parse_err};
use super::{set_bool, set_str, set_u64, unix_ms, NO_ALT};
use crate::{vars, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::Utc;
use serde::Deserialize;
use std::time::Instant;

pub const DEFAULT_URL: &str = "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%";

#[derive(Deserialize)]
struct List {
    #[serde(rename = "acList", deserialize_with = "super::required")]
    ac_list: Option<Vec<Aircraft>>,
    #[serde(rename = "lastDv", default, deserialize_with = "de_string")]
    last_dv: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Aircraft {
    #[serde(default, deserialize_with = "de_string")]
    icao: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    reg: Option<String>,
    #[serde(default, deserialize_with = "de_i64")]
    alt: Option<i64>,
    #[serde(rename = "GAlt", default, deserialize_with = "de_i64")]
    g_alt: Option<i64>,
    #[serde(default, deserialize_with = "de_string")]
    call: Option<String>,
    #[serde(default, deserialize_with = "de_f64")]
    lat: Option<f64>,
    #[serde(default, deserialize_with = "de_f64")]
    long: Option<f64>,
    #[serde(default, deserialize_with = "de_i64")]
    pos_time: Option<i64>,
    #[serde(default, deserialize_with = "de_f32")]
    spd: Option<f32>,
    #[serde(default, deserialize_with = "de_f32")]
    trak: Option<f32>,
    #[serde(default, deserialize_with = "de_string")]
    r#type: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    mdl: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    man: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    from: Option<String>,
    #[serde(default, deserialize_with = "de_string")]
    to: Option<String>,
    #[serde(default, deserialize_with = "de_i64")]
    vsi: Option<i64>,
    #[serde(rename = "WTC", default, deserialize_with = "de_i64")]
    wtc: Option<i64>,
    #[serde(default, deserialize_with = "de_string")]
    cou: Option<String>,
}

/// `int?` property values outside Int32 make Newtonsoft fail the document.
fn i32_of(x: Option<i64>) -> Result<Option<i32>, FeedError> {
    x.map(|v| i32::try_from(v).map_err(|_| FeedError::Parse(format!("{v} is not an Int32"))))
        .transpose()
}

fn map(ac: &Aircraft, use_geo_alt: bool) -> Result<Option<FeedPlane>, FeedError> {
    let alt = i32_of(if use_geo_alt { ac.g_alt } else { ac.alt })?;
    let wtc = i32_of(ac.wtc)?;
    let vsi = i32_of(ac.vsi)?;
    // a null Icao throws in C#: the plane is skipped
    let Some(icao) = ac.icao.as_deref() else {
        return Ok(None);
    };
    let Some(time) = ac.pos_time.and_then(unix_ms) else {
        return Ok(None);
    };
    let type_code = ac.r#type.clone().unwrap_or_default();
    // WTC 1..4 map 1:1, anything else (0 = none, null) to 0
    let category = wtc.filter(|c| (1..=4).contains(c)).unwrap_or(0);
    Ok(Some(FeedPlane {
        time: Some(time),
        hex: clean_hex(icao),
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.long.unwrap_or(f64::NAN),
        alt_ft: alt.map_or(NO_ALT, f64::from),
        call: ac.call.clone().unwrap_or_default(),
        reg: ac.reg.clone().unwrap_or_default(),
        track: ac.trak.map_or(f64::NAN, f64::from),
        speed_kts: ac.spd.map_or(f64::NAN, f64::from),
        category: if type_code == "A388" { 4 } else { category },
        type_code,
        manufacturer: ac.man.clone().unwrap_or_default(),
        model: ac.mdl.clone().unwrap_or_default(),
        country: ac.cou.clone().unwrap_or_default(),
        from: ac.from.clone().unwrap_or_default(),
        to: ac.to.clone().unwrap_or_default(),
        vspeed: vsi.unwrap_or(i32::MIN),
        ..FeedPlane::default()
    }))
}

/// Map one response; also returns `lastDv`.
pub fn parse(json: &str, use_geo_alt: bool) -> Result<(Vec<FeedPlane>, String), FeedError> {
    let list: List = serde_json::from_str(json).map_err(parse_err)?;
    let ac = list
        .ac_list
        .ok_or_else(|| FeedError::Parse("no aircraft list".into()))?;
    let mut planes = Vec::new();
    for a in &ac {
        if let Some(p) = map(a, use_geo_alt)? {
            planes.push(p);
        }
    }
    Ok((planes, list.last_dv.unwrap_or_default()))
}

/// The VRS feed. Passwords live only in the user's settings and are never logged.
#[derive(Clone)]
pub struct Vrs {
    pub url: String,
    pub url2: String,
    pub username: String,
    pub password: String,
    pub username2: String,
    pub password2: String,
    pub load_share: bool,
    pub use_geo_alt: bool,
    pub timeout_s: u64,
    last_dv: String,
    count1: i64,
    count2: i64,
}

impl std::fmt::Debug for Vrs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vrs")
            .field("url", &self.url)
            .field("url2", &self.url2)
            .field("load_share", &self.load_share)
            .field("use_geo_alt", &self.use_geo_alt)
            .field("timeout_s", &self.timeout_s)
            .finish_non_exhaustive()
    }
}

impl Default for Vrs {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            url2: String::new(),
            username: String::new(),
            password: String::new(),
            username2: String::new(),
            password2: String::new(),
            load_share: true,
            use_geo_alt: false,
            timeout_s: 30,
            last_dv: String::new(),
            count1: 0,
            count2: 0,
        }
    }
}

impl Vrs {
    /// GET `template` with the given credentials; adds the millisecond component of the
    /// request time to `count` on success.
    fn fetch(
        &self,
        template: &str,
        auth: (&str, &str),
        args: &FeedArgs,
        count: &mut i64,
    ) -> Result<String, FeedError> {
        let url = vars::substitute(template, args, Utc::now(), &self.last_dv)?;
        let start = Instant::now();
        let json = http_get(&url, self.timeout_s, Some(auth))?;
        *count += i64::from(start.elapsed().subsec_millis());
        Ok(json)
    }

    fn download(&mut self, args: &FeedArgs) -> Result<String, FeedError> {
        let (mut c1, mut c2) = (self.count1, self.count2);
        let a1 = (self.username.as_str(), self.password.as_str());
        let a2 = (self.username2.as_str(), self.password2.as_str());
        let json = if self.url2.is_empty() {
            c1 = 0;
            c2 = 0;
            self.fetch(&self.url, a1, args, &mut 0)?
        } else if !self.load_share || c1 <= c2 {
            // without load sharing C# tries URL2 with the credentials of URL1 (logic-findings
            // #57); not reproduced, it would disclose server 1's password to server 2
            match self.fetch(&self.url, a1, args, &mut c1) {
                Ok(j) => j,
                Err(_) => self.fetch(&self.url2, a2, args, &mut c2)?,
            }
        } else {
            match self.fetch(&self.url2, a2, args, &mut c2) {
                Ok(j) => j,
                Err(_) => self.fetch(&self.url, a1, args, &mut c1)?,
            }
        };
        if c1 > i64::MAX / 2 || c2 > i64::MAX / 2 {
            c1 = 0;
            c2 = 0;
        }
        self.count1 = c1;
        self.count2 = c2;
        Ok(json)
    }
}

impl PlaneFeed for Vrs {
    fn name(&self) -> &'static str {
        "VRSWebServer"
    }

    fn info(&self) -> &'static str {
        "Web feed from a Virtual Radar Server (AircraftList.json), with an optional second \
         server for failover or load sharing."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        None
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn secret_keys(&self) -> &'static [&'static str] {
        &["username", "password", "username2", "password2"]
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        for (k, v) in [
            ("url", &self.url),
            ("url2", &self.url2),
            ("username", &self.username),
            ("password", &self.password),
            ("username2", &self.username2),
            ("password2", &self.password2),
        ] {
            t.insert(k.into(), v.clone().into());
        }
        t.insert("load_share".into(), self.load_share.into());
        t.insert("use_geo_alt".into(), self.use_geo_alt.into());
        t.insert("timeout_s".into(), (self.timeout_s as i64).into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_str(t, "url2", &mut self.url2)?;
        set_str(t, "username", &mut self.username)?;
        set_str(t, "password", &mut self.password)?;
        set_str(t, "username2", &mut self.username2)?;
        set_str(t, "password2", &mut self.password2)?;
        set_bool(t, "load_share", &mut self.load_share)?;
        set_bool(t, "use_geo_alt", &mut self.use_geo_alt)?;
        set_u64(t, "timeout_s", &mut self.timeout_s)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let json = self.download(args)?;
        let (planes, last_dv) = parse(&json, self.use_geo_alt)?;
        self.last_dv = last_dv;
        Ok(planes)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
