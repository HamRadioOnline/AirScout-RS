//! The content routes of the web server (`AirScout/Webserver.cs:451-498`, `:1050-1104`).
//!
//! No route serves a credential: feed usernames, passwords, tokens and OAuth secrets
//! stay in the settings file, and so does the Thunderforest key (security deviation).
//! Error texts never echo a settings value.

use crate::services::Paths;
use crate::web_paths;
use crate::web_settings_json::settings_json;
use crate::web_stations;
use as_aircraft::db::AircraftDb;
use as_aircraft::{PlaneCategory, PlaneInfoCache};
use as_geo::band::Band;
use as_settings::Settings;
use as_web::dto::{AirportJson, NameValueJson};
use as_web::json::{to_compact, to_indented, Num};
use as_web::planes::{planes_json, PLANES_JSON_VERSION};
use as_web::query::Query;
use as_web::route::ApiRoute;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// The answer to a request that starts after the server stopped (new design: C# aborts
/// it).
const STOPPED: &str = "Error: web server stopped";

/// The VRS feed, the only one whose settings the page reads (`dist/index.html:836-840`).
const VRS_FEED: &str = "VRSWebServer";

/// What the routes read.
pub struct WebCtx {
    pub paths: Paths,
    pub cache: Arc<PlaneInfoCache>,
    /// The settings as the app has them now.
    pub live: Arc<RwLock<Settings>>,
    /// The aircraft database, if the caller has one open; else [`WebApi::aircraft`]
    /// opens it.
    pub aircraft: Option<Arc<AircraftDb>>,
    /// The directory of the VRS aircraft list (`<data dir>/tmp`).
    pub tmp_dir: PathBuf,
    /// Set when the server stops; it cancels running path calculations.
    pub stop: Arc<AtomicBool>,
}

/// Where the web API reports an error (the status line), each kind once.
pub type ErrorSink = Box<dyn Fn(String) + Send + Sync>;

/// The application side of the web server.
pub struct WebApi {
    pub ctx: WebCtx,
    /// The aircraft database opened on a request, when `ctx.aircraft` is `None`.
    opened: OnceLock<Arc<AircraftDb>>,
    /// Set once an open error was reported.
    reported: AtomicBool,
    on_error: Option<ErrorSink>,
}

impl WebApi {
    /// The web API over `ctx`; its errors are only answered.
    pub fn new(ctx: WebCtx) -> Self {
        WebApi {
            ctx,
            opened: OnceLock::new(),
            reported: AtomicBool::new(false),
            on_error: None,
        }
    }

    /// The web API over `ctx`; an aircraft database open error also goes to `on_error`.
    pub fn with_errors(ctx: WebCtx, on_error: ErrorSink) -> Self {
        WebApi {
            on_error: Some(on_error),
            ..WebApi::new(ctx)
        }
    }
}

impl as_web::server::Api for WebApi {
    fn answer(&self, route: ApiRoute, _raw_url: &str, query: &Query) -> String {
        // A request that starts after stop reads and writes no database.
        if self.ctx.stop.load(Ordering::Relaxed) {
            return STOPPED.to_string();
        }
        // one read per request; a poisoned lock still holds the last settings
        let s = self
            .ctx
            .live
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match route {
            ApiRoute::Version => version(),
            ApiRoute::Settings => settings_json(&s),
            ApiRoute::Bands => bands(),
            ApiRoute::BandValues => band_values(),
            ApiRoute::AircraftCategories => aircraft_categories(),
            ApiRoute::Airports => self.airports(),
            ApiRoute::PlaneFeedSettings => plane_feed_settings(&s),
            ApiRoute::Location => web_stations::location(&self.ctx, &s, query),
            ApiRoute::Qrv => web_stations::qrv(&self.ctx, &s, query),
            ApiRoute::ElevationPath => web_paths::elevation_path(&self.ctx, &s, query),
            ApiRoute::PropagationPath => web_paths::propagation_path(&self.ctx, &s, query),
            ApiRoute::NearestPlanes => {
                web_paths::nearest_planes(&self.ctx, &s, query, chrono::Utc::now())
            }
            ApiRoute::AircraftList => file_or_error(&self.ctx.tmp_dir, "vrsplanes.json"),
            ApiRoute::Planes => {
                forecast_json(&self.ctx.cache, Utc::now(), s.planes_position_ttl_min)
            }
        }
    }
}

