//! General tab: the area of interest, laid out as the Designer does it — the four
//! `tb_Coverage_*` bounds inside `groupBox25` ("Covered Area") and the
//! `gm_Options_Coverage` map that draws them as a magenta rectangle over the
//! locally cached tiles of the active provider (Decision 2).

use super::coverage::area_rect;
use super::fields::covered_area;
use crate::tiles::{self, TileProvider};
use crate::ui::forms::geometry::designer_rect;
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets;
use crate::ui::map_geom::fit_view;
use as_geo::latlon::GRect;
use as_maps::TileStore;
use as_settings::Settings;
use eframe::egui::{self, Rect, Ui};
use std::ops::RangeInclusive;
use std::sync::Arc;
use walkers::{lon_lat, Map, MapMemory, StyledSources};

/// The coverage map: a blank, navigable basemap drawn from the active provider's
/// locally cached tiles, with the magenta area rectangle on top.
#[derive(Default)]
pub(super) struct AreaPreview {
    memory: MapMemory,
    tiles: Option<StyledSources>,
    provider: Option<TileProvider>,
    /// The last valid area the rectangle was drawn for and the camera was fitted to.
    area: Option<GRect>,
    /// Force the next [`show`] to refit the camera even when `area` is unchanged
    /// (set by [`on_enter`] when the General tab is re-entered).
    refit: bool,
}

impl AreaPreview {
    /// Draw the map at its Designer rect (`gm_Options_Coverage`, `[9, 28]` ×
    /// `[476, 420]` within `groupBox25`) and refit the camera to the one-degree
    /// padded extent on entry and on valid bound changes. Invalid drafts draw no
    /// rectangle; unchanged frames leave the camera alone. `store` is the local
    /// tile store; without one the basemap stays blank and nothing is downloaded.
    pub(super) fn show(&mut self, ui: &mut Ui, s: &Settings, store: Option<&Arc<TileStore>>) {
        let provider = tiles::provider_by_name(&s.gui.map_provider, &s.gui.thunderforest_api_key);
        if self.tiles.is_none() || self.provider.as_ref() != Some(&provider) {
            self.tiles = store.map(|st| tiles::cached_tile_layer(&provider, st.clone(), ui.ctx()));
            self.provider = Some(provider);
        }

        let map_rect = designer_rect(ui.max_rect(), [9, 28], [476, 420]);

        // Refit on entry (forced by `on_enter`) or when the (valid) bounds
        // actually changed; unchanged frames leave the camera alone.
        let current = covered_area(&s.area);
        if self.refit || current != self.area {
            self.refit = false;
            self.area = current;
            if let Some(r) = current {
                let padded = padded_extent(r);
                let (centre, zoom) = fit_view(padded, map_rect.width(), map_rect.height());
                let _ = self.memory.set_zoom(zoom);
                self.memory.center_at(lon_lat(centre.lon, centre.lat));
            }
        }

        trace::scope(ui, "gm_Options_Coverage", map_rect, |ui| {
            let Self {
                area,
                memory,
                tiles,
                ..
            } = self;
            let my_position = match *area {
                Some(r) => lon_lat((r.min_lon + r.max_lon) / 2.0, (r.min_lat + r.max_lat) / 2.0),
                None => lon_lat(0.0, 0.0),
            };
            let mut map = Map::new(None, memory, my_position);
            if let Some(t) = tiles.as_mut() {
                map = map.with_layer(t, s.gui.map_opacity);
            }
            widgets::scoped(ui, map_rect, |ui| {
                map.show(ui, |ui, _response, projector, _memory| {
                    if let Some(r) = *area {
                        area_rect(ui.painter(), projector, r);
                    }
                });
            });
        });
    }

    /// Force the next [`show`] to refit the camera even when the area is
    /// unchanged: the user may have panned or zoomed away while on another tab.
    pub(super) fn on_enter(&mut self) {
        self.refit = true;
    }

    /// The camera's detached centre as `(lon, lat)` and its zoom. Test hook for
    /// the re-entry regression; production reads the same state through the map.
    #[doc(hidden)]
    pub(super) fn camera(&self) -> (Option<(f64, f64)>, f64) {
        (
            self.memory.detached().map(|p| (p.x(), p.y())),
            self.memory.zoom(),
        )
    }

    /// Point the camera at `(lon, lat)` and `zoom`, away from the fitted view.
    /// Test hook for the re-entry regression.
    #[doc(hidden)]
    pub(super) fn move_camera(&mut self, lon: f64, lat: f64, zoom: f64) {
        self.memory.center_at(lon_lat(lon, lat));
        let _ = self.memory.set_zoom(zoom);
    }
}

