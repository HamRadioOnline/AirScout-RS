//! Central map: OSM tiles, distance rings, locator grid, path, beam wedges and live planes.
//!
//! The overlay geometry (`crate::overlays`) is computed once per path / settings change and kept
//! in [`MapView`]; a frame only projects the cached points and paints them. Planes are the
//! upstream plane icon, recoloured by potential and rotated along their track.

use crate::app::AppState;
use crate::overlay_image::{self, OverlayImage, OverlayLayer};
use crate::overlays::{self, LocatorCell, PathOverlay};
use crate::services::{AirportMark, WatchMark};
use crate::style::{plane_color, plane_icon_px, rgb, tooltip_lines};
use crate::tiles::{self, TileProvider};
pub use crate::ui::map_geom::{bbox_of, fit_view};
use crate::ui::map_geom::{beam_wedges, contains, hit_plane, pad, paths_bbox, HIT_SLACK_PX};
use as_aircraft::PlaneInfo;
use as_geo::latlon::{GPoint, GRect};
use as_geo::maidenhead::check_lat_lon;
use as_maps::TileStore;
use as_settings::{Secret, Settings};
use chrono::Utc;
use eframe::egui::{
    self, Align2, Color32, ColorImage, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke,
    StrokeKind, TextureHandle, TextureId, TextureOptions, Vec2,
};
use std::path::PathBuf;
use std::sync::Arc;
use walkers::{lon_lat, Map, MapMemory, Projector, StyledSources};

/// Alpha channel of the upstream plane icon (`AirScout/plane.png`, 128 x 128, nose up): the
/// icon is a black silhouette, so only its alpha matters (`CreatePlaneIcon` keeps the alpha and
/// replaces the colour).
const PLANE_ALPHA: &[u8; PLANE_ICON_SIDE * PLANE_ICON_SIDE] =
    include_bytes!("../../assets/plane_alpha.bin");
const PLANE_ICON_SIDE: usize = 128;

/// The plane icon as a white image with the upstream alpha, so a tint gives the exact colour.
pub fn plane_icon_image() -> ColorImage {
    let rgba: Vec<u8> = PLANE_ALPHA
        .iter()
        .flat_map(|&a| [255, 255, 255, a])
        .collect();
    ColorImage::from_rgba_unmultiplied([PLANE_ICON_SIDE, PLANE_ICON_SIDE], &rgba)
}

/// A `size` x `size` square centred on `c`, rotated by `track` (degrees clockwise from north,
/// as `RotateImageByAngle`; a track outside 0..=360 draws north up), textured with `tex` and
/// tinted `fill`.
pub fn plane_icon_mesh(tex: TextureId, c: Pos2, size: f32, track: f64, fill: Color32) -> Mesh {
    let track = if (0.0..=360.0).contains(&track) {
        track
    } else {
        0.0
    };
    let (sin, cos) = (track as f32).to_radians().sin_cos();
    let h = size / 2.0;
    let rot = |x: f32, y: f32| c + Vec2::new(x * cos - y * sin, x * sin + y * cos);
    let mut mesh = Mesh::with_texture(tex);
    for (x, y, u, v) in [
        (-h, -h, 0.0, 0.0),
        (h, -h, 1.0, 0.0),
        (h, h, 1.0, 1.0),
        (-h, h, 0.0, 1.0),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: rot(x, y),
            uv: Pos2::new(u, v),
            color: fill,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    mesh
}

/// Locator cells are labelled when at least this wide [px].
const LABEL_MIN_CELL_PX: f32 = 40.0;

/// The overlay switches of the Options Map tab, as the renderer reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayFlags {
    /// `Map_ShowDistances`: distance circles around My.
    pub distances: bool,
    /// `Map_ShowLocators`: the locator grid.
    pub locators: bool,
    /// `Watchlist_Activated`: the watchlist locations.
    pub watchlist: bool,
    /// `Airports_Activate`: the airports layer.
    pub airports: bool,
    /// `Map_ShowBeamWidth`: the antenna beam wedges.
    pub beams: bool,
    /// `Map_SmallMarkers`: small station markers in MULTI and no hot-path marks.
    pub small_markers: bool,
    /// `Map_LabelCalls`: call labels at the checked watchlist locations in MULTI.
    pub label_calls: bool,
}

impl OverlayFlags {
    pub fn from_settings(s: &Settings) -> Self {
        let g = &s.gui;
        Self {
            distances: g.map_show_distances,
            locators: g.map_show_locators,
            watchlist: g.map_show_watchlist,
            airports: g.map_show_airports,
            beams: g.map_show_beam_width,
            small_markers: s.web.map_small_markers,
            label_calls: g.map_label_calls,
        }
    }
}

