//! Central map: OSM tiles, distance rings, locator grid, path, beam wedges and live planes.
//!
//! The overlay geometry (`crate::overlays`) is computed once per path / settings change and kept
//! in [`MapView`]; a frame only projects the cached points and paints them. Planes are the
//! upstream plane icon, recoloured by potential and rotated along their track.

use crate::app::AppState;
use crate::overlays::{self, LocatorCell, PathOverlay};
use crate::style::{plane_color, plane_icon_px, rgb, tooltip_lines};
use crate::tiles::{self, TileProvider};
pub use crate::ui::map_geom::{bbox_of, fit_view};
use crate::ui::map_geom::{beam_wedges, contains, hit_plane, pad, paths_bbox, HIT_SLACK_PX};
use as_aircraft::PlaneInfo;
use as_geo::latlon::{GPoint, GRect};
use as_maps::TileStore;
use as_settings::Secret;
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

/// What the cached overlays were computed from.
#[derive(Clone, Debug, PartialEq)]
struct OverlayKey {
    path_gen: u64,
    paths: usize,
    my: (f64, f64),
    max_alt_m: i32,
    show_beam: bool,
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
        }
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

        let Self {
            memory,
            tiles,
            overlays,
            provider,
            plane_tex,
            ..
        } = self;
        let plane_tex = plane_tex
            .get_or_insert_with(|| {
                ui.ctx()
                    .load_texture("plane-icon", plane_icon_image(), TextureOptions::LINEAR)
            })
            .id();
        let mut map = Map::new(None, memory, lon_lat(my_pos.lon, my_pos.lat));
        if let Some(t) = tiles.as_mut() {
            map = map.with_layer(t, state.settings.gui.map_opacity);
        }
        let out = map.show(ui, |ui, response, projector, _| {
            let mut f = Frame {
                painter: ui.painter().clone(),
                projector,
                state,
                overlays,
                plane_tex,
            };
            let hit = f.draw(response);
            // below the Fit button
            crate::ui::map_track::draw(
                &f.painter,
                projector.map_rect().min + Vec2::new(8.0, 38.0),
                f.state,
            );
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
        o.rings = overlays::distance_rings(s.my_lat, s.my_lon);
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
        self.draw_rings();
        self.draw_grid();
        self.draw_path();
        self.draw_planes(response)
    }

    fn draw_rings(&self) {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 110, 110, 160));
        for ring in &self.overlays.rings {
            self.line(ring, stroke);
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
        for c in &grid.cells {
            let a = self.pos(GPoint::new(c.rect.max_lat, c.rect.min_lon));
            let b = self.pos(GPoint::new(c.rect.min_lat, c.rect.max_lon));
            let r = Rect::from_two_pos(a, b);
            if !r.intersects(rect) {
                continue;
            }
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

    fn draw_path(&self) {
        let s = &self.state.settings;
        let beam = Color32::from_rgba_unmultiplied(128, 128, 128, 50);
        for wedge in &self.overlays.beams {
            let v: Vec<Pos2> = wedge.iter().map(|&p| self.pos(p)).collect();
            self.painter
                .add(Shape::convex_polygon(v, beam, Stroke::NONE));
        }
        for ov in &self.overlays.paths {
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
            let hot = [Color32::RED, Color32::BLUE, Color32::YELLOW];
            for (p, c) in ov.hot_markers.iter().zip(hot) {
                if let Some(p) = p {
                    self.marker(*p, 3.0, c);
                }
            }
            self.marker(ov.mid, 4.0, Color32::BLUE);
        }
        self.marker(GPoint::new(s.my_lat, s.my_lon), 5.0, Color32::RED);
        self.marker(GPoint::new(s.dx_lat, s.dx_lon), 5.0, Color32::YELLOW);
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
            if gui.map_label_calls {
                self.painter.text(
                    c + Vec2::new(size / 2.0 + 2.0, 0.0),
                    Align2::LEFT_CENTER,
                    &p.call,
                    FontId::proportional(10.0),
                    Color32::BLACK,
                );
            }
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

fn draw_tooltip(painter: &Painter, bounds: Rect, anchor: Pos2, lines: &[String]) {
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
        Color32::from_rgba_unmultiplied(255, 255, 225, 240),
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