/// The extent the C# fits the map to: the area padded by one degree, clamped to
/// geographic bounds (±90° latitude, ±180° longitude). The latitude is then
/// clamped to the Web Mercator range by [`fit_view`] (`map_geom::merc_y`), so
/// the fit stays safe near the poles (`OptionsDlg.cs:272`).
fn padded_extent(r: GRect) -> GRect {
    GRect::new(
        (r.min_lat - 1.0).clamp(-90.0, 90.0),
        (r.min_lon - 1.0).clamp(-180.0, 180.0),
        (r.max_lat + 1.0).clamp(-90.0, 90.0),
        (r.max_lon + 1.0).clamp(-180.0, 180.0),
    )
}

/// A `tb_Coverage_*` number box placed at `rect`, recorded in the trace and
/// labelled by `label` for egui_kittest.
fn coverage_box(
    ui: &mut Ui,
    name: &str,
    value: &mut f64,
    range: RangeInclusive<f64>,
    rect: Rect,
    label: egui::Response,
) {
    trace::record(ui, name, Kind::Text, "", rect);
    // Clamp only what the user types: a stored value outside the range is shown, not rewritten.
    let response = ui.put(
        rect,
        egui::DragValue::new(value)
            .range(range)
            .speed(1.0)
            .clamp_existing_to_range(false),
    );
    response.labelled_by(label.id);
}

pub fn show(
    ui: &mut Ui,
    s: &mut Settings,
    preview: &mut AreaPreview,
    store: Option<&Arc<TileStore>>,
) {
    let body = ui.max_rect();
    let group = designer_rect(body, [6, 6], [659, 466]);
    widgets::group_box(ui, "groupBox25", "Covered Area", group, |ui| {
        let a = &mut s.area;
        let lat_range = -90.0..=90.0;
        let lon_range = -180.0..=180.0;

        let l_max_lat = widgets::label(
            ui,
            "label35",
            "Max. Latitude:",
            designer_rect(group, [503, 421], [74, 13]),
        );
        let l_min_lat = widgets::label(
            ui,
            "label54",
            "Min. Latitude:",
            designer_rect(group, [503, 395], [71, 13]),
        );
        let l_max_lon = widgets::label(
            ui,
            "label59",
            "Max. Longitude:",
            designer_rect(group, [503, 368], [83, 13]),
        );
        let l_min_lon = widgets::label(
            ui,
            "label60",
            "Min. Longitude:",
            designer_rect(group, [503, 342], [80, 13]),
        );

        coverage_box(
            ui,
            "tb_Coverage_MaxLat",
            &mut a.max_lat,
            lat_range.clone(),
            designer_rect(group, [589, 416], [50, 22]),
            l_max_lat,
        );
        coverage_box(
            ui,
            "tb_Coverage_MinLat",
            &mut a.min_lat,
            lat_range.clone(),
            designer_rect(group, [589, 391], [50, 22]),
            l_min_lat,
        );
        coverage_box(
            ui,
            "tb_Coverage_MaxLon",
            &mut a.max_lon,
            lon_range.clone(),
            designer_rect(group, [589, 364], [50, 22]),
            l_max_lon,
        );
        coverage_box(
            ui,
            "tb_Coverage_MinLon",
            &mut a.min_lon,
            lon_range.clone(),
            designer_rect(group, [589, 337], [50, 22]),
            l_min_lon,
        );

        preview.show(ui, s, store);
    });
}

#[cfg(test)]
mod tests {
    use super::super::fields::covered_area;
    use as_settings::Area;

    #[test]
    fn covered_area_yields_exact_bounds() {
        let a = Area {
            min_lat: 50.0,
            max_lat: 51.0,
            min_lon: 10.0,
            max_lon: 11.0,
        };
        let r = covered_area(&a).expect("valid area");
        assert_eq!(r.min_lat, 50.0);
        assert_eq!(r.max_lat, 51.0);
        assert_eq!(r.min_lon, 10.0);
        assert_eq!(r.max_lon, 11.0);
    }

    #[test]
    fn covered_area_rejects_reversed_latitudes() {
        let a = Area {
            min_lat: 51.0,
            max_lat: 50.0,
            min_lon: 10.0,
            max_lon: 11.0,
        };
        assert!(covered_area(&a).is_none());
    }
}