/// What the last frame of the main map painted, per layer (for tests and diagnostics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawnLayers {
    pub rings: usize,
    pub grid_cells: usize,
    pub beams: usize,
    pub hot_markers: usize,
    pub watch_marks: usize,
    pub watch_labels: usize,
    pub airports: usize,
    /// The user-supplied overlay image was painted.
    pub user_overlay: bool,
}

/// What the cached overlays were computed from.
#[derive(Clone, Debug, PartialEq)]
struct OverlayKey {
    path_gen: u64,
    paths: usize,
    my: (f64, f64),
    max_alt_m: i32,
    show_beam: bool,
    show_distances: bool,
    /// (My, DX) gain of every path.
    gains: Vec<(f64, f64)>,
}

struct GridCache {
    zoom_floor: i32,
    small_letters: bool,
    /// View the cells were computed for (the visible view plus padding).
    covers: GRect,
    cells: Vec<LocatorCell>,
}

#[derive(Default)]
struct Overlays {
    key: Option<OverlayKey>,
    rings: Vec<Vec<GPoint>>,
    paths: Vec<PathOverlay>,
    beams: Vec<Vec<GPoint>>,
    /// Bounding box of My, DX and the paths.
    bbox: Option<GRect>,
    grid: Option<GridCache>,
}

/// State of the map widget that lives between frames.
pub struct MapView {
    memory: MapMemory,
    tiles: Option<StyledSources>,
    /// Tile cache directory; `None` means no tiles (tests).
    tile_dir: Option<PathBuf>,
    provider: TileProvider,
    overlays: Overlays,
    /// `path_gen` the view was last fitted to.
    fitted_for: Option<u64>,
    fit_requested: bool,
    was_auto_center: bool,
    /// The first frame zooms to the configured area.
    initialized: bool,
    /// The plane icon texture, loaded on the first frame.
    plane_tex: Option<TextureHandle>,
    /// Last automatic viewport and marker for the Spectrum map.
    nearest_target: Option<(GPoint, u8)>,
    nearest_plane: Option<PlaneInfo>,
    nearest_scroll_points: f64,
    /// What the last main-map frame painted.
    drawn: DrawnLayers,
    /// The user-supplied overlay of the main map (`gmo_UserSupplied`).
    user_overlay: OverlayLayer,
}

impl MapView {
    /// Map that loads tiles into `<data_dir>/tiles`.
    pub fn new(data_dir: &std::path::Path) -> Self {
        let mut v = Self::without_tiles();
        v.tile_dir = Some(tiles::tiles_cache_dir(data_dir));
        v
    }

    /// Map without a tile layer (no network access; used by tests).
    pub fn without_tiles() -> Self {
        Self {
            memory: MapMemory::default(),
            tiles: None,
            tile_dir: None,
            provider: tiles::provider_by_name("", &Secret::default()),
            overlays: Overlays::default(),
            fitted_for: None,
            fit_requested: false,
            was_auto_center: false,
            initialized: false,
            plane_tex: None,
            nearest_target: None,
            nearest_plane: None,
            nearest_scroll_points: 0.0,
            drawn: DrawnLayers::default(),
            user_overlay: OverlayLayer::default(),
        }
    }

    /// The committed user-supplied overlay of the main map, if any.
    pub fn user_overlay(&self) -> Option<&OverlayImage> {
        self.user_overlay.image()
    }

    /// The user-supplied overlay is being decoded off the UI thread.
    pub fn user_overlay_pending(&self) -> bool {
        self.user_overlay.pending()
    }

    /// What the last main-map frame painted.
    pub fn drawn(&self) -> DrawnLayers {
        self.drawn
    }

