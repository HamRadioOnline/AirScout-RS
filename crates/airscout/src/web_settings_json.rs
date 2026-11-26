//! The curated `/settings.json` (`AirScout/Webserver.cs:464-475`, D4).
//!
//! C# saves the settings and serialises the whole `Properties.Settings.Default`. This
//! port writes only the members the shipped page reads (the scripts of
//! `wwwroot/resources/js` and the inline script of `wwwroot/dist/index.html`), with the
//! C# names and types, in the `Settings.Designer.cs` declaration order. Nothing is saved.
//! No credential is written: `KST_Websocket` loses its userinfo and its query string, and
//! `Map_Providers` leaves out the Thunderforest entry, whose `APIKey` is a key
//! (`MapProvider.cs:30-36`).

use crate::web_api::{feed_web_name, scrub_public_url_without_query};
use as_settings::{Settings, LOCATOR_AUTO_LENGTH, LOCATOR_MAX_LENGTH};
use as_web::dto::BandSettingJson;
use as_web::json::{to_indented, Num};
use serde::Serialize;

/// A `MapProvider` (`AirScout/MapProvider.cs:44-54`). `APIKey` is always `null`.
#[derive(Serialize)]
struct MapProviderJson {
    #[serde(rename = "Name")]
    name: &'static str,
    #[serde(rename = "URL")]
    url: &'static str,
    #[serde(rename = "Attribution")]
    attribution: &'static str,
    #[serde(rename = "MinZoom")]
    min_zoom: i32,
    #[serde(rename = "MaxZoom")]
    max_zoom: i32,
    /// The unit type writes `null` and cannot carry a key.
    #[serde(rename = "APIKey")]
    api_key: (),
}

/// The OpenStreetMap and OpenTopoMap entries of `MapProviders.GetAll`
/// (`AirScout/MapProvider.cs:16-28`), texts verbatim.
const MAP_PROVIDERS: [MapProviderJson; 2] = [
    MapProviderJson {
        name: "OpenStreetMap",
        url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        attribution: "Data © <a href=\"https://www.openstreetmap.org/\">OpenStreetMap</a> \
                      contributors",
        min_zoom: 1,
        max_zoom: 19,
        api_key: (),
    },
    MapProviderJson {
        name: "OpenTopoMap",
        url: "https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png",
        attribution: "Map ©  <a href=\"https://www.opentopomap.org/\">OpenStreetMap</a>, Data © \
                      <a href=\"https://www.openstreetmap.org/\">OpenStreetMap</a> contributors",
        min_zoom: 1,
        max_zoom: 19,
        api_key: (),
    },
];

/// A `WatchlistItem` (`AirScout/Watchlist.cs:13-18`).
#[derive(Serialize)]
struct WatchlistItemJson<'a> {
    #[serde(rename = "Call")]
    call: &'a str,
    #[serde(rename = "Loc")]
    loc: &'a str,
    #[serde(rename = "Checked")]
    checked: bool,
    #[serde(rename = "Selected")]
    selected: bool,
    #[serde(rename = "OutOfRange")]
    out_of_range: bool,
    #[serde(rename = "Remove")]
    remove: bool,
}

