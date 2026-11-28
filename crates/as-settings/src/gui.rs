//! GUI settings (stage 3a). Defaults come from the original `AirScout/Properties/Settings.settings`.

use crate::Secret;
use as_aircraft::PlaneCategory;
use chrono::{DateTime, TimeZone, Utc};
use std::path::PathBuf;

/// Map area of interest (`MinLat`..`MaxLon`) [deg].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Area {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

impl Default for Area {
    fn default() -> Self {
        Self {
            min_lat: 35.0,
            max_lat: 60.0,
            min_lon: -15.0,
            max_lon: 30.0,
        }
    }
}

/// Which rows the plane info window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct InfoWin {
    pub metric: bool,
    pub imperial: bool,
    pub position: bool,
    pub time: bool,
    pub epsilon: bool,
    pub squint: bool,
    pub track: bool,
    pub type_: bool,
    pub alt: bool,
    pub dist: bool,
    pub speed: bool,
    pub angle: bool,
    pub my_azimuth: bool,
    pub my_elevation: bool,
    pub my_doppler: bool,
    pub dx_azimuth: bool,
    pub dx_elevation: bool,
    pub dx_doppler: bool,
}

impl Default for InfoWin {
    fn default() -> Self {
        Self {
            metric: true,
            imperial: false,
            position: true,
            time: true,
            epsilon: false,
            squint: true,
            track: true,
            type_: true,
            alt: true,
            dist: true,
            speed: false,
            angle: false,
            my_azimuth: false,
            my_elevation: false,
            my_doppler: false,
            dx_azimuth: false,
            dx_elevation: false,
            dx_doppler: false,
        }
    }
}

/// `Locator_MaxLength` (`Settings.settings:369`): the default for the Options field.
pub const LOCATOR_MAX_LENGTH: i32 = 10;

/// `Locator_AutoLength` (`Settings.settings:372`): the default for the Options field.
pub const LOCATOR_AUTO_LENGTH: bool = true;

