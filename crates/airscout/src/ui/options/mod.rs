//! Options dialog (`OptionsDlg` of the original): edits a draft of the settings in tabs; OK
//! hands the validated draft back, Cancel drops it.

mod alarm;
mod cat;
mod coverage;
mod database;
mod elevation;
mod fields;
mod general;
mod info;
mod map;
pub mod network;
mod path;
pub(crate) mod planes;
mod speclab;
pub mod stations;
mod track;
mod watchlist;

use crate::coverage::GridRequest;
use crate::maintenance::DbId;
use crate::ui::dialogs::disclaimer::{self, DisclaimerDlg};
use crate::ui::dialogs::feed_settings::{FeedSettingsDlg, FeedSettingsResult};
use crate::ui::dialogs::map_station::{MapStationDlg, MapStationResult};
use crate::web_service::WebState;
use as_elevation::ElevationModel;
use as_geo::latlon::GRect;
use as_maps::TileStore;
use as_settings::Settings;
use as_stations::Qrv;
pub use cat::omnirig_rig_text;
pub use database::MaintAction;
use eframe::egui::{self, ScrollArea};
use elevation::ElevationAction;
pub(crate) use fields::{num, num_step, text};
pub use fields::{validate, validate_area};
use stations::{StationAction, StationsState, Who};
use std::path::PathBuf;
use std::sync::Arc;

/// What the user did with the dialog. Short-lived, so the size gap of the variants is harmless.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum OptionsOutcome {
    Ok(Settings),
    Cancel,
}

/// What the coverage view and the station map ask the app to run or stop
/// ([`OptionsDlg::take_view_requests`]).
#[derive(Debug, Clone, PartialEq)]
pub enum ViewRequest {
    /// Show the coverage of `model`: download its catalogue from `base_url`, restricted to `area`.
    Coverage {
        model: ElevationModel,
        base_url: String,
        area: GRect,
    },
    CancelCoverage,
    Grid(GridRequest),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionsTab {
    #[default]
    General,
    Database,
    Stations,
    Map,
    Elevation,
    Path,
    Planes,
    Alarm,
    Network,
    Track,
    Cat,
    SpecLab,
    Watchlist,
    Info,
}

impl OptionsTab {
    const ALL: [(OptionsTab, &'static str); 14] = [
        (OptionsTab::General, "General"),
        (OptionsTab::Database, "Database"),
        (OptionsTab::Stations, "Stations"),
        (OptionsTab::Map, "Map"),
        (OptionsTab::Elevation, "Elevation"),
        (OptionsTab::Path, "Path"),
        (OptionsTab::Planes, "Planes"),
        (OptionsTab::Alarm, "Alarm"),
        (OptionsTab::Network, "Network"),
        (OptionsTab::Track, "Track"),
        (OptionsTab::Cat, "CAT"),
        (OptionsTab::SpecLab, "SpecLab"),
        (OptionsTab::Watchlist, "Watchlist"),
        (OptionsTab::Info, "Info"),
    ];
}

pub struct OptionsDlg {
    /// The copy of the settings the tabs edit.
    pub draft: Settings,
    pub tab: OptionsTab,
    /// Why the last OK was refused.
    pub errors: Vec<String>,
    key: map::KeyEdit,
    /// State of the Stations tab, including the QRV edits (written only on OK).
    pub stations: StationsState,
    /// The station map, while it is open.
    map_dlg: Option<(Who, MapStationDlg)>,
    /// Tile cache of the station map; `None` shows no tiles.
    tile_dir: Option<PathBuf>,
    /// Tile store the station map and the coverage view read first.
    map_store: Option<Arc<TileStore>>,
    /// The horizon or obstruction dialog the user asked for; the app opens it
    /// ([`Self::take_request`]).
    request: Option<StationAction>,
    /// The disclaimer of the feed the user just picked; the draft keeps the old feed until
    /// it is accepted.
    pub disclaimer_dlg: Option<DisclaimerDlg>,
    /// The settings editor of the selected feed.
    pub feed_dlg: Option<FeedSettingsDlg>,
    /// Database files and sizes of the Database tab; the app keeps them current.
    pub sizes: Vec<(DbId, String, f64)>,
    /// False while playing or while a job runs: the maintenance buttons are disabled.
    pub maint_enabled: bool,
    /// Bind state of the web server; the app keeps it current.
    pub web_state: WebState,
    /// The maintenance button the user pressed; the app acts ([`Self::take_maint_request`]).
    maint_request: Option<MaintAction>,
    /// The coverage view, while it is open.
    coverage: Option<coverage::CoverageView>,
    /// Jobs the views want; the app starts and cancels them ([`Self::take_view_requests`]).
    view_requests: Vec<ViewRequest>,
    /// The model whose copyright text is shown.
    copyright: Option<ElevationModel>,
    license_open: bool,
}

impl OptionsDlg {
    pub fn new(settings: &Settings) -> Self {
        Self {
            key: map::KeyEdit::new(&settings.gui.thunderforest_api_key),
            stations: StationsState::new(settings),
            map_dlg: None,
            tile_dir: None,
            map_store: None,
            request: None,
            disclaimer_dlg: None,
            feed_dlg: None,
            sizes: Vec::new(),
            maint_enabled: false,
            web_state: WebState::Off,
            maint_request: None,
            coverage: None,
            view_requests: Vec::new(),
            copyright: None,
            license_open: false,
            draft: settings.clone(),
            tab: OptionsTab::default(),
            errors: Vec::new(),
        }
    }

