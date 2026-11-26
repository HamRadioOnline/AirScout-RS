//! Coverage view of one elevation model (`OptionsDlg.cs:1356-1461`, the maps on the GLOBE, SRTM
//! and ASTER tabs): the tiles of the server's catalogue on a map, green where the database has
//! them and red where it has not, inside the magenta area rectangle.
//!
//! The view opens from "Show coverage" and asks the app for the job; closing it cancels the
//! job. (The C# runs on tab Enter and also zooms the General tab map by mistake.)

use crate::coverage::{CoverState, CoverageData};
use crate::tiles::{self, TileProvider};
use crate::ui::map_geom::fit_view;
use as_elevation::ElevationModel;
use as_geo::latlon::{GPoint, GRect};
use as_maps::TileStore;
use as_settings::Settings;
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, StrokeKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkers::{lon_lat, Map, MapMemory, Projector, StyledSources};

const MAP_SIZE: (f32, f32) = (560.0, 340.0);

/// The area rectangle: magenta, width 3 (`:1371-1374`).
const AREA: Color32 = Color32::from_rgb(255, 0, 255);

/// Fill and stroke of a tile: green or red at alpha 50 (`:1427-1456`); mixed coarse cells amber.
fn tile_colour(s: CoverState) -> Color32 {
    match s {
        CoverState::Found => Color32::from_rgba_unmultiplied(0, 128, 0, 50),
        CoverState::Missing => Color32::from_rgba_unmultiplied(255, 0, 0, 50),
        CoverState::Mixed => Color32::from_rgba_unmultiplied(255, 165, 0, 50),
    }
}

pub struct CoverageView {
    pub model: ElevationModel,
    area: GRect,
    data: Option<CoverageData>,
    status: String,
    memory: MapMemory,
    tiles: Option<StyledSources>,
    provider: Option<TileProvider>,
}

impl CoverageView {
    pub fn new(model: ElevationModel, area: GRect) -> Self {
        let mut memory = MapMemory::default();
        let (centre, zoom) = fit_view(
            GRect::new(
                area.min_lat - 1.0,
                area.min_lon - 1.0,
                area.max_lat + 1.0,
                area.max_lon + 1.0,
            ),
            MAP_SIZE.0,
            MAP_SIZE.1,
        );
        let _ = memory.set_zoom(zoom);
        memory.center_at(lon_lat(centre.lon, centre.lat));
        Self {
            model,
            area,
            data: None,
            status: format!("{}: Creating elevation tile catalogue...", model.dir_name()),
            memory,
            tiles: None,
            provider: None,
        }
    }

    /// The job's answer.
    pub fn set_result(&mut self, found: &[String], missing: &[String]) {
        let d = CoverageData::new(found, missing);
        self.status = format!("{}: {}", self.model.dir_name(), d.status());
        self.data = Some(d);
    }

    pub fn fail(&mut self, text: &str) {
        self.status = format!("error: {text}");
    }

    fn ensure_tiles(
        &mut self,
        ctx: &egui::Context,
        s: &Settings,
        dir: &Path,
        store: Option<&Arc<TileStore>>,
    ) {
        let provider = tiles::provider_by_name(&s.gui.map_provider, &s.gui.thunderforest_api_key);
        if self.tiles.is_none() || self.provider.as_ref() != Some(&provider) {
            self.tiles = Some(tiles::tile_layer(&provider, store.cloned(), dir, ctx));
            self.provider = Some(provider);
        }
    }

    /// Draw the view as a modal; true once it was closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        settings: &Settings,
        tile_dir: Option<&PathBuf>,
        map_store: Option<&Arc<TileStore>>,
    ) -> bool {
        if let Some(dir) = tile_dir {
            self.ensure_tiles(ctx, settings, dir, map_store);
        }
        let mut close = false;
        let r = egui::Modal::new(egui::Id::unique("coverage_view")).show(ctx, |ui| {
            ui.set_width(MAP_SIZE.0);
            ui.heading(format!("{} elevation coverage", self.model.dir_name()));
            self.map(ui, settings);
            ui.label(&self.status);
            close = ui.button("Close").clicked();
        });
        close || r.should_close()
    }

    fn map(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        let Self {
            area,
            data,
            memory,
            tiles,
            ..
        } = self;
        let mut map = Map::new(None, memory, lon_lat(area.min_lon, area.min_lat));
        if let Some(t) = tiles.as_mut() {
            map = map.with_layer(t, settings.gui.map_opacity);
        }
        ui.allocate_ui(egui::Vec2::new(MAP_SIZE.0, MAP_SIZE.1), |ui| {
            map.show(ui, |ui, _response, projector, _| {
                paint(ui, projector, *area, data.as_ref());
            })
        });
    }
}

fn paint(ui: &egui::Ui, projector: &Projector, area: GRect, data: Option<&CoverageData>) {
    let pos = |lat: f64, lon: f64| -> Pos2 { projector.project(lon_lat(lon, lat)).to_pos2() };
    let painter = ui.painter();
    let clip = projector.map_rect();
    if let Some(d) = data {
        let tl = projector.unproject(clip.left_top().to_vec2());
        let br = projector.unproject(clip.right_bottom().to_vec2());
        let view = GRect::new(br.y(), tl.x(), tl.y(), br.x());
        for (r, state) in d.visible(view) {
            let rect = Rect::from_two_pos(pos(r.max_lat, r.min_lon), pos(r.min_lat, r.max_lon));
            let c = tile_colour(state);
            painter.rect_filled(rect, 0.0, c);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, c), StrokeKind::Middle);
        }
    }
    let corners = [
        GPoint::new(area.min_lat, area.min_lon),
        GPoint::new(area.min_lat, area.max_lon),
        GPoint::new(area.max_lat, area.max_lon),
        GPoint::new(area.max_lat, area.min_lon),
    ];
    painter.add(Shape::closed_line(
        corners.iter().map(|p| pos(p.lat, p.lon)).collect(),
        Stroke::new(3.0, AREA),
    ));
}