    /// Zoom to the path bounding box on the next frame (`SetZoomToFitRect`).
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
    }

    /// Rebuilds the tile layer when the provider or its key changed (Options dialog).
    fn ensure_tiles(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        key: &Secret,
        store: Option<&Arc<TileStore>>,
    ) {
        let Some(dir) = &self.tile_dir else { return };
        let provider = tiles::provider_by_name(name, key);
        if self.tiles.is_none() || provider != self.provider {
            self.tiles = Some(tiles::tile_layer(&provider, store.cloned(), dir, ctx));
            self.provider = provider;
        }
    }

    fn plane_texture(&mut self, ctx: &egui::Context) -> TextureId {
        self.plane_tex
            .get_or_insert_with(|| {
                ctx.load_texture("plane-icon", plane_icon_image(), TextureOptions::LINEAR)
            })
            .id()
    }

    /// The Spectrum map has its own viewport and uses only the routes and nearest marker.
    pub fn show_nearest(&mut self, ui: &mut egui::Ui, state: &AppState) {
        let gui = &state.settings.gui;
        self.ensure_tiles(
            ui.ctx(),
            &gui.map_provider,
            &gui.thunderforest_api_key,
            state.map_store.as_ref(),
        );
        self.update_overlays(state);

        let target = nearest_view(state);
        // A tick can still contain a fix for the previous station after an edit.
        let target_valid = check_lat_lon(state.settings.my_lat, state.settings.my_lon)
            && check_lat_lon(target.0.lat, target.0.lon);
        let reference_position = if target_valid {
            lon_lat(target.0.lon, target.0.lat)
        } else {
            lon_lat(0.0, 0.0)
        };
        let plane = state
            .tick
            .nearest
            .as_ref()
            .filter(|p| target_valid && p.int_point.is_some());
        let new_fix = plane.is_some_and(|p| {
            self.nearest_plane
                .as_ref()
                .is_none_or(|last| (last.hex.as_str(), last.time) != (p.hex.as_str(), p.time))
        });
        // Keep mouse navigation between automatic fixes. The C# worker only replaces
        // the marker and camera when it receives a nearest plane.
        if target_valid
            && (plane.is_some() || self.nearest_plane.is_none())
            && (self.nearest_target != Some(target) || new_fix)
        {
            self.nearest_target = Some(target);
            let _ = self.memory.set_zoom(f64::from(target.1));
            self.memory.center_at(reference_position);
        } else if !target_valid && self.memory.detached().is_none() {
            // No station position or previous viewport. Start at a finite overview.
            let _ = self.memory.set_zoom(6.0);
            self.memory.center_at(reference_position);
        }
        if plane.is_none() && self.nearest_plane.is_some() {
            self.nearest_target = None;
        }
        if let Some(plane) = plane.filter(|p| self.nearest_plane.as_ref() != Some(*p)) {
            self.nearest_plane = Some(plane.clone());
        }

        let plane_tex = self.plane_texture(ui.ctx());
        let rect = ui.max_rect();
        // EmptyTileColor = Navy (MapDlg.Designer.cs:640).
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(0, 0, 128));
        let mut map = Map::new(None, &mut self.memory, reference_position).zoom_gesture(false);
        if let Some(tiles) = self.tiles.as_mut() {
            map = map.with_layer(tiles, 1.0);
        }
        let out = map.show(ui, |ui, response, projector, memory| {
            let frame = Frame {
                painter: ui.painter().clone(),
                projector,
                state,
                overlays: &mut self.overlays,
                plane_tex,
                flags: OverlayFlags::from_settings(&state.settings),
                drawn: DrawnLayers::default(),
            };
            for route in &frame.overlays.paths {
                frame.draw_route(route);
            }
            if let Some(plane) = &self.nearest_plane {
                let pos = frame.pos(GPoint::new(plane.lat, plane.lon));
                let size = plane_icon_px(plane.category, gui.planes_icon_size) as f32;
                let fill = color(rgb(plane_color(plane.potential)));
                frame.plane_icon(pos, size, plane.track, fill);
                if response
                    .hover_pos()
                    .is_some_and(|p| p.distance(pos) <= size / 2.0)
                {
                    let my = GPoint::new(state.settings.my_lat, state.settings.my_lon);
                    let lines =
                        tooltip_lines(plane, &gui.info_win, my, Utc::now(), state.tooltip_track());
                    let tooltip_fill = match plane.potential {
                        100 => Color32::from_rgba_unmultiplied(221, 160, 221, 150),
                        75 | 50 => {
                            Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), 150)
                        }
                        _ => Color32::from_rgba_unmultiplied(255, 255, 255, 180),
                    };
                    paint_tooltip(&frame.painter, rect, pos, &lines, tooltip_fill);
                }
            }
            draw_nearest_scale(&frame.painter, projector, memory.zoom());
            draw_attribution(&frame.painter, rect, self.provider.attribution);
            nearest_wheel_view(
                ui,
                response,
                projector,
                memory.zoom(),
                &mut self.nearest_scroll_points,
            )
        });
        if let Some((center, zoom)) = out.inner {
            self.memory.center_at(center);
            let _ = self.memory.set_zoom(zoom);
            ui.request_repaint();
        }
    }

    /// Draw the map into the remaining space of `ui`.
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let gui = &state.settings.gui;
        self.ensure_tiles(
            &ui.ctx().clone(),
            &gui.map_provider,
            &gui.thunderforest_api_key,
            state.map_store.as_ref(),
        );
        self.update_overlays(state);
        if let Some(e) = self.user_overlay.sync(ui.ctx(), &state.settings.gui) {
            state.status = e;
        }

        // Buttons of the control panel
        for _ in 0..state.pending_zoom.max(0) {
            let _ = self.memory.zoom_in();
        }
        for _ in 0..(-state.pending_zoom).max(0) {
            let _ = self.memory.zoom_out();
        }
        state.pending_zoom = 0;

        let auto_center = state.settings.gui.map_auto_center;
        let my_pos = match state.single_path() {
            Some(e) => e.path.mid_point(),
            None => GPoint::new(state.settings.my_lat, state.settings.my_lon),
        };
        let size = ui.available_size();
        if !std::mem::replace(&mut self.initialized, true) {
            let a = &state.settings.area;
            let (_, zoom) = fit_view(
                GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon),
                size.x,
                size.y,
            );
            let _ = self.memory.set_zoom(zoom);
        }
        if self.take_fit(state) {
            self.apply_fit(size, auto_center);
        } else if auto_center && !self.was_auto_center {
            self.memory.follow_my_position();
        }
        self.was_auto_center = auto_center;

        let plane_tex = self.plane_texture(ui.ctx());
        let Self {
            memory,
            tiles,
            overlays,
            provider,
            drawn,
            user_overlay,
            ..
        } = self;
        let mut map = Map::new(None, memory, lon_lat(my_pos.lon, my_pos.lat));
        if let Some(t) = tiles.as_mut() {
            map = map.with_layer(t, state.settings.gui.map_opacity);
        }
        let out = map.show(ui, |ui, response, projector, _| {
            // After the tile layer, before the vector overlays, within the configured zooms.
            let overlay = user_overlay
                .image()
                .filter(|_| overlay_image::ZOOM_RANGE.contains(&projector.zoom()));
            if let Some(image) = overlay {
                image.draw(ui.painter(), projector, state.settings.gui.overlay_opacity);
            }
            let mut f = Frame {
                painter: ui.painter().clone(),
                projector,
                state,
                overlays,
                plane_tex,
                flags: OverlayFlags::from_settings(&state.settings),
                drawn: DrawnLayers::default(),
            };
            let hit = f.draw(response);
            *drawn = DrawnLayers {
                user_overlay: overlay.is_some(),
                ..f.drawn
            };
            crate::ui::map_track::draw(&f.painter, projector.map_rect(), f.state);
            let dx = GPoint::new(f.state.settings.dx_lat, f.state.settings.dx_lon);
            let dx_click = response.secondary_clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| f.pos(dx).distance(p) <= 5.0 + HIT_SLACK_PX);
            draw_attribution(&f.painter, projector.map_rect(), provider.attribution);
            let fit = ui
                .put(
                    Rect::from_min_size(
                        projector.map_rect().min + Vec2::splat(8.0),
                        Vec2::new(36.0, 22.0),
                    ),
                    egui::Button::new("Fit"),
                )
                .on_hover_text("Zoom to the whole path")
                .clicked();
            (hit, fit, response.clicked(), dx_click)
        });
        let (hit, fit, clicked, dx_click) = out.inner;
        if clicked {
            if let Some(hex) = hit {
                state.select_plane(&hex);
            }
        }
        if dx_click {
            state.right_click_dx();
        }
        if fit {
            self.apply_fit(size, auto_center);
        }
        state.map_zoom = self.memory.zoom();
    }

    /// True once when a new path arrived or Fit was asked for.
    fn take_fit(&mut self, state: &AppState) -> bool {
        let mut fit = std::mem::take(&mut self.fit_requested);
        if fit_due(
            state.path_mode(),
            state.paths.len(),
            state.paths_done(),
            self.fitted_for,
            state.path_gen,
        ) {
            self.fitted_for = Some(state.path_gen);
            fit = true;
        }
        fit
    }

    fn apply_fit(&mut self, size: Vec2, auto_center: bool) {
        let Some(bbox) = self.overlays.bbox else {
            return;
        };
        let (centre, zoom) = fit_view(bbox, size.x, size.y);
        let _ = self
            .memory
            .set_zoom(zoom.min(f64::from(self.provider.max_zoom)));
        if auto_center {
            self.memory.follow_my_position();
        } else {
            self.memory.center_at(lon_lat(centre.lon, centre.lat));
        }
    }

    /// Recompute the cached geometry if its inputs changed.
    fn update_overlays(&mut self, state: &AppState) {
        let s = &state.settings;
        let key = OverlayKey {
            path_gen: state.path_gen,
            paths: state.paths.len(),
            my: (s.my_lat, s.my_lon),
            max_alt_m: s.planes_max_alt_m,
            show_beam: s.gui.map_show_beam_width,
            show_distances: s.gui.map_show_distances,
            gains: state
                .paths
                .iter()
                .map(|e| (e.my_gain_dbi, e.dx_gain_dbi))
                .collect(),
        };
        let o = &mut self.overlays;
        if o.key.as_ref() == Some(&key) {
            return;
        }
        // `CreateDistances` only while `Map_ShowDistances` (MapDlg.cs:4008).
        o.rings = if s.gui.map_show_distances {
            overlays::distance_rings(s.my_lat, s.my_lon)
        } else {
            Vec::new()
        };
        let max_alt = f64::from(s.planes_max_alt_m);
        o.paths = state
            .paths
            .iter()
            .map(|e| overlays::path_overlay(&e.path, max_alt))
            .collect();
        o.beams = if s.gui.map_show_beam_width {
            state.paths.iter().flat_map(beam_wedges).collect()
        } else {
            Vec::new()
        };
        o.bbox = paths_bbox(&state.paths, &o.paths);
        o.key = Some(key);
    }
}

