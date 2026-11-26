//! `TrafficDlg`: the routes of all planes of a time range on a map, with the area of interest
//! as a magenta rectangle (TrafficDlg.cs). The routes come from the `Traffic` job; the job id
//! is the dialog's own, so closing the dialog cancels only its job.

use crate::analysis::AnalysisJob;
use crate::app::AppState;
use crate::tiles::{self, TileProvider};
use crate::traffic::{route_alpha, traffic_range};
use crate::ui::analysis::analysis_enabled;
use crate::ui::map_geom::{fit_view, thin};
use as_geo::latlon::{GPoint, GRect};
use as_maps::TileStore;
use as_settings::{Area, Settings};
use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::Arc;
use walkers::{lon_lat, Map, MapMemory, StyledSources};

/// Format of the start and stop fields.
const FORMAT: &str = "%Y-%m-%d %H:%M";
/// Margin around the area when the dialog opens [deg] (:44).
const FIT_PAD_DEG: f64 = 1.0;
const MAP_SIZE: Vec2 = Vec2::new(640.0, 400.0);
const DEFAULT_OPACITY: u32 = 10;

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficResult {
    Open,
    Close,
    Show,
}

/// A date field: the text as typed, and the exact default it started from. The field has no
/// seconds, so the unchanged default keeps its seconds (else the last minute is cut off).
struct TimeField {
    text: String,
    exact: DateTime<Utc>,
}

impl TimeField {
    fn new(t: DateTime<Utc>) -> Self {
        Self {
            text: t.format(FORMAT).to_string(),
            exact: t,
        }
    }

    fn value(&self) -> Option<DateTime<Utc>> {
        if self.text == self.exact.format(FORMAT).to_string() {
            return Some(self.exact);
        }
        NaiveDateTime::parse_from_str(self.text.trim(), FORMAT)
            .ok()
            .map(|n| n.and_utc())
    }
}

pub struct TrafficDlg {
    from: TimeField,
    to: TimeField,
    /// Route opacity in percent, 10..=100.
    pub opacity: u32,
    /// Id of the job the dialog started last.
    pub job_id: u64,
    pub error: Option<String>,
    /// The thinned routes received so far.
    routes: Vec<Vec<GPoint>>,
    area: Area,
    memory: MapMemory,
    tiles: Option<StyledSources>,
    provider: Option<TileProvider>,
    tile_dir: Option<PathBuf>,
    /// Tile store read before the tile server.
    map_store: Option<Arc<TileStore>>,
}

impl TrafficDlg {
    pub fn new(
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        area: Area,
        tile_dir: Option<PathBuf>,
    ) -> Self {
        let mut memory = MapMemory::default();
        let rect = GRect::new(
            area.min_lat - FIT_PAD_DEG,
            area.min_lon - FIT_PAD_DEG,
            area.max_lat + FIT_PAD_DEG,
            area.max_lon + FIT_PAD_DEG,
        );
        let (centre, zoom) = fit_view(rect, MAP_SIZE.x, MAP_SIZE.y);
        let _ = memory.set_zoom(zoom);
        memory.center_at(lon_lat(centre.lon, centre.lat));
        Self {
            from: TimeField::new(from),
            to: TimeField::new(to),
            opacity: DEFAULT_OPACITY,
            job_id: 0,
            error: None,
            routes: Vec::new(),
            area,
            memory,
            tiles: None,
            provider: None,
            tile_dir,
            map_store: None,
        }
    }

    /// Tile store the map reads before the tile server.
    pub fn with_map_store(mut self, store: Option<Arc<TileStore>>) -> Self {
        self.map_store = store;
        self
    }

