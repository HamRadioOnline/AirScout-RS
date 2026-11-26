//! General tab: the map area of interest.

use super::num;
use as_settings::Settings;
use eframe::egui::{Grid, Ui};

pub fn show(ui: &mut Ui, s: &mut Settings) {
    ui.strong("Area of interest [deg]");
    let a = &mut s.area;
    Grid::new("opt_area").num_columns(2).show(ui, |ui| {
        num(ui, "Min Lat", &mut a.min_lat, -90.0..=90.0);
        num(ui, "Max Lat", &mut a.max_lat, -90.0..=90.0);
        num(ui, "Min Lon", &mut a.min_lon, -180.0..=180.0);
        num(ui, "Max Lon", &mut a.max_lon, -180.0..=180.0);
    });
}