/// Starting viewport of the Spectrum map and the target for a new nearest-plane fix.
pub fn nearest_view(state: &AppState) -> (GPoint, u8) {
    state
        .tick
        .nearest
        .as_ref()
        .and_then(|p| p.int_point)
        .map_or_else(
            || (GPoint::new(state.settings.my_lat, state.settings.my_lon), 6),
            |point| (point, 10),
        )
}

/// GMap.NET's MousePositionAndCenter zoom is one level per wheel step.
fn nearest_wheel_view(
    ui: &egui::Ui,
    response: &egui::Response,
    projector: &Projector<'_>,
    zoom: f64,
    scroll_points: &mut f64,
) -> Option<(walkers::Position, f64)> {
    const POINTS_PER_STEP: f64 = 50.0;
    let pointer = response.hover_pos().filter(|_| !response.dragged())?;
    let steps = ui.input_mut(|input| {
        let mut steps = 0.0;
        for event in &input.events {
            let egui::Event::MouseWheel { unit, delta, .. } = event else {
                continue;
            };
            let y = f64::from(delta.y);
            if y == 0.0 {
                continue;
            }
            match unit {
                egui::MouseWheelUnit::Point => {
                    *scroll_points += y;
                    steps += (*scroll_points / POINTS_PER_STEP).trunc();
                    *scroll_points %= POINTS_PER_STEP;
                }
                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                    steps += y.signum();
                }
            }
        }
        // Consume vertical scrolling over the map, including egui's smoothed tail.
        if input.smooth_scroll_delta.y != 0.0 {
            input.smooth_scroll_delta.y = 0.0;
        }
        steps
    });
    let next = (zoom + steps).clamp(0.0, 20.0);
    (next != zoom).then(|| (projector.unproject(pointer.to_vec2()), next))
}