fn forecast_json(cache: &PlaneInfoCache, now: DateTime<Utc>, ttl_min: i64) -> String {
    planes_json(&cache.get_all(now, ttl_min), PLANES_JSON_VERSION)
}

impl WebApi {
    /// The aircraft database: `ctx.aircraft`, else the one opened earlier, else a new
    /// open of `<data dir>/aircrafts.db3`. A failed open is tried again on the next call;
    /// only the first error goes to the error sink.
    pub fn aircraft(&self) -> Option<&Arc<AircraftDb>> {
        if let Some(db) = self.ctx.aircraft.as_ref().or_else(|| self.opened.get()) {
            return Some(db);
        }
        match AircraftDb::open(&self.ctx.paths.data_dir) {
            Ok(db) => {
                // Two requests may open at once; the first one is kept.
                let _ = self.opened.set(Arc::new(db));
                self.opened.get()
            }
            Err(e) => {
                if !self.reported.swap(true, Ordering::Relaxed) {
                    if let Some(report) = &self.on_error {
                        report(format!("Web server: aircraft database: {e}"));
                    }
                }
                None
            }
        }
    }

    /// `/airports.json` (`Webserver.cs:1050-1056`). C# dereferences the database without
    /// a check; here a missing database or a read error answers (L8 deviation).
    fn airports(&self) -> String {
        let Some(db) = self.aircraft() else {
            return "Error: airports: no aircraft database".to_string();
        };
        match db.airport_get_all() {
            Ok(rows) => {
                let rows: Vec<AirportJson> = rows
                    .into_iter()
                    .map(|a| AirportJson {
                        icao: a.icao,
                        iata: a.iata,
                        lat: Num(a.lat),
                        lon: Num(a.lon),
                        alt: Num(a.alt),
                        airport: a.airport,
                        country: a.country,
                        last_updated: a.last_updated,
                    })
                    .collect();
                to_compact(&rows)
            }
            Err(e) => format!("Error: airports: {e}"),
        }
    }
}

/// `/planes.json` and `/AircraftList.json` (`Webserver.cs:429-449`, `:1155`, `:1161`): the
/// file `<tmp_dir>/<name>` as text (a leading BOM dropped, as `StreamReader` does). C#
/// throws when the file is missing and the request never completes; here it answers
/// `"Error: <name> not found!"` (L8 deviation).
fn file_or_error(tmp_dir: &Path, name: &str) -> String {
    match std::fs::read(tmp_dir.join(name)) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
        }
        Err(_) => format!("Error: {name} not found!"),
    }
}

/// The feed name the page sees in `Planes_PlaneFeed1` and `/planefeedsettings.json`.
///
/// C# serves the plugin name (`VRSWebServer.cs:380`), which never equals the JS class
/// name `VRSWebServer` that `dist/index.html:836` compares against, so the upstream page
/// fetches no planes. This port serves the Rust feed name unchanged; for VRS it is that
/// class name (integration-layer bug, not reproduced).
pub fn feed_web_name(feed: &str) -> String {
    feed.to_string()
}

/// The schemes a served URL is used with: the VRS feed (reqwest) and the KST websocket.
const URL_SCHEMES: [&str; 4] = ["http", "https", "ws", "wss"];