    /// Tile cache directory for the station map.
    pub fn with_tile_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.tile_dir = dir;
        self
    }

    /// Tile store for the station map and the coverage view.
    pub fn with_map_store(mut self, store: Option<Arc<TileStore>>) -> Self {
        self.map_store = store;
        self
    }

    /// Load the QRV data of My and DX if their call or position changed since the last call.
    /// `load(call, loc)` reads the stored entries. Cheap otherwise, so it may run every frame.
    pub fn refresh_qrvs(&mut self, load: &mut dyn FnMut(&str, &str) -> Vec<Qrv>) {
        self.stations.refresh_qrvs(&self.draft, load);
    }

    /// The QRV entries the user changed, to write when OK was pressed.
    pub fn pending_qrvs(&self) -> Vec<Qrv> {
        self.stations.pending_qrvs()
    }

    /// The horizon or obstruction dialog requested since the last call. The app opens it on a
    /// copy of [`Self::draft`].
    pub fn take_request(&mut self) -> Option<StationAction> {
        self.request.take()
    }

    /// The Database tab button pressed since the last call.
    pub fn take_maint_request(&mut self) -> Option<MaintAction> {
        self.maint_request.take()
    }

    /// The coverage and grid jobs requested since the last call.
    pub fn take_view_requests(&mut self) -> Vec<ViewRequest> {
        std::mem::take(&mut self.view_requests)
    }

    /// The station map, while it is open.
    pub fn map_dialog(&mut self) -> Option<&mut MapStationDlg> {
        self.map_dlg.as_mut().map(|(_, d)| d)
    }

    /// The coverage job's answer.
    pub fn set_coverage(&mut self, found: &[String], missing: &[String]) {
        if let Some(v) = &mut self.coverage {
            v.set_result(found, missing);
        }
    }

    pub fn coverage_failed(&mut self, text: &str) {
        if let Some(v) = &mut self.coverage {
            v.fail(text);
        }
    }

    /// The elevation grid job's answer.
    pub fn set_grid(&mut self, cells: Vec<(GRect, i16)>) {
        if let Some(d) = self.map_dialog() {
            d.set_grid(cells);
        }
    }

