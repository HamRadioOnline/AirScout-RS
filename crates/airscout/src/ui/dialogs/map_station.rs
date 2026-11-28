//! `MapStationDlg`: move a station on a map. The marker is dragged with the mouse; latitude,
//! longitude and the 6-character locator follow it, and the locator cell is drawn as a polygon.
//! The elevation grid overlay (zoom 17 and above, `MapStationDlg.cs:269-424`) is calculated by
//! a job the app runs: the dialog says what it wants ([`MapStationDlg::take_grid_request`]) and
//! draws what arrives ([`MapStationDlg::set_grid`]).

use crate::coverage::{GridRequest, GridState};
use crate::tiles::{self, TileProvider};
use as_elevation::ElevationModel;
use as_geo::latlon::{GPoint, GRect};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_maps::TileStore;
use as_settings::Settings;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use std::path::PathBuf;
use std::sync::Arc;
use walkers::{lon_lat, Map, MapMemory, StyledSources};

/// Pick radius of the marker [px].
const MARKER_R: f32 = 8.0;
/// Zoom of the map when the dialog opens: a 6-character cell is a few kilometres wide.
const START_ZOOM: f64 = 12.0;
/// A grid cell narrower than this [px] gets no elevation label.
const LABEL_MIN_CELL_PX: f32 = 28.0;
/// Text of the overlay checkbox (`cb_Options_StationsMap_OverlayElevation`).
const OVERLAY_LABEL: &str = "Overlay elevation grid (zoom ≥ 17)";

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapStationResult {
    Open,
    /// OK with the new position.
    Ok(GPoint),
    Cancel,
}

/// Position of the station with its derived locator; no GUI. [`Self::drag_to`] is what the
/// marker drag calls.
#[derive(Clone, Debug, PartialEq)]
pub struct MapStationState {
    pub call: String,
    pub lat: f64,
    pub lon: f64,
    /// 6-character locator of `lat`/`lon`.
    pub loc: String,
}

impl MapStationState {
    pub fn new(call: &str, lat: f64, lon: f64) -> Self {
        let mut s = Self {
            call: call.to_string(),
            lat,
            lon,
            loc: String::new(),
        };
        s.loc = s.derived_loc().unwrap_or_default();
        s
    }

    fn derived_loc(&self) -> Option<String> {
        maidenhead::loc_from_lat_lon(self.lat, self.lon, false, 3, false)
            .ok()
            .flatten()
    }

    pub fn position(&self) -> GPoint {
        GPoint::new(self.lat, self.lon)
    }

    /// The marker was dragged to `p` (clamped to the valid range); position and locator follow.
    /// A non-finite point is ignored.
    pub fn drag_to(&mut self, p: GPoint) {
        if !p.lat.is_finite() || !p.lon.is_finite() {
            return;
        }
        self.set_position(
            p.lat.clamp(-89.999_999, 89.999_999),
            p.lon.clamp(-180.0, 179.999_999),
        );
    }

    /// Latitude or longitude was typed.
    pub fn set_position(&mut self, lat: f64, lon: f64) {
        self.lat = lat;
        self.lon = lon;
        self.loc = self.derived_loc().unwrap_or_default();
    }

    /// A locator was typed: a valid one of at least 6 characters moves the marker to its centre.
    /// Returns whether it was taken.
    pub fn set_loc(&mut self, text: &str) -> bool {
        let t = text.trim();
        if t.len() < 6 || !maidenhead::check(t) {
            return false;
        }
        match maidenhead::lat_lon_from_loc(t, PositionInRectangle::MiddleMiddle) {
            Ok((lat, lon)) => {
                self.set_position(lat, lon);
                true
            }
            Err(_) => false,
        }
    }

    /// Corners of the locator cell (closed by the caller); empty if the position has no locator.
    pub fn polygon(&self) -> Vec<GPoint> {
        match maidenhead::bounds_from_loc(&self.loc) {
            Ok(r) => vec![
                GPoint::new(r.min_lat, r.min_lon),
                GPoint::new(r.min_lat, r.max_lon),
                GPoint::new(r.max_lat, r.max_lon),
                GPoint::new(r.max_lat, r.min_lon),
            ],
            Err(_) => Vec::new(),
        }
    }
}