/// Settings of the GUI application.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    pub map_provider: String,
    /// Map refresh interval [s].
    pub map_update_s: u64,
    pub map_auto_center: bool,
    pub map_opacity: f32,
    pub map_label_calls: bool,
    pub map_show_beam_width: bool,
    /// `Map_ShowDistances` (`Settings.settings:724`): distance circles around My.
    pub map_show_distances: bool,
    /// `Map_ShowLocators` (`Settings.settings:721`): the Maidenhead locator grid.
    pub map_show_locators: bool,
    /// `Watchlist_Activated` (`Settings.settings:386`): the watchlist locations on the map.
    pub map_show_watchlist: bool,
    /// `Airports_Activate` (`Settings.settings:332`): the airports layer.
    pub map_show_airports: bool,
    pub locator_small_letters_for_subsquares: bool,
    pub locator_max_length: i32,
    pub locator_auto_length: bool,
    pub qrz_url_database: String,
    /// Plane filter: minimum altitude [m].
    pub planes_filter_min_alt_m: i32,
    /// Plane filter: maximum circumcircle [km] (0 = off).
    pub planes_filter_max_circumcircle_km: f64,
    pub planes_filter_min_category: PlaneCategory,
    /// Plane icon size [px] for Light, Medium, Heavy, Superheavy.
    pub planes_icon_size: [u32; 4],
    pub alarm_activate: bool,
    pub alarm_distance_km: f64,
    pub alarm_bring_to_front: bool,
    pub alarm_play_sound: bool,
    pub info_win: InfoWin,
    /// Recently used calls, most recent first (max 10).
    pub my_calls: Vec<String>,
    pub window_size: Option<[f32; 2]>,
    pub window_pos: Option<[f32; 2]>,
    pub side_panel_width: Option<f32>,
    pub bottom_panel_height: Option<f32>,
    pub show_info_box: bool,
    pub show_zoom_box: bool,
    pub show_filter_box: bool,
    pub show_alarm_box: bool,
    pub elevation_base_url: String,
    pub elevation_download: bool,
    /// Thunderforest map API key; only from user settings.
    pub thunderforest_api_key: Secret,
    pub horizon_plot_polar: bool,
    /// `Map_TrackingGaugesShow`.
    pub tracking_gauges_show: bool,
    /// `Map_TrackingGaugeWidth` (`Settings.settings:727`): width of each tracking gauge [px].
    /// The Options spinner offers 175..=1000; the painter clamps what it reads.
    pub gauge_width: u32,
    /// `Map_TrackingGaugeColor` (`Settings.settings:730`): a `KnownColor` name.
    pub gauge_forecolor: String,
    /// `StationsMap_OverlayElevation`.
    pub stations_map_overlay_elevation: bool,
    /// `Map_Preloader_Enabled` (`Settings.settings:598`): preload the area's map tiles from the
    /// airscout.eu mirror into the tile store. Off by default (owner decision Q3).
    pub map_preloader_enabled: bool,
    /// `Map_Preloader_MaxZoom` (`Settings.settings:604`): the preloader fetches zoom 5 down to
    /// this level. Clamped to `0..=`[`MAP_PRELOADER_MAX_ZOOM`] when read.
    #[serde(deserialize_with = "clamped_preloader_zoom")]
    pub map_preloader_max_zoom: u8,
    /// `Map_ShowUserSuppliedOverlay` (`Settings.settings:805`): draw the user-supplied image.
    pub overlay_enabled: bool,
    /// `Map_UserSuppliedOverlay_Filename` (`Settings.settings:802`): the image file; `None` is
    /// the empty C# default.
    pub overlay_filename: Option<PathBuf>,
    /// `Map_UserSuppliedOverlay_Opacity` (`Settings.settings:793`): 0 (clear) ..= 1 (opaque).
    pub overlay_opacity: f32,
    /// `Map_UserSuppliedOverlay_MinLat` (`Settings.settings:781`): south edge [deg].
    pub overlay_min_lat: f64,
    /// `Map_UserSuppliedOverlay_MaxLat` (`Settings.settings:784`): north edge [deg].
    pub overlay_max_lat: f64,
    /// `Map_UserSuppliedOverlay_MinLon` (`Settings.settings:787`): west edge [deg].
    pub overlay_min_lon: f64,
    /// `Map_UserSuppliedOverlay_MaxLon` (`Settings.settings:790`): east edge [deg].
    pub overlay_max_lon: f64,
    pub news_url: String,
    pub news_interval_s: u64,
    pub news_last_update: DateTime<Utc>,
    pub news_feed_enabled: bool,
}

impl std::fmt::Debug for GuiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuiSettings")
            .field("map_provider", &self.map_provider)
            .field("map_update_s", &self.map_update_s)
            .field("map_auto_center", &self.map_auto_center)
            .field("map_opacity", &self.map_opacity)
            .field("map_label_calls", &self.map_label_calls)
            .field("map_show_beam_width", &self.map_show_beam_width)
            .field("map_show_distances", &self.map_show_distances)
            .field("map_show_locators", &self.map_show_locators)
            .field("map_show_watchlist", &self.map_show_watchlist)
            .field("map_show_airports", &self.map_show_airports)
            .field(
                "locator_small_letters_for_subsquares",
                &self.locator_small_letters_for_subsquares,
            )
            .field("locator_max_length", &self.locator_max_length)
            .field("locator_auto_length", &self.locator_auto_length)
            .field("planes_filter_min_alt_m", &self.planes_filter_min_alt_m)
            .field(
                "planes_filter_max_circumcircle_km",
                &self.planes_filter_max_circumcircle_km,
            )
            .field(
                "planes_filter_min_category",
                &self.planes_filter_min_category,
            )
            .field("planes_icon_size", &self.planes_icon_size)
            .field("alarm_activate", &self.alarm_activate)
            .field("alarm_distance_km", &self.alarm_distance_km)
            .field("alarm_bring_to_front", &self.alarm_bring_to_front)
            .field("alarm_play_sound", &self.alarm_play_sound)
            .field("info_win", &self.info_win)
            .field("my_calls", &self.my_calls)
            .field("window_size", &self.window_size)
            .field("window_pos", &self.window_pos)
            .field("side_panel_width", &self.side_panel_width)
            .field("bottom_panel_height", &self.bottom_panel_height)
            .field("show_info_box", &self.show_info_box)
            .field("show_zoom_box", &self.show_zoom_box)
            .field("show_filter_box", &self.show_filter_box)
            .field("show_alarm_box", &self.show_alarm_box)
            .field("elevation_download", &self.elevation_download)
            .field("thunderforest_api_key", &self.thunderforest_api_key)
            .field("horizon_plot_polar", &self.horizon_plot_polar)
            .field("tracking_gauges_show", &self.tracking_gauges_show)
            .field("gauge_width", &self.gauge_width)
            .field("gauge_forecolor", &self.gauge_forecolor)
            .field(
                "stations_map_overlay_elevation",
                &self.stations_map_overlay_elevation,
            )
            .field("map_preloader_enabled", &self.map_preloader_enabled)
            .field("map_preloader_max_zoom", &self.map_preloader_max_zoom)
            // `overlay_filename` is a local path (it may name the user); left out.
            .field("overlay_enabled", &self.overlay_enabled)
            .field("overlay_opacity", &self.overlay_opacity)
            .field("overlay_min_lat", &self.overlay_min_lat)
            .field("overlay_max_lat", &self.overlay_max_lat)
            .field("overlay_min_lon", &self.overlay_min_lon)
            .field("overlay_max_lon", &self.overlay_max_lon)
            .field("news_interval_s", &self.news_interval_s)
            .field("news_last_update", &self.news_last_update)
            .field("news_feed_enabled", &self.news_feed_enabled)
            .finish_non_exhaustive()
    }
}