    pub fn grid_failed(&mut self, text: &str) {
        if let Some(d) = self.map_dialog() {
            d.grid_failed(text);
        }
    }

    /// Draw the dialog; `Some` once the user pressed OK (and the draft is valid) or Cancel.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<OptionsOutcome> {
        let mut outcome = None;
        let mut open_map = None;
        let mut request = None;
        let mut maint = None;
        let mut elevation_action = None;
        let mut license = false;
        let mut feed_action = planes::FeedAction::None;
        egui::Modal::new(egui::Id::unique("options_dlg")).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.heading("Options");
            ui.horizontal_wrapped(|ui| {
                for (tab, name) in OptionsTab::ALL {
                    ui.selectable_value(&mut self.tab, tab, name);
                }
            });
            ui.separator();
            ScrollArea::both()
                .max_height(380.0)
                .auto_shrink(false)
                .show(ui, |ui| {
                    let d = &mut self.draft;
                    match self.tab {
                        OptionsTab::General => general::show(ui, d),
                        OptionsTab::Database => {
                            maint = database::show(ui, d, &self.sizes, self.maint_enabled)
                        }
                        OptionsTab::Stations => match stations::show(ui, d, &mut self.stations) {
                            Some(StationAction::Map(who)) => open_map = Some(who),
                            other => request = other,
                        },
                        OptionsTab::Map => map::show(ui, d, &mut self.key),
                        OptionsTab::Elevation => elevation_action = elevation::show(ui, d),
                        OptionsTab::Path => path::show(ui, d),
                        OptionsTab::Planes => feed_action = planes::show(ui, d),
                        OptionsTab::Alarm => alarm::show(ui, d),
                        OptionsTab::Network => network::show(ui, d, &self.web_state),
                        OptionsTab::Track => track::show(ui, d),
                        OptionsTab::Cat => cat::show(ui, d),
                        OptionsTab::SpecLab => speclab::show(ui, d),
                        OptionsTab::Watchlist => watchlist::show(ui, d),
                        OptionsTab::Info => license = info::show(ui),
                    }
                });
            ui.separator();
            for e in &self.errors {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            for w in network::port_warnings(&self.draft) {
                ui.colored_label(ui.visuals().warn_fg_color, w);
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    self.errors = validate(&self.draft);
                    if self.errors.is_empty() {
                        outcome = Some(OptionsOutcome::Ok(self.draft.clone()));
                    }
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(OptionsOutcome::Cancel);
                }
            });
        });
        if request.is_some() {
            self.request = request;
        }
        if maint.is_some() {
            self.maint_request = maint;
        }
        match elevation_action {
            Some(ElevationAction::Copyright(m)) => self.copyright = Some(m),
            Some(ElevationAction::Coverage(m)) => self.open_coverage(m),
            None => {}
        }
        self.license_open |= license;
        self.show_texts(ctx);
        self.show_coverage(ctx);
        if let Some(who) = open_map {
            let d = &self.draft;
            let (call, lat, lon) = match who {
                Who::My => (&d.my_call, d.my_lat, d.my_lon),
                Who::Dx => (&d.dx_call, d.dx_lat, d.dx_lon),
            };
            let (overlay, model) = (
                d.gui.stations_map_overlay_elevation,
                d.elevation_enabled.effective_model(),
            );
            self.map_dlg = Some((
                who,
                MapStationDlg::new(call, lat, lon, self.tile_dir.clone())
                    .with_map_store(self.map_store.clone())
                    .with_elevation(overlay, model),
            ));
        }
        self.handle_feed_action(feed_action);
        self.show_map(ctx);
        self.show_feed_dialogs(ctx);
        outcome
    }

    /// "Show coverage": the only start of the catalogue download.
    fn open_coverage(&mut self, model: ElevationModel) {
        if self.coverage.is_some() {
            return;
        }
        let a = &self.draft.area;
        let area = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
        self.coverage = Some(coverage::CoverageView::new(model, area));
        self.view_requests.push(ViewRequest::Coverage {
            model,
            base_url: self.draft.gui.elevation_base_url.clone(),
            area,
        });
    }

    fn show_coverage(&mut self, ctx: &egui::Context) {
        let Some(v) = &mut self.coverage else { return };
        if v.show(
            ctx,
            &self.draft,
            self.tile_dir.as_ref(),
            self.map_store.as_ref(),
        ) {
            self.coverage = None;
            self.view_requests.push(ViewRequest::CancelCoverage);
        }
    }

    /// The copyright of an elevation model and the license, as windows over the dialog.
    fn show_texts(&mut self, ctx: &egui::Context) {
        if let Some(m) = self.copyright {
            let title = format!("{} Copyright", m.dir_name());
            if text_window(ctx, "opt_copyright", &title, m.copyright()) {
                self.copyright = None;
            }
        }
        if self.license_open && text_window(ctx, "opt_license", "License", info::LICENSE) {
            self.license_open = false;
        }
    }

    fn handle_feed_action(&mut self, action: planes::FeedAction) {
        match action {
            planes::FeedAction::None => {}
            planes::FeedAction::Select(name) => {
                self.disclaimer_dlg = disclaimer::select_feed(&mut self.draft, &name);
            }
            planes::FeedAction::Settings => {
                let stored = self
                    .draft
                    .feed_settings
                    .get(&self.draft.feed)
                    .and_then(|v| v.as_table());
                self.feed_dlg = FeedSettingsDlg::new(&self.draft.feed, stored);
            }
        }
    }

    /// Disclaimer (Accept remembers the feed and selects it, Decline keeps the old one) and
    /// feed settings (OK stores the table that a fresh feed accepted).
    fn show_feed_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(dlg) = self.disclaimer_dlg.take() {
            let result = dlg.show(ctx);
            self.disclaimer_dlg = disclaimer::resolve(&mut self.draft, dlg, result);
        }
        if let Some(mut dlg) = self.feed_dlg.take() {
            match dlg.show(ctx) {
                FeedSettingsResult::Open => self.feed_dlg = Some(dlg),
                FeedSettingsResult::Ok(t) => {
                    self.draft.feed_settings.insert(dlg.feed, t.into());
                }
                FeedSettingsResult::Cancel => {}
            }
        }
    }

    /// The station map: OK moves the station (the locator field follows on the next frame).
    fn show_map(&mut self, ctx: &egui::Context) {
        let Some((who, dlg)) = &mut self.map_dlg else {
            return;
        };
        let who = *who;
        let result = dlg.show(ctx, &self.draft);
        self.draft.gui.stations_map_overlay_elevation = dlg.overlay();
        if let Some(r) = dlg.take_grid_request() {
            self.view_requests.push(ViewRequest::Grid(r));
        }
        match result {
            MapStationResult::Open => {}
            MapStationResult::Cancel => self.close_map(),
            MapStationResult::Ok(p) => {
                let d = &mut self.draft;
                match who {
                    Who::My => (d.my_lat, d.my_lon) = (p.lat, p.lon),
                    Who::Dx => (d.dx_lat, d.dx_lon) = (p.lat, p.lon),
                }
                self.close_map();
            }
        }
    }

    /// Closing the station map stops its grid job.
    fn close_map(&mut self) {
        self.map_dlg = None;
        self.view_requests
            .push(ViewRequest::Grid(GridRequest::Cancel));
    }
}

/// A window with a scrolling read-only text and a Close button; true once it was closed.
fn text_window(ctx: &egui::Context, id: &str, title: &str, body: &str) -> bool {
    let mut close = false;
    let r = egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(title);
        ScrollArea::vertical()
            .id_salt(id)
            .max_height(360.0)
            .show(ui, |ui| {
                ui.label(body.trim_start());
            });
        close = ui.button("Close").clicked();
    });
    close || r.should_close()
}