/// The dialog: state plus the map widget's memory.
pub struct MapStationDlg {
    pub state: MapStationState,
    /// Text of the locator field while it is being edited.
    loc_text: String,
    memory: MapMemory,
    tiles: Option<StyledSources>,
    provider: Option<TileProvider>,
    /// Tile cache directory; `None` shows no tiles.
    tile_dir: Option<PathBuf>,
    /// Tile store read before the tile server.
    map_store: Option<Arc<TileStore>>,
    /// The pointer is on the marker: the map must not pan.
    on_marker: bool,
    grid: GridState,
    /// The job the grid wants started or stopped; the app takes it.
    grid_req: Option<GridRequest>,
    grid_status: String,
}

impl MapStationDlg {
    pub fn new(call: &str, lat: f64, lon: f64, tile_dir: Option<PathBuf>) -> Self {
        let state = MapStationState::new(call, lat, lon);
        let mut memory = MapMemory::default();
        let _ = memory.set_zoom(START_ZOOM);
        memory.center_at(lon_lat(state.lon, state.lat));
        Self {
            loc_text: state.loc.clone(),
            state,
            memory,
            tiles: None,
            provider: None,
            tile_dir,
            map_store: None,
            on_marker: false,
            grid: GridState::new(false, ElevationModel::None),
            grid_req: None,
            grid_status: String::new(),
        }
    }

    /// Tile store the map reads before the tile server.
    pub fn with_map_store(mut self, store: Option<Arc<TileStore>>) -> Self {
        self.map_store = store;
        self
    }

    /// The elevation overlay setting and the model it reads (the effective one).
    pub fn with_elevation(mut self, overlay: bool, model: ElevationModel) -> Self {
        self.grid = GridState::new(overlay, model);
        self
    }

    /// The overlay checkbox.
    pub fn overlay(&self) -> bool {
        self.grid.overlay()
    }

    pub fn set_zoom(&mut self, zoom: f64) {
        let _ = self.memory.set_zoom(zoom);
    }

    /// The job to start (a newer one supersedes the older) or to cancel, since the last call.
    pub fn take_grid_request(&mut self) -> Option<GridRequest> {
        self.grid_req.take()
    }

    /// The grid job's answer.
    pub fn set_grid(&mut self, cells: Vec<(GRect, i16)>) {
        self.grid.set_cells(cells);
        self.grid_status = "Calculating elevation grid finished.".into();
    }

    pub fn grid_failed(&mut self, text: &str) {
        self.grid_status = format!("elevation grid: {text}");
    }

