//! Info tab: version, license, attribution.

use eframe::egui::{ScrollArea, Ui};

const NOTICE: &str = include_str!("../../../../../NOTICE");
/// The license text (`LicenseDlg`).
pub const LICENSE: &str = include_str!("../../../../../LICENSE");

/// True if "Show license" was clicked.
pub fn show(ui: &mut Ui) -> bool {
    ui.heading("AirScout-RS");
    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
    ui.label("License: GPL-3.0-only");
    let license = ui.button("Show license").clicked();
    ui.label("Port of AirScout by DL2ALF (https://github.com/dl2alf/AirScout).");
    ui.separator();
    ScrollArea::vertical().id_salt("opt_notice").show(ui, |ui| {
        ui.label(NOTICE);
    });
    license
}
