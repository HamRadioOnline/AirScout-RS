//! SpecLab tab.

use super::{num, text};
use as_settings::Settings;
use eframe::egui::{Grid, Ui};

pub fn show(ui: &mut Ui, s: &mut Settings) {
    let l = &mut s.speclab;
    ui.checkbox(&mut l.enabled, "Enable SpecLab");
    Grid::new("opt_speclab").num_columns(2).show(ui, |ui| {
        text(ui, "URL", &mut l.url);
        text(ui, "File name", &mut l.file_name);
        num(ui, "F1 [Hz]", &mut l.f1, 0..=u32::MAX);
        num(ui, "F2 [Hz]", &mut l.f2, 0..=u32::MAX);
        num(ui, "Update interval [s]", &mut l.update_s, 1..=86_400);
    });
}