/// Ground distances from GMapControl's scale, in metres at the current Mercator latitude.
fn draw_nearest_scale(painter: &Painter, projector: &Projector<'_>, zoom: f64) {
    let rect = projector.map_rect();
    let lat = projector.unproject(rect.center().to_vec2()).y();
    let meters_per_pixel =
        std::f64::consts::TAU * 6_378_137.0 * lat.to_radians().cos() / (256.0 * zoom.exp2());
    let start = rect.min + Vec2::splat(10.0);
    let stroke = Stroke::new(3.0, Color32::BLACK);
    for (meters, min_zoom, label, offset) in [
        (5_000_000.0, 0.0, "5000Km", 40.0),
        (1_000_000.0, 0.0, "1000Km", 40.0),
        (100_000.0, 6.0, "100Km", 35.0),
        (10_000.0, 9.0, "10Km", 30.0),
        (1_000.0, 12.0, "1km", 25.0),
        (100.0, 15.0, "100m", 30.0),
    ] {
        let width = (meters / meters_per_pixel) as f32;
        if zoom >= min_zoom && width.trunc() >= 1.0 && width.trunc() < rect.width() {
            let end = start + Vec2::new(width.trunc(), 0.0);
            painter.add(Shape::line(
                vec![
                    start + Vec2::new(0.0, 10.0),
                    start,
                    end,
                    end + Vec2::new(0.0, 10.0),
                ],
                stroke,
            ));
            painter.text(
                egui::pos2(end.x - 10.0 - offset, rect.top() + 13.0),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(10.0),
                Color32::BLACK,
            );
        }
    }
}

