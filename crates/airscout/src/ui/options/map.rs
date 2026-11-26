//! Map tab: tile provider, icon sizes, labels, refresh interval, the map preloader and the
//! Thunderforest key.

use super::num;
use crate::map_preloader::PRELOAD_MIN_ZOOM;
use crate::tiles;
use as_geo::latlon::GRect;
use as_maps::preload_tile_count;
use as_settings::{Secret, Settings, MAP_PRELOADER_MAX_ZOOM};
use eframe::egui::{ComboBox, Grid, TextEdit, Ui};

/// Edit buffer of the API key, with the "show" toggle. Masked unless shown.
pub struct KeyEdit {
    buf: String,
    pub show: bool,
}

impl KeyEdit {
    pub fn new(key: &Secret) -> Self {
        Self {
            buf: key.expose().to_string(),
            show: false,
        }
    }
}

pub fn show(ui: &mut Ui, s: &mut Settings, key: &mut KeyEdit) {
    let list = tiles::providers(&s.gui.thunderforest_api_key);
    // A provider that needs a key falls back to OSM without it (provider_by_name).
    if !list.iter().any(|p| p.name == s.gui.map_provider) {
        s.gui.map_provider = list[0].name.to_string();
    }
    let g = &mut s.gui;
    ComboBox::from_label("Provider")
        .selected_text(g.map_provider.clone())
        .show_ui(ui, |ui| {
            for p in &list {
                ui.selectable_value(&mut g.map_provider, p.name.to_string(), p.name);
            }
        });
    Grid::new("opt_map").num_columns(2).show(ui, |ui| {
        num(ui, "Opacity", &mut g.map_opacity, 0.0..=1.0);
        for (i, name) in ["Icon size L", "Icon size M", "Icon size H", "Icon size S"]
            .into_iter()
            .enumerate()
        {
            num(ui, name, &mut g.planes_icon_size[i], 4..=256);
        }
        num(ui, "Update interval [s]", &mut g.map_update_s, 1..=3600);
    });
    ui.checkbox(&mut g.map_show_beam_width, "Show beam width");
    ui.checkbox(&mut g.map_label_calls, "Label calls");
    ui.separator();
    // `cb_Options_Map_Preloader_Enabled` / `ud_Options_Map_Preloader_MaxZoom`
    // (OptionsDlg.Designer.cs:7069-7117, label :2321-2329)
    ui.checkbox(&mut g.map_preloader_enabled, "Enable Map Preloading");
    ui.add_enabled_ui(g.map_preloader_enabled, |ui| {
        Grid::new("opt_map_preloader")
            .num_columns(2)
            .show(ui, |ui| {
                num(
                    ui,
                    "down to Zoom Level:",
                    &mut g.map_preloader_max_zoom,
                    0..=MAP_PRELOADER_MAX_ZOOM,
                );
            });
    });
    // O(zoom levels), no list: cheap enough for every frame
    let a = &s.area;
    let area = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
    let n = preload_tile_count(&area, PRELOAD_MIN_ZOOM, g.map_preloader_max_zoom);
    ui.label(format!(
        "Preloads about {n} tiles from www.airscout.eu (OpenStreetMap only); off by default."
    ));
    ui.separator();
    ui.horizontal(|ui| {
        let l = ui.label("Thunderforest API key");
        let edit = TextEdit::singleline(&mut key.buf)
            .password(!key.show)
            .desired_width(240.0);
        if ui.add(edit).labelled_by(l.id).changed() {
            g.thunderforest_api_key = Secret::from(key.buf.trim());
        }
        ui.checkbox(&mut key.show, "show");
    });
}
