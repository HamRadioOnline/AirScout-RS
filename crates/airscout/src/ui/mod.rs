//! Drawing only: every widget calls [`crate::app::AppState`] methods for its logic.

pub mod analysis;
pub mod control;
pub mod dialogs;
pub mod map;
pub mod map_geom;
pub mod map_track;
pub mod multi;
pub mod options;
pub mod planes;
pub mod profile;
pub mod spectrum;
pub mod status;

use crate::app::{AppState, BottomTab};
use as_geo::band::Band;
use eframe::egui::{self, CentralPanel, ComboBox, Panel};

/// C# `Color.LightSalmon`.
const LIGHT_SALMON: egui::Color32 = egui::Color32::from_rgb(255, 160, 122);
const DEFAULT_SIDE_PANEL_WIDTH: f32 = 280.0;
const DEFAULT_BOTTOM_PANEL_HEIGHT: f32 = 220.0;

/// The whole window: top bar, status bar, right control panel, central map area.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, map: &mut map::MapView) {
    Panel::top("top_bar").show(ui, |ui| top_bar(ui, state));
    Panel::bottom("status_bar").show(ui, |ui| status::show(ui, state));
    let width = state
        .settings
        .gui
        .side_panel_width
        .unwrap_or(DEFAULT_SIDE_PANEL_WIDTH);
    let side = Panel::right("control_panel")
        .default_size(width)
        .show(ui, |ui| control::show(ui, state));
    state.settings.gui.side_panel_width = Some(side.response.rect.width());
    let height = state
        .settings
        .gui
        .bottom_panel_height
        .unwrap_or(DEFAULT_BOTTOM_PANEL_HEIGHT);
    let bottom = Panel::bottom("bottom_panel")
        .resizable(true)
        .default_size(height)
        .show(ui, |ui| bottom_tabs(ui, state));
    // Only write on change: a plain store would dirty the settings every frame.
    let h = bottom.response.rect.height();
    if state
        .settings
        .gui
        .bottom_panel_height
        .is_none_or(|old| (old - h).abs() > 0.5)
    {
        state.settings.gui.bottom_panel_height = Some(h);
    }
    CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| map.show(ui, state));
    dialogs::show(ui.ctx(), state);
}

/// Tabs below the map: path profile and plane list.
fn bottom_tabs(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.bottom_tab, BottomTab::Profile, "Path Profile");
        ui.selectable_value(&mut state.bottom_tab, BottomTab::Planes, "Planes");
        ui.selectable_value(&mut state.bottom_tab, BottomTab::Spectrum, "Spectrum");
        ui.selectable_value(&mut state.bottom_tab, BottomTab::Analysis, "Analysis");
    });
    ui.separator();
    match state.bottom_tab {
        BottomTab::Profile => profile::show(ui, state),
        BottomTab::Planes => planes::show(ui, state),
        BottomTab::Spectrum => spectrum::show(ui, state),
        BottomTab::Analysis => analysis::show(ui, state),
    }
}

/// `pa_CommonInfo`: UTC time and band.
fn top_bar(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        // tb_UTC (MapDlg.cs:3400): LightSalmon online, Plum offline; a double click sets the time
        let fill = if state.settings.time.online {
            LIGHT_SALMON
        } else {
            status::ALARM_PLUM
        };
        let utc = state.current_time(chrono::Utc::now());
        let text = egui::RichText::new(format!("UTC {}", utc.format("%Y-%m-%d %H:%M:%S")))
            .background_color(fill)
            .color(egui::Color32::BLACK);
        let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        if r.double_clicked() {
            state.open_set_time_dlg();
        }
        ui.separator();
        ui.add_enabled_ui(!state.is_playing(), |ui| {
            ComboBox::from_label("Band")
                .selected_text(state.settings.band.name())
                .show_ui(ui, |ui| {
                    for b in Band::all_except_none_and_all() {
                        ui.selectable_value(&mut state.settings.band, *b, b.name());
                    }
                });
        });
    });
}