/// `raw` without userinfo (`user:password@`), so no URL credential reaches the LAN.
///
/// `raw` is parsed with the `url` crate, the WHATWG parser that reqwest uses too, so the
/// rule cannot differ from what a client reads as credentials.
/// - An `http`, `https`, `ws` or `wss` URL without userinfo is served byte-identical.
/// - Such a URL with userinfo is served in the crate's canonical form without it.
/// - Anything else (another scheme, or no URL) is served from after its last `@`, if
///   it has one; the text then holds no `@`, so no parser can find userinfo in it.
pub fn strip_userinfo(raw: &str) -> String {
    if let Ok(mut u) = url::Url::parse(raw) {
        if URL_SCHEMES.contains(&u.scheme()) {
            if u.username().is_empty() && u.password().is_none() {
                return raw.to_string();
            }
            // cannot fail: a URL with userinfo has a non-empty host
            let _ = u.set_username("");
            let _ = u.set_password(None);
            return u.into();
        }
    }
    raw.rfind('@').map_or(raw, |at| &raw[at + 1..]).to_string()
}

/// The `%NAME%` placeholders the shipped page substitutes into the VRS feed URL
/// (`airscout.feeds.js:74-96`), kept through the query scrub so the browser-side feed keeps
/// working (`#257`).
const VRS_PLACEHOLDERS: [&str; 14] = [
    "MINLAT",
    "MAXLAT",
    "MINLON",
    "MAXLON",
    "MINALTM",
    "MAXALTM",
    "MINALTFT",
    "MAXALTFT",
    "UNIXTIME",
    "MYLAT",
    "MYLON",
    "MINDISTKM",
    "MAXDISTKM",
    "LASTDV",
];

/// `raw` as served to the LAN in `/planefeedsettings.json` (`#257`, `#277`).
///
/// The value is parsed with the `url` crate — the WHATWG parser reqwest uses — so the served form
/// can never differ from what a client reads as credentials. Anything that is not a clean absolute
/// URL is **not served** (the empty string): a value that does not parse, holds more than one `@`,
/// or whose userinfo holds a literal `?`/`#` (an ambiguous credential boundary). Otherwise the
/// userinfo and the fragment are dropped, and the query keeps only the `%NAME%` placeholders the
/// page substitutes. A credential in the URL path is **not** removed — the browser-side feed needs
/// the path, so a path-embedded token still reaches the LAN (owner decision B: warn against path
/// tokens, do not strip them). The KST websocket drops its whole query instead via
/// [`scrub_public_url_without_query`].
pub fn scrub_public_url(raw: &str) -> String {
    scrub_public(raw, true)
}

/// [`scrub_public_url`] but with the query string dropped unconditionally: the KST websocket does
/// not substitute the page placeholders (`airscout.wtkst.js`), so no query part survives (#277).
pub fn scrub_public_url_without_query(raw: &str) -> String {
    scrub_public(raw, false)
}

/// The shared implementation of [`scrub_public_url`] and [`scrub_public_url_without_query`].
///
/// `raw` is parsed with the `url` crate (the WHATWG parser reqwest uses), so the served form can
/// never differ from what a client reads as credentials. The value is served as the empty string
/// when it does not parse as an absolute `http`/`https`/`ws`/`wss` URL, when it holds more than
/// one `@` (an `@` in a password would otherwise be folded into the userinfo by the parser), or
/// when an `@` sits outside the userinfo (a literal `?`, `#`, `/` or `\` inside a password ends
/// the authority, so the credential prefix would be served as a bare host — ambiguous, not
/// served). Otherwise the userinfo and the fragment are cleared, and
/// the query keeps only the `%NAME%` placeholder parameters when `keep_placeholders` is set, or
/// is dropped entirely otherwise.
fn scrub_public(raw: &str, keep_placeholders: bool) -> String {
    let Ok(mut u) = url::Url::parse(raw) else {
        return String::new();
    };
    if !URL_SCHEMES.contains(&u.scheme()) {
        return String::new();
    }
    // A password may hold a literal `@`: the parser splits userinfo on the last `@`, so the served
    // canonical form would otherwise fold it in. Fail closed instead (#257, #277).
    if raw.bytes().filter(|&b| b == b'@').count() > 1 {
        return String::new();
    }
    // A literal `?`, `#`, `/` or `\` inside a password ends the authority, so the `@` that would
    // separate userinfo from host lands in the query/fragment/path instead and the parser reads
    // the prefix as a bare `host[:port]` (with any port, default, empty or none). An `@` outside
    // the userinfo is therefore never served; fail closed (#257, #277).
    if u.username().is_empty() && u.password().is_none() && raw.contains('@') {
        return String::new();
    }
    // Cannot fail: an `http`/`https`/`ws`/`wss` URL has a non-empty host.
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_fragment(None);
    if keep_placeholders {
        if let Some(q) = u.query().map(str::to_owned) {
            let kept = keep_placeholder_params(&q);
            if kept.is_empty() {
                u.set_query(None);
            } else {
                u.set_query(Some(&kept.join("&")));
            }
        }
    } else {
        u.set_query(None);
    }
    u.into()
}