    pub fn grid_cells(&self) -> &[(GRect, i16)] {
        self.grid.cells()
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

    /// Draw the dialog as a modal. The map tile layer follows the provider in `settings`.
    pub fn show(&mut self, ctx: &egui::Context, settings: &Settings) -> MapStationResult {
        self.ensure_tiles(ctx, settings);
        let mut result = MapStationResult::Open;
        egui::Modal::new(egui::Id::unique("map_station_dlg")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(format!("Station {}", self.state.call));
            self.fields(ui);
            self.overlay_row(ui);
            self.map(ui, settings);
            if !self.grid_status.is_empty() && self.grid.overlay() {
                ui.label(&self.grid_status);
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    result = MapStationResult::Ok(self.state.position());
                }
                if ui.button("Cancel").clicked() {
                    result = MapStationResult::Cancel;
                }
            });
        });
        result
    }

    fn fields(&mut self, ui: &mut egui::Ui) {
        let (mut lat, mut lon) = (self.state.lat, self.state.lon);
        let mut moved = false;
        ui.horizontal(|ui| {
            let l = ui.label("Latitude");
            moved |= ui
                .add(
                    egui::DragValue::new(&mut lat)
                        .range(-90.0..=90.0)
                        .speed(0.0005),
                )
                .labelled_by(l.id)
                .changed();
            let l = ui.label("Longitude");
            moved |= ui
                .add(
                    egui::DragValue::new(&mut lon)
                        .range(-180.0..=180.0)
                        .speed(0.0005),
                )
                .labelled_by(l.id)
                .changed();
            let l = ui.label("Locator");
            let r = ui
                .add(egui::TextEdit::singleline(&mut self.loc_text).desired_width(80.0))
                .labelled_by(l.id);
            if r.changed() {
                self.state.set_loc(&self.loc_text);
            }
        });
        if moved {
            self.state.set_position(lat, lon);
        }
        // a locator that is being typed stays as typed; otherwise it follows the position
        if !self.state.loc.eq_ignore_ascii_case(&self.loc_text) && !self.loc_text_is_partial() {
            self.loc_text = self.state.loc.clone();
        }
    }

    fn overlay_row(&mut self, ui: &mut egui::Ui) {
        let mut on = self.grid.overlay();
        let none = self.grid.model() == ElevationModel::None;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!none, |ui| ui.checkbox(&mut on, OVERLAY_LABEL));
            if none {
                ui.label("No elevation model enabled");
            }
        });
        if on != self.grid.overlay() {
            self.grid.set_overlay(on);
            self.grid_status.clear();
        }
    }

    /// The field holds an unfinished (or invalid) locator the user is still typing.
    fn loc_text_is_partial(&self) -> bool {
        let t = self.loc_text.trim();
        t.len() < 6 || !maidenhead::check(t)
    }

    fn map(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        let Self {
            state,
            memory,
            tiles,
            on_marker,
            loc_text,
            grid,
            grid_req,
            grid_status,
            ..
        } = self;
        let mut map = Map::new(None, memory, lon_lat(state.lon, state.lat)).panning(!*on_marker);
        if let Some(t) = tiles.as_mut() {
            map = map.with_layer(t, settings.gui.map_opacity);
        }
        let out = ui.allocate_ui(Vec2::new(560.0, 340.0), |ui| {
            map.show(ui, |ui, _response, projector, _| {
                let to_pos =
                    |p: GPoint| -> Pos2 { projector.project(lon_lat(p.lon, p.lat)).to_pos2() };
                let rect = projector.map_rect();
                let tl = projector.unproject(rect.left_top().to_vec2());
                let br = projector.unproject(rect.right_bottom().to_vec2());
                let view = GRect::new(br.y(), tl.x(), tl.y(), br.x());
                if let Some(r) = grid.update(view, projector.zoom()) {
                    if matches!(r, GridRequest::Run { .. }) {
                        *grid_status = "Calculating elevation grid...".into();
                    }
                    *grid_req = Some(r);
                }
                draw_grid(ui, &grid.visible(view), &to_pos, rect);
                let painter = ui.painter();
                let poly: Vec<Pos2> = state.polygon().into_iter().map(to_pos).collect();
                if poly.len() >= 3 {
                    painter.add(Shape::convex_polygon(
                        poly,
                        Color32::from_rgba_unmultiplied(0, 120, 255, 40),
                        Stroke::new(2.0, Color32::from_rgb(0, 90, 200)),
                    ));
                }
                let centre = to_pos(state.position());
                let hit = ui.interact(
                    Rect::from_center_size(centre, Vec2::splat(2.0 * MARKER_R)),
                    ui.scope_id().with("station_marker"),
                    Sense::drag(),
                );
                if hit.dragged() {
                    if let Some(p) = hit.interact_pointer_pos() {
                        let u = projector.unproject(p.to_vec2());
                        state.drag_to(GPoint::new(u.y(), u.x()));
                    }
                }
                ui.painter().circle(
                    to_pos(state.position()),
                    MARKER_R,
                    Color32::from_rgb(220, 40, 40),
                    Stroke::new(1.5, Color32::BLACK),
                );
                (hit.hovered() || hit.dragged(), hit.dragged())
            })
        });
        let (over, dragged) = out.inner.inner;
        *on_marker = over;
        if dragged {
            *loc_text = state.loc.clone();
        }
    }
}

/// The cells of the grid: filled and stroked in their colour at alpha 100, with the elevation
/// as black text in the middle (`bw_Elevationgrid_DoWork`, `GMapTextPolygon.OnRender`).
fn draw_grid(
    ui: &egui::Ui,
    cells: &[crate::coverage::GridCell],
    to_pos: &dyn Fn(GPoint) -> Pos2,
    clip: Rect,
) {
    let painter = ui.painter();
    for c in cells {
        let r = Rect::from_two_pos(
            to_pos(GPoint::new(c.rect.max_lat, c.rect.min_lon)),
            to_pos(GPoint::new(c.rect.min_lat, c.rect.max_lon)),
        );
        if !r.intersects(clip) {
            continue;
        }
        let [red, green, blue] = c.colour;
        let col = Color32::from_rgba_unmultiplied(red, green, blue, 100);
        painter.rect_filled(r, 0.0, col);
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, col), StrokeKind::Middle);
        if r.width() >= LABEL_MIN_CELL_PX {
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                format!("{} m", c.elv),
                FontId::proportional(9.0),
                Color32::BLACK,
            );
        }
    }
}