/// Highest preloader zoom level: the C# preloader hard-codes 11 (`MapPreloader.cs:146`) and the
/// Options spinner ends there.
pub const MAP_PRELOADER_MAX_ZOOM: u8 = 11;

/// Any integer, clamped to `0..=MAP_PRELOADER_MAX_ZOOM`, so a hand-edited value out of range
/// does not make the whole settings file unreadable.
fn clamped_preloader_zoom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let v: i64 = serde::Deserialize::deserialize(d)?;
    // In 0..=11 after the clamp, so the cast is exact.
    Ok(v.clamp(0, i64::from(MAP_PRELOADER_MAX_ZOOM)) as u8)
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            map_provider: "OpenStreetMap".into(),
            map_update_s: 1,
            map_auto_center: true,
            map_opacity: 1.0,
            map_label_calls: true,
            map_show_beam_width: false,
            map_show_distances: false,
            map_show_locators: false,
            map_show_watchlist: true,
            map_show_airports: true,
            locator_small_letters_for_subsquares: false,
            locator_max_length: LOCATOR_MAX_LENGTH,
            locator_auto_length: LOCATOR_AUTO_LENGTH,
            qrz_url_database: "http://www.qrz.com/db/".into(),
            planes_filter_min_alt_m: 0,
            planes_filter_max_circumcircle_km: 0.0,
            planes_filter_min_category: PlaneCategory::None,
            planes_icon_size: [16, 24, 36, 48],
            alarm_activate: false,
            alarm_distance_km: 100.0,
            alarm_bring_to_front: true,
            alarm_play_sound: true,
            info_win: InfoWin::default(),
            my_calls: Vec::new(),
            window_size: None,
            window_pos: None,
            side_panel_width: None,
            bottom_panel_height: None,
            show_info_box: true,
            show_zoom_box: true,
            show_filter_box: true,
            show_alarm_box: true,
            elevation_base_url: as_elevation::DEFAULT_BASE_URL.into(),
            elevation_download: true,
            thunderforest_api_key: Secret::default(),
            horizon_plot_polar: true,
            tracking_gauges_show: true,
            gauge_width: 175,
            gauge_forecolor: "Black".into(),
            stations_map_overlay_elevation: false,
            map_preloader_enabled: false,
            map_preloader_max_zoom: MAP_PRELOADER_MAX_ZOOM,
            overlay_enabled: false,
            overlay_filename: None,
            overlay_opacity: 0.5,
            overlay_min_lat: 49.0,
            overlay_max_lat: 59.0,
            overlay_min_lon: -14.5,
            overlay_max_lon: 6.0,
            news_url: "http://airscout.eu/index.php/news".into(),
            news_interval_s: 3600,
            news_last_update: Utc.with_ymd_and_hms(2015, 1, 31, 15, 21, 0).unwrap(),
            news_feed_enabled: false,
        }
    }
}
