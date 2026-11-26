//! App state machine (play/pause, ticks, service events) and the eframe app.
//!
//! [`AppState`] is GUI-free and talks to the background services through [`Services`], so tests
//! inject a recorder. `ui::*` only draws and calls `AppState` methods.

mod cat;
mod dialogs;
mod history;
mod integrations;
mod maintenance;
mod services_api;
mod services_trait;
mod shell;
mod tracking;
mod wintest;

pub use history::HistoryState;
pub use maintenance::{MaintState, MAINT_ID_BASE};
pub use services_api::{FailedServices, RealServices};
pub use services_trait::Services;
pub use shell::{AirScoutApp, RootApp};

use crate::background::{BackgroundService, Led, LedState, LEDS};
use crate::horizon_service::{HorizonHandle, HorizonRequest, HorizonService};
use crate::pipeline::{tick, TickInput, TickResult};
use crate::services::{
    open_data, FeedService, MyStation, PathRequest, PathService, PathTarget, Paths, ServiceEvent,
};
use crate::sound::{AlarmBeeps, Beeper, NullBeeper};
use crate::speclab::{SpecLabService, SpectrumState};
use crate::ui::dialogs::horizon::{HorizonDlg, HorizonState};
use crate::ui::dialogs::obstructions::ObstructionsDlg;
use crate::ui::dialogs::set_time::SetTimeDlg;
use crate::ui::dialogs::watchlist::WatchlistDlg;
use crate::ui::dialogs::wizard::WizardState;
use crate::ui::options::stations::{station_key, StationAction, Who};
use crate::ui::options::OptionsDlg;
use as_aircraft::enums::{LifeMode, PathMode};
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use as_elevation::{ElevationModel, LocalObstruction};
use as_feeds::worker::FeedEvent;
use as_geo::latlon::{GPoint, GRect};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_propagation::PropagationPath;
use as_settings::{Settings, WatchlistItem};
use as_signals::SignalDb;
use as_stations::Qrv;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Paused,
    Playing,
}

/// Tab of the right control panel (`tc_Control`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlTab {
    #[default]
    Single,
    Multi,
    Opt,
}

/// Tab of the bottom panel below the map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BottomTab {
    #[default]
    Profile,
    Planes,
    Spectrum,
    Analysis,
}

/// A locator (`JO50AA`) or `lat,lon` in degrees. `None` if neither is valid.
pub fn parse_position(text: &str) -> Option<GPoint> {
    let t = text.trim();
    if let Some((a, b)) = t.split_once(',') {
        let lat: f64 = a.trim().parse().ok()?;
        let lon: f64 = b.trim().parse().ok()?;
        return maidenhead::check_lat_lon(lat, lon).then(|| GPoint::new(lat, lon));
    }
    if !maidenhead::check(t) {
        return None;
    }
    let (lat, lon) = maidenhead::lat_lon_from_loc(t, PositionInRectangle::MiddleMiddle).ok()?;
    Some(GPoint::new(lat, lon))
}

/// Load the settings. A file that cannot be read keeps the defaults and returns the error text;
/// the app then never saves, so the broken file survives for the user to fix.
pub fn load_settings(path: &Path) -> (Settings, Option<String>) {
    match Settings::load_or_default(path) {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e.to_string())),
    }
}

/// Data directory: the OS data dir of `eu.airscout.AirScout-RS` (on macOS
/// `~/Library/Application Support/eu.airscout.AirScout-RS`), or `./data` if the OS has none.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("eu", "airscout", "AirScout-RS")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // absolute once, so the delete guards (which refuse relative dirs) accept it
            let rel = PathBuf::from("data");
            std::path::absolute(&rel).unwrap_or(rel)
        })
}

