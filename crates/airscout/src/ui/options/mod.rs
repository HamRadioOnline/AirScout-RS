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
use crate::ui::dialogs::feed_file::{FeedFileDlg, FeedFileKind, FeedFileRequest, FeedFileResult};
use crate::ui::dialogs::feed_settings::{
    desktop_default_feed, FeedSettingsDlg, FeedSettingsResult,
};
use crate::ui::dialogs::map_station::{MapStationDlg, MapStationResult};
use crate::ui::forms::geometry::designer_rect;
use crate::ui::forms::trace;
use crate::ui::forms::widgets::{self, tab_control_with_guard};
use crate::web_service::WebState;
use as_elevation::ElevationModel;
use as_geo::latlon::GRect;
use as_maps::TileStore;
use as_settings::Settings;
use as_stations::{Location, Qrv};
pub use cat::omnirig_rig_text;
use chrono::{DateTime, Utc};
pub use database::MaintAction;
use eframe::egui::{self, pos2, vec2, Rect, ScrollArea, Sense};
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
    /// The tab pages in Designer order: `(tab, designer control name, caption)`.
    ///
    /// The five per-model elevation pages (`tab_Options_GLOBE`, `tab_Options_SRTM3`,
    /// `tab_Options_SRTM1`, `tab_Options_ASTER3`, `tba_Option_ASTER1`) are collapsed
    /// into the single interim `Elevation` page (Decision 1), and `Spectrum` is the
    /// displayed name of `SpecLab`. The Designer's `tab_Options_Misc` page is deferred
    /// to Phase 3B.
    const ALL: [(OptionsTab, &'static str, &'static str); 14] = [
        (OptionsTab::General, "tab_Options_General", "General"),
        (OptionsTab::Database, "tab_Options_Database", "Database"),
        (OptionsTab::Stations, "tab_Options_Stations", "Stations"),
        (OptionsTab::Map, "tab_Options_Map", "Map"),
        (OptionsTab::Elevation, "tab_Options_Elevation", "Elevation"),
        (OptionsTab::Path, "tab_Options_Path", "Path"),
        (OptionsTab::Planes, "tab_Options_Planes", "Planes"),
        (OptionsTab::Alarm, "tab_Options_Alarm", "Alarm"),
        (OptionsTab::Network, "tab_Options_Network", "Network"),
        (OptionsTab::SpecLab, "tab_Options_SpecLab", "Spectrum"),
        (OptionsTab::Track, "tab_Options_Track", "Track"),
        (OptionsTab::Cat, "tab_Options_CAT", "CAT"),
        (OptionsTab::Watchlist, "tab_Options_Watchlist", "Watchlist"),
        (OptionsTab::Info, "tab_Options_Info", "Info"),
    ];
}

/// Height of the Options dialog's status strip (`ss_Options`, 22 in the Designer).
const OPTIONS_STATUS_HEIGHT: f32 = 22.0;

/// The status-strip summary and full hover text: errors and warnings, single-line
/// summary joined with `"; "` (`"Status"` while idle), hover joined with `"\n"`.
fn status_text(
    tab_errors: &[String],
    errors: &[String],
    messages: [Option<&str>; 2],
    draft: &Settings,
) -> (String, String) {
    let warnings = network::port_warnings(draft);
    let messages: Vec<&str> = tab_errors
        .iter()
        .chain(errors)
        .map(String::as_str)
        .chain(messages.into_iter().flatten())
        .chain(warnings.iter().map(String::as_str))
        .collect();
    let hover = messages.join("\n");
    let visible = if messages.is_empty() {
        "Status".to_string()
    } else {
        messages.join("; ").replace('\n', " ")
    };
    (visible, hover)
}

/// Commit a tab switch, or refuse it. A refusal replaces the departing page's errors in the
/// status strip (never appends them); a committed switch clears them. Shared by
/// [`OptionsDlg::try_select_tab`] and the guarded tab control.
fn commit_switch(
    tab: &mut OptionsTab,
    tab_errors: &mut Vec<String>,
    departing: &[String],
    next: OptionsTab,
) -> bool {
    tab_errors.clear();
    if departing.is_empty() {
        *tab = next;
        true
    } else {
        tab_errors.extend_from_slice(departing);
        false
    }
}