/// Whether the map fits the view to the paths now: SINGLE on the first entry, MULTI once the
/// whole request is done (bbox of all entries); once per `path_gen`.
pub fn fit_due(
    mode: as_aircraft::enums::PathMode,
    entries: usize,
    paths_done: bool,
    fitted_for: Option<u64>,
    path_gen: u64,
) -> bool {
    let ready = match mode {
        as_aircraft::enums::PathMode::Multi => paths_done,
        _ => true,
    };
    ready && entries > 0 && fitted_for != Some(path_gen)
}

fn color(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Everything one frame needs to paint.
struct Frame<'a> {
    painter: Painter,
    projector: &'a Projector<'a>,
    state: &'a AppState,
    overlays: &'a mut Overlays,
    plane_tex: TextureId,
    flags: OverlayFlags,
    drawn: DrawnLayers,
}

impl Frame<'_> {
    fn pos(&self, p: GPoint) -> Pos2 {
        self.projector.project(lon_lat(p.lon, p.lat)).to_pos2()
    }

    fn line(&self, pts: &[GPoint], stroke: Stroke) {
        let v: Vec<Pos2> = pts.iter().map(|&p| self.pos(p)).collect();
        self.painter.add(Shape::line(v, stroke));
    }

    fn marker(&self, p: GPoint, r: f32, fill: Color32) {
        self.painter
            .circle(self.pos(p), r, fill, Stroke::new(1.0, Color32::BLACK));
    }

    /// Paint everything; returns the hex of the plane under the pointer.
    fn draw(&mut self, response: &egui::Response) -> Option<String> {
        if self.flags.distances {
            self.draw_rings();
        }
        if self.flags.locators {
            self.draw_grid();
        }
        let pointer = response.hover_pos();
        if self.flags.airports {
            self.draw_airports(pointer);
        }
        if self.flags.watchlist {
            self.draw_watchlist(pointer);
        }
        self.draw_path();
        self.draw_planes(response)
    }

    fn draw_rings(&mut self) {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 110, 110, 160));
        for ring in &self.overlays.rings {
            self.line(ring, stroke);
        }
        self.drawn.rings = self.overlays.rings.len();
    }

    /// Airports (`UpdateAirports`, `MapDlg.cs:2929-2950`): a symbol per airport, the name and
    /// IATA/ICAO codes on hover.
    fn draw_airports(&mut self, pointer: Option<Pos2>) {
        let rect = self.projector.map_rect();
        let fill = Color32::from_rgb(70, 70, 70);
        let mut hover: Option<(&AirportMark, Pos2)> = None;
        for a in &self.state.airports {
            let c = self.pos(a.pos);
            if !rect.contains(c) {
                continue;
            }
            let r = 4.0;
            self.painter.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -r),
                    c + Vec2::new(r, 0.0),
                    c + Vec2::new(0.0, r),
                    c + Vec2::new(-r, 0.0),
                ],
                fill,
                Stroke::new(1.0, Color32::WHITE),
            ));
            self.drawn.airports += 1;
            if pointer.is_some_and(|p| p.distance(c) <= r + HIT_SLACK_PX) {
                hover = Some((a, c));
            }
        }
        if let Some((a, c)) = hover {
            let lines = [a.name.clone(), format!("{}/{}", a.iata, a.icao)];
            draw_tooltip(&self.painter, rect, c, &lines);
        }
    }

    /// Watchlist locations (`UpdateWatchlistInMap`, `MapDlg.cs:5235-5259`): green for a
    /// user-set position, else white; the call on hover, or always for a checked item in MULTI
    /// with `Map_LabelCalls`.
    fn draw_watchlist(&mut self, pointer: Option<Pos2>) {
        let rect = self.projector.map_rect();
        let multi = self.state.path_mode() == as_aircraft::enums::PathMode::Multi;
        let mut hover: Option<(&WatchMark, Pos2)> = None;
        for m in &self.state.watch_marks {
            let c = self.pos(m.pos);
            if !rect.contains(c) {
                continue;
            }
            let fill = if m.from_user {
                Color32::from_rgb(0, 160, 0)
            } else {
                Color32::WHITE
            };
            self.painter
                .circle(c, 3.0, fill, Stroke::new(1.0, Color32::BLACK));
            self.drawn.watch_marks += 1;
            if multi && self.flags.label_calls && m.checked {
                label_box(&self.painter, c, &m.call);
                self.drawn.watch_labels += 1;
            } else if pointer.is_some_and(|p| p.distance(c) <= 3.0 + HIT_SLACK_PX) {
                hover = Some((m, c));
            }
        }
        if let Some((m, c)) = hover {
            draw_tooltip(&self.painter, rect, c, std::slice::from_ref(&m.call));
        }
    }

    fn draw_grid(&mut self) {
        let gui = &self.state.settings.gui;
        let rect = self.projector.map_rect();
        let zoom = self.projector.zoom();
        let view = self.view_rect();
        let zoom_floor = zoom.floor() as i32;
        let stale = match &self.overlays.grid {
            Some(g) => {
                g.zoom_floor != zoom_floor
                    || g.small_letters != gui.locator_small_letters_for_subsquares
                    || !contains(&g.covers, &view)
            }
            None => true,
        };
        if stale {
            let covers = pad(&view);
            self.overlays.grid = Some(GridCache {
                zoom_floor,
                small_letters: gui.locator_small_letters_for_subsquares,
                cells: overlays::locator_grid(
                    covers,
                    zoom,
                    gui.locator_small_letters_for_subsquares,
                ),
                covers,
            });
        }
        let Some(grid) = &self.overlays.grid else {
            return;
        };
        let stroke = Stroke::new(0.5, Color32::from_rgba_unmultiplied(80, 80, 160, 120));
        let text = Color32::from_rgba_unmultiplied(60, 60, 140, 200);
        let mut painted = 0;
        for c in &grid.cells {
            let a = self.pos(GPoint::new(c.rect.max_lat, c.rect.min_lon));
            let b = self.pos(GPoint::new(c.rect.min_lat, c.rect.max_lon));
            let r = Rect::from_two_pos(a, b);
            if !r.intersects(rect) {
                continue;
            }
            painted += 1;
            self.painter.rect_stroke(r, 0.0, stroke, StrokeKind::Middle);
            if r.width() >= LABEL_MIN_CELL_PX {
                self.painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &c.loc,
                    FontId::proportional(11.0),
                    text,
                );
            }
        }
        self.drawn.grid_cells = painted;
    }

    /// Visible part of the map in degrees.
    fn view_rect(&self) -> GRect {
        let r = self.projector.map_rect();
        let tl = self.projector.unproject(r.left_top().to_vec2());
        let br = self.projector.unproject(r.right_bottom().to_vec2());
        GRect::new(
            br.y().clamp(-90.0, 90.0),
            tl.x().clamp(-180.0, 180.0),
            tl.y().clamp(-90.0, 90.0),
            br.x().clamp(-180.0, 180.0),
        )
    }

    fn draw_route(&self, ov: &PathOverlay) {
        let full = if ov.valid {
            Color32::BLACK
        } else {
            Color32::RED
        };
        for l in &ov.full {
            self.line(l, Stroke::new(3.0, full));
        }
        for l in &ov.visible {
            self.line(l, Stroke::new(3.0, Color32::from_rgb(255, 0, 255)));
        }
    }

    /// Beams, routes and the station marks (`MapDlg.cs:4125-4180`, `:4375`): small marks in
    /// MULTI with `Map_SmallMarkers`, and the three hot-path marks only without it.
    fn draw_path(&mut self) {
        let s = &self.state.settings;
        let beam = Color32::from_rgba_unmultiplied(128, 128, 128, 50);
        for wedge in &self.overlays.beams {
            let v: Vec<Pos2> = wedge.iter().map(|&p| self.pos(p)).collect();
            self.painter
                .add(Shape::convex_polygon(v, beam, Stroke::NONE));
        }
        self.drawn.beams = self.overlays.beams.len();
        let small = self.flags.small_markers
            && self.state.path_mode() == as_aircraft::enums::PathMode::Multi;
        let (station_r, mid_r) = if small { (3.0, 3.0) } else { (5.0, 4.0) };
        let mut hot_count = 0;
        for ov in &self.overlays.paths {
            self.draw_route(ov);
            if !self.flags.small_markers {
                let hot = [Color32::RED, Color32::BLUE, Color32::YELLOW];
                for (p, c) in ov.hot_markers.iter().zip(hot) {
                    if let Some(p) = p {
                        self.marker(*p, 3.0, c);
                        hot_count += 1;
                    }
                }
            }
            self.marker(ov.mid, mid_r, Color32::BLUE);
        }
        self.drawn.hot_markers = hot_count;
        self.marker(GPoint::new(s.my_lat, s.my_lon), station_r, Color32::RED);
        self.marker(GPoint::new(s.dx_lat, s.dx_lon), station_r, Color32::YELLOW);
    }

    fn draw_planes(&self, response: &egui::Response) -> Option<String> {
        let st = self.state;
        let gui = &st.settings.gui;
        let pointer = response.hover_pos();
        let rect = self.projector.map_rect();
        let mut selected: Option<(&PlaneInfo, Pos2)> = None;
        for p in &st.tick.shown {
            let c = self.pos(GPoint::new(p.lat, p.lon));
            if !rect.expand(30.0).contains(c) {
                continue;
            }
            let size = plane_icon_px(p.category, gui.planes_icon_size) as f32;
            self.plane_icon(c, size, p.track, color(rgb(plane_color(p.potential))));
            if st.selected.as_deref() == Some(p.hex.as_str()) {
                selected = Some((p, c));
            }
        }
        let planes_pos = |p: &PlaneInfo| self.pos(GPoint::new(p.lat, p.lon));
        let icon = |p: &PlaneInfo| plane_icon_px(p.category, gui.planes_icon_size) as f32;
        let hover = pointer
            .and_then(|ptr| hit_plane(&st.tick.shown, planes_pos, icon, ptr))
            .map(|p| (p, planes_pos(p)));
        let now = Utc::now();
        let track = st.tooltip_track();
        let my = GPoint::new(st.settings.my_lat, st.settings.my_lon);
        if let Some((p, c)) = selected {
            if let Some(ip) = p.int_point {
                self.painter
                    .line_segment([c, self.pos(ip)], Stroke::new(1.0, Color32::BLACK));
            }
            let lines = tooltip_lines(p, &gui.info_win, my, now, track);
            draw_tooltip(&self.painter, rect, c, &lines);
        }
        if let Some((p, c)) = hover {
            if st.selected.as_deref() != Some(p.hex.as_str()) {
                let lines = tooltip_lines(p, &gui.info_win, my, now, track);
                draw_tooltip(&self.painter, rect, c, &lines);
            }
            return Some(p.hex.clone());
        }
        None
    }

    /// The plane icon pointing along `track` (degrees clockwise from north).
    fn plane_icon(&self, c: Pos2, size: f32, track: f64, fill: Color32) {
        self.painter.add(Shape::mesh(plane_icon_mesh(
            self.plane_tex,
            c,
            size,
            track,
            fill,
        )));
    }
}