/// The tail after a leading `%NAME%` placeholder, if `s` starts with one of
/// [`VRS_PLACEHOLDERS`]. The name matches case-insensitively, as the shipped page upper-cases it
/// before the lookup (`scoutbase.core.js`).
fn strip_placeholder(s: &str) -> Option<&str> {
    let inner = s.strip_prefix('%')?;
    let (name, tail) = inner.split_once('%')?;
    VRS_PLACEHOLDERS
        .iter()
        .any(|p| p.eq_ignore_ascii_case(name))
        .then_some(tail)
}

/// Whether `value` is a non-empty sequence of at least one [`VRS_PLACEHOLDERS`] token separated
/// only by `,`, `;` or whitespace — placeholders with no literal text that could carry a
/// credential. A value of separators only (or empty) holds no placeholder and returns `false`.
fn is_placeholder_only(value: &str) -> bool {
    let mut rest = value;
    let mut saw_placeholder = false;
    loop {
        rest = rest.trim_start_matches([',', ';', ' ', '\t']);
        if rest.is_empty() {
            return saw_placeholder;
        }
        let Some(tail) = strip_placeholder(rest) else {
            return false;
        };
        saw_placeholder = true;
        rest = tail;
    }
}

/// The `&`/`;`-separated parameters of `query` whose value is placeholder-only, in order; every
/// other parameter (a literal value, which may carry a credential) is dropped.
fn keep_placeholder_params(query: &str) -> Vec<&str> {
    query
        .split(['&', ';'])
        .filter(|seg| {
            let Some((_, value)) = seg.split_once('=') else {
                return false;
            };
            !value.is_empty() && is_placeholder_only(value)
        })
        .collect()
}

/// `/version.json` (`Webserver.cs:451-462`): the version as an indented JSON string.
fn version() -> String {
    to_indented(&env!("CARGO_PKG_VERSION"))
}

/// `/bands.json` (`Webserver.cs:477-483`): the band names, compact.
fn bands() -> String {
    let names: Vec<&str> = Band::all_except_none_and_all()
        .iter()
        .map(|b| b.name())
        .collect();
    to_compact(&names)
}

/// `/bandvalues.json` (`Webserver.cs:484-490`). `Name` is the band text, as
/// `Bands.GetBandsExceptNoneAndAll` sets it (`ScoutBase/ScoutBase.Core/Band.cs:151`).
fn band_values() -> String {
    let rows: Vec<NameValueJson> = Band::all_except_none_and_all()
        .iter()
        .map(|b| NameValueJson {
            name: b.name().to_string(),
            value: *b as i32,
        })
        .collect();
    to_compact(&rows)
}

/// `/aircraftcategories.json` (`Webserver.cs:492-498`).
fn aircraft_categories() -> String {
    let rows: Vec<NameValueJson> = PlaneCategory::ALL
        .iter()
        .map(|c| NameValueJson {
            name: c.as_str().to_string(),
            value: *c as i32,
        })
        .collect();
    to_compact(&rows)
}