/// The served members, in `AirScout/Properties/Settings.Designer.cs` order (the line of
/// each property in the comment). `double` is [`Num`], `int` and enums are `i32`.
#[derive(Serialize)]
struct SettingsJson<'a> {
    /// :29
    #[serde(rename = "MyCall")]
    my_call: &'a str,
    /// :41
    #[serde(rename = "DXCall")]
    dx_call: &'a str,
    /// :53, `BAND`
    #[serde(rename = "Band")]
    band: i32,
    /// :77
    #[serde(rename = "Planes_MinAlt")]
    planes_min_alt: i32,
    /// :89
    #[serde(rename = "Map_AutoCenter")]
    map_auto_center: bool,
    /// :125
    #[serde(rename = "Planes_MaxAlt")]
    planes_max_alt: i32,
    /// :200, `PLANECATEGORY`
    #[serde(rename = "Planes_Filter_Min_Category")]
    planes_filter_min_category: i32,
    /// :212
    #[serde(rename = "MinLon")]
    min_lon: Num,
    /// :224
    #[serde(rename = "MaxLon")]
    max_lon: Num,
    /// :236
    #[serde(rename = "MinLat")]
    min_lat: Num,
    /// :248
    #[serde(rename = "MaxLat")]
    max_lat: Num,
    /// :260
    #[serde(rename = "MyLat")]
    my_lat: Num,
    /// :272
    #[serde(rename = "MyLon")]
    my_lon: Num,
    /// :284
    #[serde(rename = "DXLat")]
    dx_lat: Num,
    /// :296
    #[serde(rename = "DXLon")]
    dx_lon: Num,
    /// :500
    #[serde(rename = "Path_BestCaseElevation")]
    path_best_case_elevation: bool,
    /// :536
    #[serde(rename = "Planes_PlaneFeed2")]
    planes_plane_feed2: String,
    /// :548
    #[serde(rename = "Planes_PlaneFeed3")]
    planes_plane_feed3: String,
    /// :560
    #[serde(rename = "Planes_PlaneFeed1")]
    planes_plane_feed1: String,
    /// :572
    #[serde(rename = "Planes_Position_TTL")]
    planes_position_ttl: i32,
    /// :596
    #[serde(rename = "Server_Name")]
    server_name: &'a str,
    /// :608
    #[serde(rename = "Map_Provider")]
    map_provider: &'a str,
    /// :716
    #[serde(rename = "InfoWin_Metric")]
    info_win_metric: bool,
    /// :740
    #[serde(rename = "InfoWin_Position")]
    info_win_position: bool,
    /// :752
    #[serde(rename = "InfoWin_Time")]
    info_win_time: bool,
    /// :764
    #[serde(rename = "InfoWin_Epsilon")]
    info_win_epsilon: bool,
    /// :776
    #[serde(rename = "InfoWin_Squint")]
    info_win_squint: bool,
    /// :788
    #[serde(rename = "InfoWin_Track")]
    info_win_track: bool,
    /// :800
    #[serde(rename = "InfoWin_Type")]
    info_win_type: bool,
    /// :812
    #[serde(rename = "InfoWin_Alt")]
    info_win_alt: bool,
    /// :824
    #[serde(rename = "InfoWin_Dist")]
    info_win_dist: bool,
    /// :836
    #[serde(rename = "InfoWin_Speed")]
    info_win_speed: bool,
    /// :848
    #[serde(rename = "InfoWin_Angle")]
    info_win_angle: bool,
    /// :1304, the km value truncated to `int`
    #[serde(rename = "Planes_Filter_Max_Circumcircle")]
    planes_filter_max_circumcircle: i32,
    /// :1496, `decimal`; written like a double (`10.0`)
    #[serde(rename = "Locator_MaxLength")]
    locator_max_length: Num,
    /// :1508
    #[serde(rename = "Locator_AutoLength")]
    locator_auto_length: bool,
    /// :1520
    #[serde(rename = "Locator_SmallLettersForSubsquares")]
    locator_small_letters_for_subsquares: bool,
    /// :1531
    #[serde(rename = "Watchlist")]
    watchlist: Vec<WatchlistItemJson<'a>>,
    /// :1555
    #[serde(rename = "Watchlist_SyncWithKST")]
    watchlist_sync_with_kst: bool,
    /// :1835, one row per band of the table
    #[serde(rename = "Path_Band_Settings")]
    path_band_settings: Vec<BandSettingJson>,
    /// :1858
    #[serde(rename = "MyCalls")]
    my_calls: &'a [String],
    /// :1870
    #[serde(rename = "Map_SmallMarkers")]
    map_small_markers: bool,
    /// :2884
    #[serde(rename = "Map_Providers")]
    map_providers: &'static [MapProviderJson],
    /// :2896
    #[serde(rename = "KST_Websocket")]
    kst_websocket: String,
    /// :2908, `decimal`; written like a double (`150.0`)
    #[serde(rename = "KST_WebSocket_Timeout")]
    kst_websocket_timeout: Num,
}

