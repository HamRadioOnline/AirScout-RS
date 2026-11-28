//! Alarm tab.

use super::num;
use as_settings::Settings;
use eframe::egui::{Grid, Ui};

pub fn show(ui: &mut Ui, s: &mut Settings) {
    let g = &mut s.gui;
    ui.checkbox(&mut g.alarm_activate, "Activate alarm");
    Grid::new("opt_alarm").num_columns(2).show(ui, |ui| {
        num(
            ui,
            "Distance [km]",
            &mut g.alarm_distance_km,
            0.0..=10_000.0,
        );
    });
    ui.checkbox(&mut g.alarm_bring_to_front, "Bring window to front");
    ui.checkbox(&mut g.alarm_play_sound, "Play sound");
}