pub struct OptionsDlg {
    /// The copy of the settings the tabs edit.
    pub draft: Settings,
    pub tab: OptionsTab,
    /// Why the last OK was refused.
    pub errors: Vec<String>,
    /// Why the last tab switch was refused: the departing page's errors, kept apart from the
    /// OK errors and cleared once the page is valid or left.
    tab_errors: Vec<String>,
    /// Test-only refusal hook: when `true`, the current page refuses departure, so the
    /// guarded tab switch is exercised. Production code never sets this.
    #[doc(hidden)]
    pub test_invalid_departure: bool,
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
    /// The disclaimer of the feed the user just picked for a slot; the draft keeps the old feed
    /// until it is accepted.
    pub disclaimer_dlg: Option<(usize, DisclaimerDlg)>,
    /// The settings editor of the selected feed.
    pub feed_dlg: Option<FeedSettingsDlg>,
    feed_file_dlg: Option<FeedFileDlg>,
    feed_file_request: Option<FeedFileRequest>,
    feed_file_status: Option<String>,
    tmp_dir: Option<PathBuf>,
    /// The overlay file picker of the Map tab, while it is open (Decision 7).
    overlay_file_dlg: Option<map::OverlayFileDlg>,
    /// Database files and sizes of the Database tab; the app keeps them current.
    pub sizes: Vec<(DbId, String, f64)>,
    /// False while playing or while a job runs: the maintenance buttons are disabled.
    pub maint_enabled: bool,
    /// A destructive maintenance job is running: OK/Cancel stay disabled until it ends, so the
    /// restarted workers cannot race the delete/clear (#155).
    pub maint_running: bool,
    /// Background workers still stopping: the Database tab shows a note and keeps the destructive
    /// buttons disabled until this reaches `None` (#155).
    pub maint_waiting: Option<usize>,
    /// Bind state of the web server; the app keeps it current.
    pub web_state: WebState,
    /// The maintenance button the user pressed; the app acts ([`Self::take_maint_request`]).
    maint_request: Option<MaintAction>,
    /// The coverage view, while it is open.
    coverage: Option<coverage::CoverageView>,
    /// The General tab's covered-area map (`gm_Options_Coverage`).
    preview: general::AreaPreview,
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
            feed_file_dlg: None,
            feed_file_request: None,
            feed_file_status: None,
            tmp_dir: None,
            overlay_file_dlg: None,
            sizes: Vec::new(),
            maint_enabled: false,
            maint_running: false,
            maint_waiting: None,
            web_state: WebState::Off,
            maint_request: None,
            coverage: None,
            view_requests: Vec::new(),
            preview: general::AreaPreview::default(),
            copyright: None,
            license_open: false,
            draft: settings.clone(),
            tab: OptionsTab::default(),
            errors: Vec::new(),
            tab_errors: Vec::new(),
            test_invalid_departure: false,
        }
    }

    /// Select `tab`, validating the departing page first. Returns `false` on refusal.
    pub fn try_select_tab(&mut self, tab: OptionsTab) -> bool {
        let prev = self.tab;
        let departing = self.departing_errors();
        let ok = commit_switch(&mut self.tab, &mut self.tab_errors, &departing, tab);
        if ok {
            if prev == OptionsTab::Stations && self.tab != prev {
                self.stations.clear_band_error();
            }
            if prev == OptionsTab::Planes && self.tab != prev {
                self.feed_file_status = None;
            }
            self.note_general_entry(prev);
        }
        ok
    }

    /// On a transition into `General`, force the coverage preview to refit its
    /// camera on the next draw (`OptionsDlg.cs:235-238` refits on every entry).
    fn note_general_entry(&mut self, prev: OptionsTab) {
        if prev != OptionsTab::General && self.tab == OptionsTab::General {
            self.preview.on_enter();
        }
    }

    /// Validation errors on the currently selected page that must be resolved before leaving.
    fn departing_errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.test_invalid_departure {
            out.push("The current page has invalid values.".to_string());
        }
        if self.tab == OptionsTab::Stations {
            out.extend(self.validate_station_edits().err());
        } else if self.tab == OptionsTab::Planes {
            out.extend(fields::validate_planes(&self.draft));
        }
        out
    }

    /// The station checks OK runs: both stations and every staged QRV row while the Stations
    /// page is current (`tab_Options_Stations_Validating`, `OptionsDlg.cs:868-873`, fires when
    /// the page loses focus), else only the staged QRV rows. An untouched invalid station, such
    /// as the NaN DX position the main window leaves for an unknown call, never blocks OK from
    /// another page.
    pub(crate) fn validate_station_edits(&self) -> Result<(), String> {
        self.stations
            .validate_station_edits(&self.draft, self.tab == OptionsTab::Stations)
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

    /// The coverage preview's camera centre as `(lon, lat)` and its zoom.
    /// Test hook for the General re-entry regression.
    #[doc(hidden)]
    pub fn preview_camera(&self) -> (Option<(f64, f64)>, f64) {
        self.preview.camera()
    }

    /// Move the coverage preview's camera away from the fitted view.
    /// Test hook for the General re-entry regression.
    #[doc(hidden)]
    pub fn move_preview_camera(&mut self, lon: f64, lat: f64, zoom: f64) {
        self.preview.move_camera(lon, lat, zoom);
    }

    /// Load the QRV data of My and DX if their call or position changed since the last call.
    /// `load(call, loc)` reads the stored entries. Returns whether either key changed.
    pub fn refresh_qrvs(&mut self, load: &mut dyn FnMut(&str, &str) -> Vec<Qrv>) -> bool {
        self.stations.refresh_qrvs(&self.draft, load)
    }

    /// Station positions changed in this draft, for writing on outer OK.
    pub fn pending_locations(&self, now: DateTime<Utc>) -> Vec<Location> {
        self.stations.pending_locations(&self.draft, now)
    }

    /// Retain the draft and show a local write error in the status strip.
    pub fn retain_write_error(&mut self, error: String) {
        self.errors = vec![error];
        self.tab_errors.clear();
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

    pub(crate) fn with_tmp_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.tmp_dir = dir;
        self
    }

    pub(crate) fn take_feed_file_request(&mut self) -> Option<FeedFileRequest> {
        self.feed_file_request.take()
    }

    pub(crate) fn set_feed_file_status(&mut self, message: String) {
        self.feed_file_status = Some(message);
    }

    /// The status-strip summary and full hover text (errors and warnings).
    ///
    /// The summary is one line, joined with `"; "`, and `"Status"` while idle; the hover
    /// text lists every error and warning joined with `"\n"` (no trailing newline, and
    /// warnings are not dropped when errors exist).
    #[doc(hidden)]
    pub fn status_texts(&self) -> (String, String) {
        status_text(
            &self.tab_errors,
            &self.errors,
            [self.stations.message(), self.feed_file_status.as_deref()],
            &self.draft,
        )
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
        let mut map_action = map::MapAction::None;
        // `Area` only reads `default_size` on its first sizing pass, then persists the
        // previous frame's rect (and `Area::constrain` only moves that rect, it never
        // shrinks it). So bound the dialog by the live viewport every frame: it clips
        // and scrolls when the window shrinks below the 784×561 Designer client, and
        // grows back to that client when the window enlarges.
        egui::Modal::new(egui::Id::unique("options_dlg")).show(ctx, |ui| {
            let viewport = ctx.content_rect().size();
            let margin = ui.spacing().menu_margin;
            ui.set_max_size(vec2(
                viewport.x - f32::from(margin.left) - f32::from(margin.right),
                viewport.y - f32::from(margin.top) - f32::from(margin.bottom),
            ));

            // The dialog title stays fixed while the client below it scrolls.
            ui.heading("Air Scout Options");

            // An outer scroll area constrains the dialog to the available space, so on a
            // viewport smaller than the 784×561 Designer client the footer and status
            // strip stay reachable instead of clipping. Inside it, a resizable client at
            // the Designer size can grow with its handle (the trace stays in Designer
            // points regardless).
            ScrollArea::both().id_salt("options_scroll").show(ui, |ui| {
                egui::Resize::default()
                    .id_salt("options_resize")
                    .default_size(vec2(784.0, 561.0))
                    .min_size(vec2(784.0, 561.0))
                    .show(ui, |ui| {
                        let client_size = vec2(784.0, 561.0);
                        let (client, _) = ui.allocate_exact_size(client_size, Sense::hover());
                        trace::record_window(ui, client);

                        // `ss_Options`: the status strip, docked Bottom at 22 pt.
                        let status_rect = Rect::from_min_max(
                            pos2(client.min.x, client.max.y - OPTIONS_STATUS_HEIGHT),
                            client.max,
                        );
                        // `tc_Options`, OK and Cancel at their Designer rects.
                        let tc_rect = designer_rect(client, [12, 12], [679, 524]);
                        let ok_rect = designer_rect(client, [697, 471], [75, 23]);
                        let cancel_rect = designer_rect(client, [697, 501], [75, 23]);

                        // The departing page's validation errors, precomputed before the
                        // tab control so the switch guard never holds a conflicting borrow
                        // of `self.draft`.
                        let departing = self.departing_errors();
                        if departing.is_empty() {
                            // Fixed in place: a refused switch's errors no longer apply.
                            self.tab_errors.clear();
                        }
                        let OptionsDlg {
                            draft,
                            tab,
                            errors,
                            tab_errors,
                            key,
                            stations,
                            sizes,
                            maint_enabled,
                            maint_waiting,
                            maint_running,
                            web_state,
                            preview,
                            map_store,
                            feed_file_status,
                            ..
                        } = self;
                        // The tab at frame start: a click that lands on General
                        // counts as a re-entry below (the guard already moved
                        // `*tab` by the time the General body runs).
                        let prev = *tab;

                        // Tab control: Designer order, with the tab body scrolling
                        // independently of the header and footer. The guard runs
                        // `try_select_tab`'s logic before the destination body renders,
                        // so a refused switch keeps the departing tab selected.
                        let pages: Vec<(&str, &str)> = OptionsTab::ALL
                            .iter()
                            .map(|&(_, name, caption)| (name, caption))
                            .collect();
                        let mut selected = OptionsTab::ALL
                            .iter()
                            .position(|&(t, _, _)| t == *tab)
                            .expect("current tab is in ALL");
                        tab_control_with_guard(
                            ui,
                            "tc_Options",
                            tc_rect,
                            &pages,
                            &mut selected,
                            |i| commit_switch(tab, tab_errors, &departing, OptionsTab::ALL[i].0),
                            |ui, idx| {
                                ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                                    let d = &mut *draft;
                                    match OptionsTab::ALL[idx].0 {
                                        OptionsTab::General => {
                                            // Re-entering General refits the preview
                                            // camera (the user may have panned/zoomed
                                            // away while on another tab).
                                            if prev != OptionsTab::General {
                                                preview.on_enter();
                                            }
                                            general::show(ui, d, preview, map_store.as_ref())
                                        }
                                        OptionsTab::Database => {
                                            maint = database::show(
                                                ui,
                                                d,
                                                sizes,
                                                *maint_enabled,
                                                *maint_waiting,
                                            )
                                        }
                                        OptionsTab::Stations => {
                                            match stations::show(ui, d, &mut *stations) {
                                                Some(StationAction::Map(who)) => {
                                                    open_map = Some(who)
                                                }
                                                other => request = other,
                                            }
                                        }
                                        OptionsTab::Map => map_action = map::show(ui, d, &mut *key),
                                        OptionsTab::Elevation => {
                                            elevation_action = elevation::show(ui, d)
                                        }
                                        OptionsTab::Path => path::show(ui, d),
                                        OptionsTab::Planes => feed_action = planes::show(ui, d),
                                        OptionsTab::Alarm => alarm::show(ui, d),
                                        OptionsTab::Network => network::show(ui, d, &*web_state),
                                        OptionsTab::Track => track::show(ui, d),
                                        OptionsTab::Cat => cat::show(ui, d),
                                        OptionsTab::SpecLab => speclab::show(ui, d),
                                        OptionsTab::Watchlist => watchlist::show(ui, d),
                                        OptionsTab::Info => license = info::show(ui),
                                    }
                                });
                            },
                        );
                        if prev == OptionsTab::Stations && *tab != prev {
                            stations.clear_band_error();
                        }
                        if prev == OptionsTab::Planes && *tab != prev {
                            *feed_file_status = None;
                        }

                        // Status strip drawn after the tab control, so a refused switch's
                        // errors appear in the same frame. One line, truncated to the strip.
                        let (status_summary, hover) = status_text(
                            tab_errors.as_slice(),
                            errors.as_slice(),
                            [stations.message(), feed_file_status.as_deref()],
                            draft,
                        );
                        trace::scope(ui, "ss_Options", status_rect, |ui| {
                            let label_rect = Rect::from_min_size(
                                status_rect.min,
                                vec2(status_rect.width(), 17.0),
                            );
                            let response = widgets::label_truncated(
                                ui,
                                "tsl_Options_Status",
                                &status_summary,
                                label_rect,
                            );
                            if !hover.is_empty() {
                                response.on_hover_text(hover);
                            }
                        });

                        // OK / Cancel, disabled while a destructive maintenance job runs (#155).
                        let mut footer = egui::UiBuilder::new().max_rect(client);
                        if *maint_running {
                            footer = footer.disabled();
                        }
                        ui.scope_builder(footer, |ui| {
                            if widgets::button(ui, "btn_Options_OK", "OK", ok_rect).clicked() {
                                // OK reports its own errors; a refused switch's copy of
                                // the same page errors would only repeat them.
                                tab_errors.clear();
                                *errors = validate(draft);
                                errors.extend(
                                    stations
                                        .validate_station_edits(draft, *tab == OptionsTab::Stations)
                                        .err(),
                                );
                                if errors.is_empty() {
                                    outcome = Some(OptionsOutcome::Ok((*draft).clone()));
                                }
                            }
                            if widgets::button(ui, "btn_Options_Cancel", "Cancel", cancel_rect)
                                .clicked()
                            {
                                outcome = Some(OptionsOutcome::Cancel);
                            }
                        });
                    });
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
        if map_action == map::MapAction::SelectOverlay {
            self.overlay_file_dlg = Some(map::OverlayFileDlg::new(
                self.tmp_dir.clone(),
                self.draft.gui.overlay_filename.as_deref(),
            ));
        }
        self.show_map(ctx);
        self.show_feed_dialogs(ctx);
        self.show_overlay_file_dlg(ctx);
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
            planes::FeedAction::Select(slot, name) => {
                self.feed_file_status = None;
                self.disclaimer_dlg =
                    disclaimer::select_feed(&mut self.draft, slot, &name).map(|d| (slot, d));
            }
            planes::FeedAction::Settings(slot) => {
                self.feed_file_status = None;
                let feed = self.draft.feed_slot(slot).to_string();
                let stored = self
                    .draft
                    .feed_settings
                    .get(&feed)
                    .and_then(|v| v.as_table());
                self.feed_dlg = FeedSettingsDlg::new(&feed, stored);
            }
            planes::FeedAction::Default(slot) => {
                self.feed_file_status = None;
                let name = self.draft.feed_slot(slot).to_owned();
                if let Some(feed) = desktop_default_feed(&name) {
                    self.draft
                        .feed_settings
                        .insert(name, feed.settings().into());
                }
            }
            planes::FeedAction::Import(slot) => self.open_feed_file(FeedFileKind::Import, slot),
            planes::FeedAction::Export(slot) => self.open_feed_file(FeedFileKind::Export, slot),
        }
    }

    fn open_feed_file(&mut self, kind: FeedFileKind, slot: usize) {
        self.feed_file_dlg = Some(FeedFileDlg::new(
            kind,
            slot,
            self.draft.feed_slot(slot),
            self.tmp_dir.clone(),
        ));
    }

    /// Disclaimer (Accept remembers the feed and selects it, Decline keeps the old one) and
    /// feed settings (OK stores the table that a fresh feed accepted).
    fn show_feed_dialogs(&mut self, ctx: &egui::Context) {
        if let Some((slot, dlg)) = self.disclaimer_dlg.take() {
            let result = dlg.show(ctx);
            self.disclaimer_dlg =
                disclaimer::resolve(&mut self.draft, slot, dlg, result).map(|d| (slot, d));
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
        if let Some(mut dlg) = self.feed_file_dlg.take() {
            match dlg.show(ctx) {
                FeedFileResult::Open => self.feed_file_dlg = Some(dlg),
                FeedFileResult::Cancel => {}
                FeedFileResult::Submit(request) => self.feed_file_request = Some(request),
            }
        }
    }

    /// The overlay file picker: Use stores the chosen file in the draft.
    fn show_overlay_file_dlg(&mut self, ctx: &egui::Context) {
        let Some(mut dlg) = self.overlay_file_dlg.take() else {
            return;
        };
        match dlg.show(ctx) {
            map::OverlayFileResult::Open => self.overlay_file_dlg = Some(dlg),
            map::OverlayFileResult::Cancel => {}
            map::OverlayFileResult::Use(path) => self.draft.gui.overlay_filename = Some(path),
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
                self.stations.set_position(&mut self.draft, who, p);
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
