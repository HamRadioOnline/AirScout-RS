//! Right control panel (`tc_Control`): Single and Opt tabs, Play/Pause.

use crate::app::{AppState, ControlTab};
use as_aircraft::enums::LifeMode;
use as_aircraft::PlaneCategory;
use eframe::egui::{self, Color32, ComboBox, DragValue, TextEdit};

const PALE_GREEN: Color32 = Color32::from_rgb(152, 251, 152);
const PALE_RED: Color32 = Color32::from_rgb(255, 182, 182);

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let label = if state.is_playing() { "Pause" } else { "Play" };
    // HISTORY has its own player (Analysis tab); the feed cannot run there
    let play_ok = state.life_mode != LifeMode::History;
    if ui.add_enabled(play_ok, egui::Button::new(label)).clicked() {
        if state.is_playing() {
            state.pause();
        } else {
            // the reason of a refusal is in `state.status`
            let _ = state.play();
        }
    }
    // the Options dialog edits the inputs of a run: only while paused (btn_Options)
    if ui
        .add_enabled(!state.is_playing(), egui::Button::new("Options"))
        .clicked()
    {
        state.open_options_dlg();
    }
    ui.separator();
    ui.horizontal(|ui| {
        // the tab is fixed while playing (tc_Control_Selecting)
        for (tab, name) in [
            (ControlTab::Single, "Single"),
            (ControlTab::Multi, "Multi"),
            (ControlTab::Opt, "Opt"),
        ] {
            let r = ui.selectable_label(state.control_tab == tab, name);
            if r.clicked() {
                state.set_control_tab(tab);
            }
        }
    });
    ui.separator();
    match state.control_tab {
        ControlTab::Single => single(ui, state),
        ControlTab::Multi => super::multi::show(ui, state),
        ControlTab::Opt => opt(ui, state),
    }
}

/// A text field tied to its label, so screen readers and tests find it by name.
fn text_field(
    ui: &mut egui::Ui,
    enabled: bool,
    name: &str,
    text: &mut String,
    fill: Option<Color32>,
) -> bool {
    let l = ui.label(name);
    let mut edit = TextEdit::singleline(text).desired_width(120.0);
    if let Some(f) = fill {
        edit = edit.background_color(f).text_color(Color32::BLACK);
    }
    let r = ui.add_enabled(enabled, edit).labelled_by(l.id);
    ui.end_row();
    r.changed()
}

/// Pale green if the position is more precise than the locator shows (`UpdateStatus`),
/// pale red if the text is not a position.
fn loc_fill(text: &str, lat: f64, lon: f64) -> Option<Color32> {
    if crate::app::parse_position(text).is_none() {
        return Some(PALE_RED);
    }
    as_geo::maidenhead::is_precise(lat, lon, 3)
        .unwrap_or(false)
        .then_some(PALE_GREEN)
}

fn single(ui: &mut egui::Ui, state: &mut AppState) {
    let enabled = !state.is_playing();
    egui::Grid::new("single_grid")
        .num_columns(2)
        .show(ui, |ui| {
            text_field(ui, enabled, "My call", &mut state.settings.my_call, None);
            let s = &state.settings;
            let fill = loc_fill(&state.my_loc_text, s.my_lat, s.my_lon);
            let mut t = state.my_loc_text.clone();
            if text_field(ui, enabled, "My loc", &mut t, fill) {
                state.set_my_loc_text(&t);
            }
            text_field(ui, enabled, "DX call", &mut state.settings.dx_call, None);
            let s = &state.settings;
            let fill = if state.path_too_long().is_some() {
                Some(PALE_RED)
            } else {
                loc_fill(&state.dx_loc_text, s.dx_lat, s.dx_lon)
            };
            let mut t = state.dx_loc_text.clone();
            if text_field(ui, enabled, "DX loc", &mut t, fill) {
                state.set_dx_loc_text(&t);
            }
        });
    let (qtf, qrb) = state.qtf_qrb();
    ui.label(format!("QTF {qtf}°"));
    ui.label(format!("QRB {qrb} km"));
}

fn opt(ui: &mut egui::Ui, state: &mut AppState) {
    let enabled = !state.is_playing();
    ui.group(|ui| {
        ui.strong("Zoom");
        ui.horizontal(|ui| {
            if ui.button("Zoom +").clicked() {
                state.pending_zoom += 1;
            }
            if ui.button("Zoom -").clicked() {
                state.pending_zoom -= 1;
            }
        });
        ui.checkbox(&mut state.settings.gui.map_auto_center, "Auto-center");
    });
    ui.group(|ui| {
        ui.strong("Filter");
        ui.add_enabled_ui(enabled, |ui| {
            let g = &mut state.settings.gui;
            let l = ui.label("Min alt [m]");
            ui.add(DragValue::new(&mut g.planes_filter_min_alt_m).range(0..=20000))
                .labelled_by(l.id);
            let l = ui.label("Max circumcircle [km]");
            ui.add(DragValue::new(&mut g.planes_filter_max_circumcircle_km).range(0.0..=2000.0))
                .labelled_by(l.id);
            ComboBox::from_label("Min category")
                .selected_text(format!("{:?}", g.planes_filter_min_category))
                .show_ui(ui, |ui| {
                    for c in PlaneCategory::ALL {
                        ui.selectable_value(&mut g.planes_filter_min_category, c, format!("{c:?}"));
                    }
                });
        });
    });
    let alarm = state.alarm_text().is_some();
    let frame = egui::Frame::group(ui.style()).fill(if alarm {
        crate::ui::status::ALARM_PLUM
    } else {
        Color32::TRANSPARENT
    });
    frame.show(ui, |ui| {
        ui.strong("Alarm");
        ui.checkbox(&mut state.settings.gui.alarm_activate, "Alarm on");
    });
}
