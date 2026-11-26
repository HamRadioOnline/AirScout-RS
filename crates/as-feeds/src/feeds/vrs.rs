//! Virtual Radar Server feed (`AirScout.PlaneFeeds.Plugin.VRSWebServer`):
//! `AircraftList.json` with optional Basic auth, a second server for failover or load
//! sharing, and the `lastDv` data version fed back through `%LASTDV%`.
//!
//! Load sharing compares each endpoint’s cumulative monotonic service time.
//! Each request uses the credentials of its selected endpoint.

use super::{checked_optional_i32, map_records, set_bool, set_str, set_u64, unix_ms, NO_ALT};
use super::{clean_hex, de_f32, de_f64, de_i64, de_string, http_get, parse_err};
use crate::{vars, FeedArgs, FeedBatch, FeedError, FeedPlane, PlaneFeed};
use as_aircraft::AltitudeProvenance;
use chrono::Utc;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::time::{Duration, Instant};

pub const DEFAULT_URL: &str = "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%";

#[derive(Deserialize)]
struct List<'a> {
    #[serde(rename = "acList", borrow, deserialize_with = "super::required")]
    ac_list: Option<Vec<&'a RawValue>>,
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

fn map(ac: &Aircraft, use_geo_alt: bool) -> Option<FeedPlane> {
    let alt = checked_optional_i32(if use_geo_alt { ac.g_alt } else { ac.alt })?;
    let altitude_provenance = match (use_geo_alt, alt.is_some()) {
        (true, true) => AltitudeProvenance::Geometric,
        (false, true) => AltitudeProvenance::PressureEstimate,
        (_, false) => AltitudeProvenance::Unknown,
    };
    let wtc = checked_optional_i32(ac.wtc)?;
    let vsi = checked_optional_i32(ac.vsi)?;
    let icao = ac.icao.as_deref()?;
    let time = ac.pos_time.and_then(unix_ms)?;
    let type_code = ac.r#type.clone().unwrap_or_default();
    // WTC 1..4 map 1:1, anything else (0 = none, null) to 0
    let category = wtc.filter(|c| (1..=4).contains(c)).unwrap_or(0);
    Some(FeedPlane {
        time: Some(time),
        hex: clean_hex(icao),
        lat: ac.lat.unwrap_or(f64::NAN),
        lon: ac.long.unwrap_or(f64::NAN),
        alt_ft: alt.map_or(NO_ALT, f64::from),
        altitude_provenance,
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
    })
}

/// Map one response; also returns `lastDv`.
pub fn parse(json: &str, use_geo_alt: bool) -> Result<(Vec<FeedPlane>, String), FeedError> {
    let (batch, version) = parse_batch(json, use_geo_alt)?;
    Ok((batch.planes, version))
}

fn parse_batch(json: &str, use_geo_alt: bool) -> Result<(FeedBatch, String), FeedError> {
    let list: List<'_> = serde_json::from_str(json).map_err(parse_err)?;
    let ac = list
        .ac_list
        .ok_or_else(|| FeedError::Parse("no aircraft list".into()))?;
    let batch = map_records(ac, |a: Aircraft| map(&a, use_geo_alt))?;
    Ok((batch, list.last_dv.unwrap_or_default()))
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

fn record_duration(count: &mut i64, elapsed: Duration) {
    *count = count.saturating_add(i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX));
}

impl Vrs {
    /// GET `template` with the given credentials and record elapsed time on success.
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
        record_duration(count, start.elapsed());
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
        Ok(self.get_batch(args)?.planes)
    }

    fn get_batch(&mut self, args: &FeedArgs) -> Result<FeedBatch, FeedError> {
        let json = self.download(args)?;
        let (batch, last_dv) = parse_batch(&json, self.use_geo_alt)?;
        self.last_dv = last_dv;
        Ok(batch)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_58_load_sharing_uses_full_elapsed_time() {
        let mut server = mockito::Server::new();
        let faster = server.mock("GET", "/fast").with_body("fast").create();
        let mut feed = Vrs {
            url: format!("{}/slow", server.url()),
            url2: format!("{}/fast", server.url()),
            ..Default::default()
        };
        record_duration(&mut feed.count1, Duration::from_millis(1200));
        record_duration(&mut feed.count2, Duration::from_millis(900));
        assert_eq!((feed.count1, feed.count2), (1200, 900));
        assert_eq!(feed.download(&FeedArgs::default()).unwrap(), "fast");
        faster.assert();

        let mut count = i64::MAX - 1;
        record_duration(&mut count, Duration::MAX);
        assert_eq!(count, i64::MAX);
    }
}
