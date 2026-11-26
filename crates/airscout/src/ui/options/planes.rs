//! Planes tab: altitude window, position lifetime, filter and the plane info rows.
//! The feed combo opens the disclaimer and settings dialogs (handled by the Options dialog).

use super::num;
use as_aircraft::PlaneCategory;
use as_settings::Settings;
use eframe::egui::{ComboBox, Grid, Ui};

/// What the feed controls asked for this frame.
#[derive(Debug, PartialEq)]
pub enum FeedAction {
    None,
    /// The user picked another feed; the draft is not changed yet.
    Select(String),
    /// Open the settings of the selected feed.
    Settings,
}

pub fn show(ui: &mut Ui, s: &mut Settings) -> FeedAction {
    let action = feed_controls(ui, s);
    ui.separator();
    Grid::new("opt_planes").num_columns(2).show(ui, |ui| {
        num(ui, "Min alt [m]", &mut s.planes_min_alt_m, 0..=100_000);
        num(ui, "Max alt [m]", &mut s.planes_max_alt_m, 0..=100_000);
        num(
            ui,
            "Position TTL [min]",
            &mut s.planes_position_ttl_min,
            1..=1440,
        );
        num(
            ui,
            "Max delta [m]",
            &mut s.planes_max_delta_m,
            0.0..=100_000.0,
        );
    });
    let g = &mut s.gui;
    ComboBox::from_label("Filter min category")
        .selected_text(format!("{:?}", g.planes_filter_min_category))
        .show_ui(ui, |ui| {
            for c in PlaneCategory::ALL {
                ui.selectable_value(&mut g.planes_filter_min_category, c, format!("{c:?}"));
            }
        });
    ui.separator();
    ui.strong("Plane info window");
    let w = &mut g.info_win;
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut w.metric, "Metric");
        ui.checkbox(&mut w.imperial, "Imperial");
        ui.checkbox(&mut w.position, "Position");
        ui.checkbox(&mut w.time, "Time");
        ui.checkbox(&mut w.epsilon, "Epsilon");
        ui.checkbox(&mut w.squint, "Squint");
        ui.checkbox(&mut w.track, "Track");
        ui.checkbox(&mut w.type_, "Type");
        ui.checkbox(&mut w.alt, "Alt");
        ui.checkbox(&mut w.dist, "Dist");
        ui.checkbox(&mut w.speed, "Speed");
        ui.checkbox(&mut w.angle, "Angle");
        ui.checkbox(&mut w.my_azimuth, "My azimuth");
        ui.checkbox(&mut w.my_elevation, "My elevation");
        ui.checkbox(&mut w.my_doppler, "My doppler");
        ui.checkbox(&mut w.dx_azimuth, "DX azimuth");
        ui.checkbox(&mut w.dx_elevation, "DX elevation");
        ui.checkbox(&mut w.dx_doppler, "DX doppler");
    });
    action
}

fn feed_controls(ui: &mut Ui, s: &Settings) -> FeedAction {
    let mut action = FeedAction::None;
    ui.horizontal(|ui| {
        action = feed_combo(ui, s);
        if ui.button("Feed settings...").clicked() {
            action = FeedAction::Settings;
        }
    });
    feed_info(ui, s);
    action
}

/// The feed combo (also used by the first-run wizard).
pub(crate) fn feed_combo(ui: &mut Ui, s: &Settings) -> FeedAction {
    let mut action = FeedAction::None;
    ComboBox::from_label("Feed")
        .selected_text(s.feed.clone())
        .show_ui(ui, |ui| {
            for f in &as_feeds::registry::all_feeds() {
                let r = ui.selectable_label(s.feed == f.name(), f.name());
                if r.on_hover_text(f.info()).clicked() && s.feed != f.name() {
                    action = FeedAction::Select(f.name().to_string());
                }
            }
        });
    action
}

/// The info text of the selected feed.
pub(crate) fn feed_info(ui: &mut Ui, s: &Settings) {
    if let Some(f) = as_feeds::registry::feed_by_name(&s.feed) {
        ui.weak(f.info());
    }
}