/// The members of the VRS settings the page reads (`airscout.feeds.js:101`, `:254-255`,
/// `:320`), in the C# declaration order (`VRSWebServer.cs:41-92`). Username and password
/// are left out.
#[derive(Serialize)]
struct VrsWebSettings {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "URL2")]
    url2: String,
    #[serde(rename = "LoadShare")]
    load_share: bool,
    #[serde(rename = "Timeout")]
    timeout: i32,
}

/// An object without members.
#[derive(Serialize)]
struct Empty {}

#[derive(Serialize)]
#[serde(untagged)]
enum FeedValue {
    Vrs(VrsWebSettings),
    Empty(Empty),
}

#[derive(Serialize)]
struct FeedSettingsJson {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Value")]
    value: FeedValue,
}

/// `/planefeedsettings.json` (`Webserver.cs:1058-1104`) for the one active feed. VRS
/// gets the members the page reads; every other feed gets `{}`, since its settings hold
/// keys or credentials. An unknown feed gives `[]`.
fn plane_feed_settings(s: &Settings) -> String {
    let Some(mut feed) = as_feeds::registry::feed_by_name(&s.feed) else {
        return to_compact(&Vec::<FeedSettingsJson>::new());
    };
    let value = if feed.name() == VRS_FEED {
        let loaded = match s.feed_settings.get(feed.name()).and_then(|v| v.as_table()) {
            Some(t) => feed.load_settings(t).is_ok(),
            None => true,
        };
        if loaded {
            vrs_web_settings(&feed.settings())
        } else {
            FeedValue::Empty(Empty {})
        }
    } else {
        FeedValue::Empty(Empty {})
    };
    to_compact(&[FeedSettingsJson {
        name: feed_web_name(feed.name()),
        value,
    }])
}

fn vrs_web_settings(t: &toml::Table) -> FeedValue {
    let text = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let timeout = t.get("timeout_s").and_then(|v| v.as_integer()).unwrap_or(0);
    FeedValue::Vrs(VrsWebSettings {
        url: scrub_public_url(&text("url")),
        url2: scrub_public_url(&text("url2")),
        load_share: t
            .get("load_share")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        timeout: timeout.clamp(i32::MIN.into(), i32::MAX.into()) as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use as_aircraft::PlaneInfo;
    use as_feeds::feeds::airscoutserver;
    use chrono::TimeDelta;

    #[test]
    fn finding_370_forecasts_at_request_time_and_expires() {
        let start = DateTime::from_timestamp(1_795_521_600, 0).unwrap();
        let cache = PlaneInfoCache::new();
        cache.set(PlaneInfo::new(
            start,
            "TE1ST",
            "",
            "3C0001",
            0.0,
            0.0,
            90.0,
            35_000.0,
            360.0,
            "TE1",
            "",
            "",
            PlaneCategory::Medium,
        ));
        let now = start + TimeDelta::minutes(1);
        let response = forecast_json(&cache, now, 1);
        let planes = airscoutserver::parse(&response, now).unwrap();
        let [plane] = planes.as_slice() else {
            panic!("expected one forecast: {response}");
        };
        assert_eq!(plane.hex, "3C0001");
        assert_eq!(plane.time, Some(now));
        // 360 kt for one minute is 6 nautical miles, i.e. 11.112 km.
        // An equatorial arc subtends 11.112 / 6371 radians, or 0.0999° at wire precision.
        assert_eq!((plane.lat, plane.lon), (0.0, 0.0999));

        let expired = now + TimeDelta::seconds(1);
        let response = forecast_json(&cache, expired, 1);
        assert_eq!(response, "{\"full_count\":0,\"version\":1}\n");
        assert!(airscoutserver::parse(&response, expired)
            .unwrap()
            .is_empty());
    }
}