/// A permanent call label next to a location (`MarkerTooltipMode.Always`).
fn label_box(painter: &Painter, at: Pos2, text: &str) {
    let galley =
        painter.layout_no_wrap(text.to_string(), FontId::proportional(10.0), Color32::BLACK);
    let r = Rect::from_min_size(at + Vec2::new(6.0, -galley.size().y / 2.0), galley.size());
    painter.rect(
        r.expand(2.0),
        2.0,
        Color32::from_rgba_unmultiplied(255, 255, 225, 230),
        Stroke::new(1.0, Color32::BLACK),
        StrokeKind::Outside,
    );
    painter.galley(r.min, galley, Color32::BLACK);
}

fn draw_tooltip(painter: &Painter, bounds: Rect, anchor: Pos2, lines: &[String]) {
    paint_tooltip(
        painter,
        bounds,
        anchor,
        lines,
        Color32::from_rgba_unmultiplied(255, 255, 225, 240),
    );
}

fn paint_tooltip(painter: &Painter, bounds: Rect, anchor: Pos2, lines: &[String], fill: Color32) {
    let galley = painter.layout_no_wrap(lines.join("\n"), FontId::monospace(11.0), Color32::BLACK);
    let size = galley.size() + Vec2::splat(8.0);
    let mut min = anchor + Vec2::new(14.0, 14.0);
    if min.x + size.x > bounds.right() {
        min.x = anchor.x - 14.0 - size.x;
    }
    if min.y + size.y > bounds.bottom() {
        min.y = anchor.y - 14.0 - size.y;
    }
    let r = Rect::from_min_size(min, size);
    painter.rect(
        r,
        3.0,
        fill,
        Stroke::new(1.0, Color32::BLACK),
        StrokeKind::Inside,
    );
    painter.galley(r.min + Vec2::splat(4.0), galley, Color32::BLACK);
}

fn draw_attribution(painter: &Painter, rect: Rect, text: &str) {
    let galley =
        painter.layout_no_wrap(text.to_string(), FontId::proportional(10.0), Color32::BLACK);
    let r = Rect::from_min_size(
        rect.right_bottom() - galley.size() - Vec2::splat(4.0),
        galley.size(),
    );
    painter.rect_filled(
        r.expand(2.0),
        2.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 190),
    );
    painter.galley(r.min, galley, Color32::BLACK);
}