    /// The range of the fields; `None` while one is not a date.
    pub fn range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        Some((self.from.value()?, self.to.value()?))
    }

    /// Show was clicked: the routes are cleared (:100) and `id` is the new job.
    pub fn begin(&mut self, id: u64) {
        self.job_id = id;
        self.error = None;
        self.routes.clear();
    }

    /// Routes arrive thinned for drawing (`map_geom::thin`).
    pub fn add_routes(&mut self, routes: Vec<Vec<GPoint>>) {
        self.routes.extend(routes.into_iter().map(thin));
    }

    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    fn ensure_tiles(&mut self, ctx: &egui::Context, s: &Settings) {
        let Some(dir) = &self.tile_dir else { return };
        let provider = tiles::provider_by_name(&s.gui.map_provider, &s.gui.thunderforest_api_key);
        if self.tiles.is_none() || self.provider.as_ref() != Some(&provider) {
            self.tiles = Some(tiles::tile_layer(
                &provider,
                self.map_store.clone(),
                dir,
                ctx,
            ));
            self.provider = Some(provider);
        }
    }

    /// Draw the dialog; `idle` is true while no job runs.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        idle: bool,
        settings: &Settings,
        status: &str,
    ) -> TrafficResult {
        self.ensure_tiles(ctx, settings);
        let mut out = TrafficResult::Open;
        let mut open = true;
        egui::Window::new("Traffic")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (label, f) in [("Start", &mut self.from), ("Stop", &mut self.to)] {
                        let l = ui.label(label);
                        ui.add(egui::TextEdit::singleline(&mut f.text).desired_width(120.0))
                            .labelled_by(l.id);
                    }
                    let l = ui.label("Opacity %");
                    ui.add(egui::DragValue::new(&mut self.opacity).range(10..=100))
                        .labelled_by(l.id);
                    let ready = idle && self.range().is_some();
                    if ui.add_enabled(ready, egui::Button::new("Show")).clicked() {
                        out = TrafficResult::Show;
                    }
                    if ui.button("Close").clicked() {
                        out = TrafficResult::Close;
                    }
                });
                if self.range().is_none() {
                    ui.label(format!("Dates need the form {FORMAT}"));
                }
                self.map(ui, settings);
                if let Some(e) = &self.error {
                    ui.label(format!("error: {e}"));
                }
                ui.label(status);
            });
        if !open {
            out = TrafficResult::Close;
        }
        out
    }

    fn map(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        let Self {
            memory,
            tiles,
            routes,
            area,
            opacity,
            ..
        } = self;
        let centre = memory.detached().unwrap_or(lon_lat(
            (area.min_lon + area.max_lon) / 2.0,
            (area.min_lat + area.max_lat) / 2.0,
        ));
        let mut map = Map::new(None, memory, centre);
        if let Some(t) = tiles.as_mut() {
            map = map.with_layer(t, settings.gui.map_opacity);
        }
        let stroke = Stroke::new(
            1.0,
            Color32::from_black_alpha(route_alpha((*opacity).clamp(10, 100))),
        );
        let a = *area;
        ui.allocate_ui(MAP_SIZE, |ui| {
            map.show(ui, |ui, _response, projector, _| {
                let to_pos =
                    |p: GPoint| -> Pos2 { projector.project(lon_lat(p.lon, p.lat)).to_pos2() };
                let painter = ui.painter();
                for r in routes.iter() {
                    painter.add(Shape::line(r.iter().map(|&p| to_pos(p)).collect(), stroke));
                }
                let corners = [
                    GPoint::new(a.min_lat, a.min_lon),
                    GPoint::new(a.min_lat, a.max_lon),
                    GPoint::new(a.max_lat, a.max_lon),
                    GPoint::new(a.max_lat, a.min_lon),
                ];
                painter.add(Shape::closed_line(
                    corners.iter().map(|&p| to_pos(p)).collect(),
                    Stroke::new(3.0, Color32::MAGENTA),
                ));
            })
        });
    }
}

/// Show All Traffic (MapDlg.cs): the dialog for the whole loaded history.
pub fn open(state: &mut AppState) {
    let Some(h) = &state.history else { return };
    let positions = &h.data.positions;
    let (from, to) = traffic_range(
        positions.first().map(|p| p.time),
        positions.last().map(|p| p.time),
        Utc::now(),
    );
    let tile_dir = state.data_dir.as_deref().map(tiles::tiles_cache_dir);
    let dlg = TrafficDlg::new(from, to, state.settings.area, tile_dir)
        .with_map_store(state.map_store.clone());
    state.analysis_ui.traffic_dlg = Some(dlg);
}

/// Draw the dialog and act on what the user did; called every frame by the analysis tab.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.analysis_ui.traffic_dlg.take() else {
        return;
    };
    let idle = analysis_enabled(state).traffic;
    let status = state.status_line();
    match dlg.show(ctx, idle, &state.settings, &status) {
        TrafficResult::Open => {}
        TrafficResult::Close => {
            // `Close` cancels the worker (:182); never a job that started or ended since
            if !idle {
                state.cancel_job(dlg.job_id);
            }
            return;
        }
        TrafficResult::Show => {
            if let (true, Some((from, to))) = (idle, dlg.range()) {
                state.status = "Getting positions...".into();
                state.run_analysis_job(AnalysisJob::Traffic { from, to });
                dlg.begin(state.current_job_id());
            }
        }
    }
    state.analysis_ui.traffic_dlg = Some(dlg);
}

/// The Traffic job (current job `id`) delivered routes.
pub fn on_routes(state: &mut AppState, id: u64, routes: Vec<Vec<GPoint>>) {
    if let Some(d) = dlg_of(state, id) {
        d.add_routes(routes);
    }
}

/// The Traffic job failed.
pub fn on_failed(state: &mut AppState, id: u64, error: &str) {
    if let Some(d) = dlg_of(state, id) {
        d.fail(error.to_string());
    }
}

fn dlg_of(state: &mut AppState, id: u64) -> Option<&mut TrafficDlg> {
    state
        .analysis_ui
        .traffic_dlg
        .as_mut()
        .filter(|d| d.job_id == id)
}