/// `Map_Provider` as served: the selected name when it is one of [`MAP_PROVIDERS`], else
/// `OpenStreetMap`, so the page always finds its base layer (`airscout.ui.js:112`).
fn served_map_provider(selected: &str) -> &str {
    if MAP_PROVIDERS.iter().any(|p| p.name == selected) {
        selected
    } else {
        MAP_PROVIDERS[0].name
    }
}

/// The `/settings.json` body, indented.
pub fn settings_json(s: &Settings) -> String {
    let iw = &s.gui.info_win;
    let j = SettingsJson {
        my_call: &s.my_call,
        dx_call: &s.dx_call,
        band: s.band as i32,
        planes_min_alt: s.planes_min_alt_m,
        map_auto_center: s.gui.map_auto_center,
        planes_max_alt: s.planes_max_alt_m,
        planes_filter_min_category: s.gui.planes_filter_min_category as i32,
        min_lon: Num(s.area.min_lon),
        max_lon: Num(s.area.max_lon),
        min_lat: Num(s.area.min_lat),
        max_lat: Num(s.area.max_lat),
        my_lat: Num(s.my_lat),
        my_lon: Num(s.my_lon),
        dx_lat: Num(s.dx_lat),
        dx_lon: Num(s.dx_lon),
        path_best_case_elevation: s.path_best_case_elevation,
        planes_plane_feed2: feed_web_name(&s.feed2),
        planes_plane_feed3: feed_web_name(&s.feed3),
        planes_plane_feed1: feed_web_name(&s.feed),
        planes_position_ttl: s
            .planes_position_ttl_min
            .clamp(i32::MIN.into(), i32::MAX.into()) as i32,
        server_name: &s.server.name,
        map_provider: served_map_provider(&s.gui.map_provider),
        info_win_metric: iw.metric,
        info_win_position: iw.position,
        info_win_time: iw.time,
        info_win_epsilon: iw.epsilon,
        info_win_squint: iw.squint,
        info_win_track: iw.track,
        info_win_type: iw.type_,
        info_win_alt: iw.alt,
        info_win_dist: iw.dist,
        info_win_speed: iw.speed,
        info_win_angle: iw.angle,
        // `as` truncates toward zero and saturates (NaN gives 0)
        planes_filter_max_circumcircle: s.gui.planes_filter_max_circumcircle_km as i32,
        locator_max_length: Num(f64::from(LOCATOR_MAX_LENGTH)),
        locator_auto_length: LOCATOR_AUTO_LENGTH,
        locator_small_letters_for_subsquares: s.gui.locator_small_letters_for_subsquares,
        watchlist: s
            .watchlist
            .0
            .iter()
            .map(|w| WatchlistItemJson {
                call: &w.call,
                loc: &w.loc,
                checked: w.checked,
                selected: w.selected,
                out_of_range: w.out_of_range,
                remove: w.remove,
            })
            .collect(),
        watchlist_sync_with_kst: s.watchlist_sync_with_kst,
        path_band_settings: s
            .band_settings
            .iter()
            .map(|(b, bs)| BandSettingJson {
                band: b.name().to_string(),
                k_factor: Num(bs.k_factor),
                f1_clearance: Num(bs.f1_clearance),
                ground_clearance: Num(bs.ground_clearance),
                max_distance: Num(bs.max_distance),
                max_squint: Num(bs.max_squint),
                max_elevation: Num(bs.max_elevation),
            })
            .collect(),
        my_calls: &s.gui.my_calls,
        map_small_markers: s.web.map_small_markers,
        map_providers: &MAP_PROVIDERS,
        kst_websocket: scrub_public_url_without_query(&s.web.kst_websocket),
        kst_websocket_timeout: Num(s.web.kst_websocket_timeout_s),
    };
    to_indented(&j)
}