/// The watchlist item for the DX station of the settings: locator of 3 precision steps, flagged
/// out of range if the My-DX distance exceeds `Path_MaxLength`. `None` if the DX position has no
/// locator.
pub fn dx_watch_item(s: &Settings) -> Option<WatchlistItem> {
    let loc = maidenhead::loc_from_lat_lon(s.dx_lat, s.dx_lon, false, 3, false)
        .ok()
        .flatten()?;
    let d = as_geo::latlon::distance(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
    Some(WatchlistItem {
        call: s.dx_call.clone(),
        loc,
        out_of_range: d > s.path_max_length_km,
        ..WatchlistItem::default()
    })
}

pub struct AppState {
    pub settings: Settings,
    pub mode: RunMode,
    /// Computed paths, ordered by target index.
    pub paths: Vec<PathEntry>,
    /// Best plane potential per watchlist call of the last MULTI tick (C# writes it into the
    /// watchlist item tooltips, MULTI only).
    pub watch_potential: BTreeMap<String, i32>,
    /// The watchlist check marks changed while playing in MULTI; the next tick re-sends the
    /// paths (`WatchListUpdated`).
    watchlist_dirty: bool,
    /// Id of the request whose `PathsDone` arrived.
    paths_done_id: Option<u64>,
    /// Id of the newest path request; results of older ones are dropped.
    pub path_gen: u64,
    pub tick: TickResult,
    /// Spectrum Lab levels and status of the Spectrum tab.
    pub spectrum: SpectrumState,
    /// LIFE or HISTORY (`LifeMode`).
    pub life_mode: LifeMode,
    /// The HISTORY state; `Some` from ON until OFF.
    pub history: Option<HistoryState>,
    /// Database maintenance: dialogs, confirmation and the running job.
    pub maint: MaintState,
    /// Id of the newest analysis job; events of older ones are dropped.
    job_id: u64,
    pub status: String,
    /// Elevation note of the current path request (missing squares, download error); shown after
    /// the status, cleared on Play.
    pub path_note: Option<String>,
    /// Id of the newest feed start; events of older feeds are dropped.
    pub feed_gen: u64,
    /// Hex of the plane selected in the map or the lists.
    pub selected: Option<String>,
    /// `TrackMode`, the values of the last tracking step and the TRK status (`app/tracking.rs`).
    pub track_mode: as_aircraft::enums::TrackMode,
    pub track: Option<crate::tracking::TrackValues>,
    pub track_status: crate::tracking::TrackStatus,
    track_rt: tracking::TrackRt,
    /// ROT field of the status bar (`SayRot`), as the Track worker reports it.
    pub rot_status: crate::tracking::RotStatus,
    /// CAT field of the status bar (`SayCAT`) and the rig the worker identified (`app/cat.rs`).
    pub cat_status: as_cat::RigStatus,
    pub cat_ident: Option<as_cat::RigIdent>,
    /// Generation of the running integrations; their events carry it (`app/integrations.rs`).
    pub integration_gen: u64,
    /// The nearest settings the Win-Test receiver got last.
    pub nearest_synced: Option<crate::wintest_nearest::NearestSettings>,
    /// When the web server's settings were checked last and what it got (`integrations.rs`).
    pub web_synced: Option<(Instant, Settings)>,
    pub settings_error: Option<String>,
    pub control_tab: ControlTab,
    pub bottom_tab: BottomTab,
    /// Transient widget state of the Analysis tab.
    pub analysis_ui: crate::ui::analysis::AnalysisUi,
    pub my_loc_text: String,
    pub dx_loc_text: String,
    /// Text of the feed LED in the status bar.
    pub feed_status: String,
    /// State and tooltip of each status LED, in status bar order.
    pub leds: Vec<(Led, LedState, String)>,
    /// Background jobs that finished a run since the last (re)start of the workers.
    pub background_idle: BTreeSet<Led>,
    /// Generation of the current background workers; their events carry it.
    pub background_gen: u64,
    /// Progress of the background path calculators (`tsl_Calculations`).
    pub calculations: String,
    /// My as last given to the services.
    my_synced: Option<(String, f64, f64)>,
    pub cache: Arc<PlaneInfoCache>,
    /// Zoom steps requested by the buttons; the map takes them.
    pub pending_zoom: i32,
    /// The open `WatchlistDlg`, if any.
    pub watchlist_dlg: Option<WatchlistDlg>,
    /// The open Options dialog, if any.
    pub options_dlg: Option<OptionsDlg>,
    /// The open `LocalObstructionsDlg`, if any.
    pub obstructions_dlg: Option<ObstructionsDlg>,
    /// The open `HorizonDlg`, if any.
    pub horizon_dlg: Option<HorizonDlg>,
    /// The data directory; `None` exports nowhere (tests).
    pub data_dir: Option<PathBuf>,
    horizon_seq: u64,
    /// Tile cache directory of the station map in the Options dialog; `None` shows no tiles.
    pub tile_dir: Option<PathBuf>,
    /// The `as-maps` tile store (`<data>/maptiles.gmdb`) the map layers read first; `None`
    /// loads every tile over HTTP.
    pub map_store: Option<Arc<as_maps::TileStore>>,
    /// Where `apply_options` saves the settings; `None` saves nowhere (tests).
    pub settings_path: Option<PathBuf>,
    /// The first-run wizard, while it is open. Nothing runs and nothing is saved meanwhile.
    pub wizard: Option<WizardState>,
    /// Set by a tick when the alarm wants the window in front; the shell takes it.
    pub alarm_focus: bool,
    /// The open `SetTimeDlg`, if any.
    pub set_time_dlg: Option<SetTimeDlg>,
    beeper: Box<dyn Beeper>,
    alarm_beeps: AlarmBeeps,
    services: Box<dyn Services>,
    last_tick: Option<Instant>,
}

/// One computed path with the data the map and profile need.
#[derive(Debug, Clone)]
pub struct PathEntry {
    /// Index of the target in the path request.
    pub index: usize,
    pub call: String,
    pub path: PropagationPath,
    /// Ground elevation per info point of `path`.
    pub terrain: Vec<f64>,
    pub my_gain_dbi: f64,
    pub dx_gain_dbi: f64,
}

impl AppState {
    /// The path of SINGLE mode (the first entry), if computed.
    pub fn single_path(&self) -> Option<&PathEntry> {
        self.paths.first()
    }

    pub fn new(
        settings: Settings,
        settings_error: Option<String>,
        services: Box<dyn Services>,
        cache: Arc<PlaneInfoCache>,
    ) -> Self {
        let my_loc_text = settings.my_loc().unwrap_or_default();
        let dx_loc_text = settings.dx_loc().unwrap_or_default();
        Self {
            settings,
            mode: RunMode::Paused,
            paths: Vec::new(),
            watch_potential: BTreeMap::new(),
            watchlist_dirty: false,
            paths_done_id: None,
            path_gen: 0,
            tick: TickResult::default(),
            spectrum: SpectrumState::default(),
            life_mode: LifeMode::Life,
            history: None,
            maint: MaintState::default(),
            job_id: 0,
            status: String::new(),
            path_note: None,
            feed_gen: 0,
            selected: None,
            track_mode: Default::default(),
            track: None,
            track_status: Default::default(),
            track_rt: Default::default(),
            rot_status: Default::default(),
            cat_status: Default::default(),
            cat_ident: None,
            integration_gen: 0,
            nearest_synced: None,
            web_synced: None,
            settings_error,
            control_tab: ControlTab::default(),
            bottom_tab: BottomTab::default(),
            analysis_ui: Default::default(),
            my_loc_text,
            dx_loc_text,
            feed_status: "off".into(),
            leds: LEDS
                .iter()
                .map(|l| {
                    (
                        *l,
                        LedState::Unknown,
                        crate::background::led_text(LedState::Unknown).into(),
                    )
                })
                .collect(),
            background_idle: BTreeSet::new(),
            background_gen: 0,
            calculations: String::new(),
            my_synced: None,
            cache,
            pending_zoom: 0,
            watchlist_dlg: None,
            options_dlg: None,
            obstructions_dlg: None,
            horizon_dlg: None,
            data_dir: None,
            horizon_seq: 0,
            tile_dir: None,
            map_store: None,
            settings_path: None,
            wizard: None,
            alarm_focus: false,
            set_time_dlg: None,
            beeper: Box::new(NullBeeper),
            alarm_beeps: AlarmBeeps::default(),
            services,
            last_tick: None,
        }
    }

    /// Edit of the My locator field: a valid text moves the station.
    pub fn set_my_loc_text(&mut self, text: &str) {
        self.my_loc_text = text.to_string();
        if let Some(p) = parse_position(text) {
            self.settings.my_lat = p.lat;
            self.settings.my_lon = p.lon;
        }
        self.sync_my();
    }

    pub fn set_dx_loc_text(&mut self, text: &str) {
        self.dx_loc_text = text.to_string();
        if let Some(p) = parse_position(text) {
            self.settings.dx_lat = p.lat;
            self.settings.dx_lon = p.lon;
        }
    }

    /// Both fields hold a valid position.
    pub fn locators_valid(&self) -> bool {
        parse_position(&self.my_loc_text).is_some() && parse_position(&self.dx_loc_text).is_some()
    }

    /// The My-DX great-circle distance [km] if it exceeds `Path_MaxLength` (the C# program
    /// refuses such a path), else `None`. Also `None` while a locator is invalid.
    pub fn path_too_long(&self) -> Option<f64> {
        if !self.locators_valid() {
            return None;
        }
        let s = &self.settings;
        let d = as_geo::latlon::distance(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
        (d > s.path_max_length_km).then_some(d)
    }

    /// Status text with the elevation note of the current path request, if any.
    pub fn status_line(&self) -> String {
        match (&self.path_note, self.status.is_empty()) {
            (Some(n), false) => format!("{}; {n}", self.status),
            (Some(n), true) => n.clone(),
            (None, _) => self.status.clone(),
        }
    }

    /// Text of the alarm shown in the status bar while a tick has an alarm plane.
    pub fn alarm_text(&self) -> Option<String> {
        self.tick
            .alarm
            .as_ref()
            .map(|p| format!("ALARM: {} {:.0} km", p.call, p.int_qrb))
    }

    /// `(QTF, QRB)` rounded as in `UpdateStatus` (MapDlg.cs:3400): `(0, 0)` unless both locators
    /// are valid.
    pub fn qtf_qrb(&self) -> (i64, i64) {
        if !self.locators_valid() {
            return (0, 0);
        }
        let s = &self.settings;
        let qtf = as_geo::latlon::bearing(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
        let qrb = as_geo::latlon::distance(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
        (qtf.round_ties_even() as i64, qrb.round_ties_even() as i64)
    }

    /// All targets of the current path request have been answered.
    pub fn paths_done(&self) -> bool {
        self.paths_done_id == Some(self.path_gen)
    }

    /// MULTI if the Multi tab is selected, else SINGLE (the tab is fixed while playing).
    pub fn path_mode(&self) -> PathMode {
        if self.control_tab == ControlTab::Multi {
            PathMode::Multi
        } else {
            PathMode::Single
        }
    }

    /// Select a control tab; refused (returns false) while playing (`tc_Control_Selecting`,
    /// MapDlg.cs:9922).
    pub fn set_control_tab(&mut self, tab: ControlTab) -> bool {
        if self.is_playing() {
            return false;
        }
        self.control_tab = tab;
        true
    }

    /// MULTI targets (`UpdatePaths`): checked, in-range items with a valid call and locator. The
    /// position is the stored one of the station database (`LocationFindOrCreate` returns the
    /// stored entry), else the locator centre (what a newly created entry gets).
    fn multi_targets(&self) -> Vec<PathTarget> {
        self.settings
            .watchlist
            .0
            .iter()
            .filter(|i| i.checked && !i.out_of_range)
            .filter(|i| as_stations::callsign::check(&i.call) && maidenhead::check(&i.loc))
            .filter_map(|i| {
                let pos = self
                    .services
                    .station_position(&i.call, &i.loc)
                    .or_else(|| parse_position(&i.loc))?;
                Some(PathTarget {
                    call: i.call.clone(),
                    pos,
                })
            })
            .collect()
    }

    /// Send a path request for `targets` under a new id (older results are dropped).
    fn send_paths(&mut self, targets: Vec<PathTarget>) {
        self.paths.clear();
        self.path_gen += 1;
        self.services.request_path(PathRequest {
            id: self.path_gen,
            my: GPoint::new(self.settings.my_lat, self.settings.my_lon),
            my_call: self.settings.my_call.clone(),
            targets,
            settings: self.settings.clone(),
        });
    }

    /// `Play` (MapDlg.cs:3998): validate, request the path(s), start the feed.
    pub fn play(&mut self) -> Result<(), String> {
        if self.maint.running.is_some() {
            // the feed would write while a clear holds the database
            let e = "database maintenance is running";
            self.status = e.into();
            return Err(e.into());
        }
        let targets = self.path_targets()?;
        let mode = self.path_mode();
        self.feed_gen += 1;
        if let Err(e) = self
            .services
            .start_feed(self.feed_gen, &self.settings, self.cache.clone())
        {
            self.status = format!("feed: {e}");
            self.feed_status = "error".into();
            return Err(e);
        }
        self.feed_status = "starting".into();
        self.start_speclab();
        self.last_tick = None;
        self.track_reset();
        self.send_targets(targets, mode);
        self.mode = RunMode::Playing;
        self.status = "calculating path".into();
        Ok(())
    }

    /// Start the Spectrum Lab poller if it is enabled; the plot starts empty.
    fn start_speclab(&mut self) {
        self.spectrum.clear();
        self.spectrum.record.store(false, Ordering::Relaxed);
        if self.settings.speclab.enabled {
            self.services
                .start_speclab(&self.settings, self.spectrum.record.clone());
        } else {
            self.services.stop_speclab();
        }
    }

    /// Replace the alarm beeper (the default is silent).
    pub fn set_beeper(&mut self, beeper: Box<dyn Beeper>) {
        self.beeper = beeper;
    }

    /// The time the UTC field shows (`CurrentTime`).
    pub fn current_time(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        if self.settings.time.online {
            now
        } else {
            self.settings.time.offline
        }
    }

    /// `Pause` (MapDlg.cs:4063): stop the feed; the inputs are enabled again.
    pub fn pause(&mut self) {
        self.services.stop_feed();
        self.services.stop_speclab();
        self.spectrum.record.store(false, Ordering::Relaxed);
        self.alarm_beeps.clear();
        self.mode = RunMode::Paused;
        self.track_stop();
    }

    pub fn is_playing(&self) -> bool {
        self.mode == RunMode::Playing
    }

    /// Exit: stop the workers and save the settings, except while the wizard is open (an abort
    /// writes no settings file).
    pub fn on_exit(&mut self) {
        self.restore_time_mode(); // HISTORY must not leave `online = false` in the file
        self.pause();
        self.stop_background();
        // waits for the CAT worker to restore the rig, at most `EXIT_WAIT`
        self.services
            .stop_integrations_and_wait(crate::cat_service::EXIT_WAIT);
        self.reset_integration_state();
        if self.wizard.is_some() {
            return;
        }
        if let Some(path) = self.settings_path.clone() {
            if let Err(e) = self.save_settings(&path) {
                eprintln!("cannot save settings: {e}");
            }
        }
    }

    /// Start the background updaters; the one place that does it (the wizard can defer it).
    pub fn start_background(&mut self) {
        self.sync_services();
        let generation = self.reset_background_state();
        self.services.start_background(&self.settings, generation);
    }

    /// Give the services the settings they do not get in a request: the near-field suppression
    /// and My.
    fn sync_services(&mut self) {
        self.services
            .set_near_field_suppression(self.settings.path_near_field_suppression_m);
        self.my_synced = None;
        self.sync_my();
    }

    /// Tell the services My if it differs from what they got last. Cheap; also called every frame,
    /// because the call field is edited in place.
    pub fn sync_my(&mut self) {
        if self.wizard.is_some() {
            return; // the services have not started
        }
        let s = &self.settings;
        let now = (s.my_call.clone(), s.my_lat, s.my_lon);
        if self.my_synced.as_ref() != Some(&now) {
            self.services.set_my(&now.0, now.1, now.2);
            self.my_synced = Some(now);
        }
    }

    /// New generation, LEDs back to unknown, no job idle. Returns the generation.
    fn reset_background_state(&mut self) -> u64 {
        self.background_gen += 1;
        self.background_idle.clear();
        self.calculations.clear();
        for l in &mut self.leds {
            l.1 = LedState::Unknown;
            l.2 = crate::background::led_text(LedState::Unknown).into();
        }
        self.background_gen
    }

    /// Cancel the background updaters (exit).
    pub fn stop_background(&mut self) {
        self.services.stop_background();
    }

    pub fn on_event(&mut self, e: ServiceEvent) {
        match e {
            ServiceEvent::Led { generation, .. }
            | ServiceEvent::BackgroundIdle { generation, .. }
            | ServiceEvent::Calculations { generation, .. }
                if generation != self.background_gen => {}
            ServiceEvent::Led {
                led, state, text, ..
            } => {
                if let Some(slot) = self.leds.iter_mut().find(|l| l.0 == led) {
                    slot.1 = state;
                    slot.2 = text;
                }
            }
            ServiceEvent::BackgroundIdle { led, .. } => {
                self.background_idle.insert(led);
            }
            ServiceEvent::Calculations { text, .. } => self.calculations = text,
            ServiceEvent::Status(s) => self.status = s,
            ServiceEvent::Error(s) => self.status = format!("error: {s}"),
            ServiceEvent::PathNote { id, text } => {
                if id == self.path_gen {
                    self.path_note = Some(text);
                }
            }
            ServiceEvent::PathReady { id, .. }
            | ServiceEvent::PathFailed { id, .. }
            | ServiceEvent::PathsDone { id }
                if id != self.path_gen => {}
            ServiceEvent::PathReady {
                index,
                call,
                path,
                terrain,
                my_gain_dbi,
                dx_gain_dbi,
                ..
            } => {
                let entry = PathEntry {
                    index,
                    call,
                    path,
                    terrain,
                    my_gain_dbi,
                    dx_gain_dbi,
                };
                // keep the entries ordered by target index
                let at = self.paths.partition_point(|e| e.index < index);
                self.paths.insert(at, entry);
                self.status = "path ready".into();
            }
            ServiceEvent::PathFailed { error, .. } => self.status = error,
            ServiceEvent::PathsDone { id } => self.paths_done_id = Some(id),
            ServiceEvent::TrackHeights { id, my_m, dx_m } => self.on_track_heights(id, my_m, dx_m),
            ServiceEvent::Job { id, event } => self.on_job_event(id, event),
            ServiceEvent::Stations(list) => {
                if let Some(d) = &mut self.watchlist_dlg {
                    d.set_all(list);
                }
            }
            ServiceEvent::HorizonPoint { id, az, point } => {
                if let Some(d) = &mut self.horizon_dlg {
                    d.on_point(id, az, point);
                }
            }
            ServiceEvent::HorizonDone { id, horizon } => {
                if let Some(d) = &mut self.horizon_dlg {
                    d.on_done(id, *horizon);
                }
            }
            ServiceEvent::HorizonFailed { id, error } => {
                if let Some(d) = &mut self.horizon_dlg {
                    d.on_failed(id, error);
                }
            }
            // a poller that was just stopped can still have an event in the queue
            ServiceEvent::Spectrum { .. } | ServiceEvent::SpectrumError(_)
                if !self.is_playing() => {}
            ServiceEvent::Spectrum { at, level } => self.spectrum.on_level(at, level),
            ServiceEvent::SpectrumError(e) => self.spectrum.on_error(e),
            ServiceEvent::Track { generation, event } => self.on_track_event(generation, event),
            ServiceEvent::WinTest { generation, cmd } => {
                if generation == self.integration_gen {
                    self.on_wintest(cmd);
                }
            }
            ServiceEvent::Cat { generation, event } => self.on_cat_event(generation, event),
            ServiceEvent::Feed(id, _) if id != self.feed_gen => {}
            ServiceEvent::Feed(_, f) => match f {
                FeedEvent::Started => self.feed_status = "running".into(),
                FeedEvent::Planes(p) => self.feed_status = format!("running, {} planes", p.len()),
                FeedEvent::Error(s) => {
                    self.feed_status = "error".into();
                    self.status = format!("feed: {s}");
                }
                FeedEvent::Stopped => self.feed_status = "off".into(),
            },
        }
    }

    /// True once per `map_update_s` while playing (`ti_Progress`); the first call after Play is
    /// due at once.
    pub fn tick_due(&mut self, at: Instant) -> bool {
        if !self.ticking() {
            return false;
        }
        let every = Duration::from_secs(self.settings.gui.map_update_s.max(1));
        let due = self.last_tick.is_none_or(|t| at.duration_since(t) >= every);
        if due {
            self.last_tick = Some(at);
        }
        due
    }

    /// `ti_Progress_Tick`, LIFE branch (MapDlg.cs:5353-5372): only while playing, and planes
    /// update only with the online time (`Time_Mode_Online`); offline LIFE shows no updates.
    pub fn on_tick(&mut self, now: DateTime<Utc>, cache: &PlaneInfoCache) {
        if self.life_mode == LifeMode::History {
            self.history_tick(now, true);
            return;
        }
        if !self.is_playing() || !self.settings.time.online {
            return;
        }
        self.resend_watch_paths();
        let planes = cache.get_all(now, self.settings.planes_position_ttl_min);
        self.run_tick(planes, now, now, true);
        self.track_unselected();
    }

    /// `UpdatePlanes` on `planes` read at `now`; the nearest-plane maths run at `nearest_at`.
    /// `alarm` is false in HISTORY (no sound, no window to the front).
    fn run_tick(
        &mut self,
        planes: Vec<PlaneInfo>,
        now: DateTime<Utc>,
        nearest_at: DateTime<Utc>,
        alarm: bool,
    ) {
        let paths: Vec<PropagationPath> = self.paths.iter().map(|e| e.path.clone()).collect();
        self.tick = tick(TickInput {
            now,
            nearest_at,
            paths: &paths,
            planes,
            settings: &self.settings,
            mode: self.path_mode(),
        });
        self.spectrum
            .record
            .store(self.tick.nearest.is_some(), Ordering::Relaxed);
        if self.path_mode() == PathMode::Multi {
            self.watch_potential = self
                .paths
                .iter()
                .zip(&self.tick.per_path_potential)
                .map(|(e, p)| (e.call.clone(), *p))
                .collect();
        }
        if !alarm {
            return;
        }
        self.alarm_beeps.update(
            &self.tick.alarm_hexes,
            self.settings.gui.alarm_play_sound,
            self.beeper.as_mut(),
        );
        if self.tick.alarm.is_some() && self.settings.gui.alarm_bring_to_front {
            self.alarm_focus = true;
        }
    }

    /// Save the settings, unless loading them failed (the broken file is kept).
    pub fn save_settings(&self, path: &Path) -> Result<(), String> {
        if self.settings_error.is_some() {
            return Ok(());
        }
        self.settings.save(path).map_err(|e| e.to_string())
    }
}
