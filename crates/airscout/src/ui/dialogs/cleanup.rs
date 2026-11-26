//! `CleanupDlg`: choose what `--clean` erases (`CleanupDlg.Designer.cs`).

use crate::cleanup::CleanupDlg;
use eframe::egui;

/// Draw the dialog into `ui` (it is the whole window). Closing the window is handled by the
/// caller as `CleanupDlg::close`.
pub fn show(ui: &mut egui::Ui, dlg: &mut CleanupDlg) {
    dlg.poll();
    let busy = dlg.busy();
    ui.vertical_centered(|ui| {
        ui.label("This will cleanup your AirScout database and user settings.");
        ui.label("Please choose which parts should be erased:");
    });
    ui.add_space(8.0);
    let failed = !dlg.errors.is_empty();
    ui.add_enabled_ui(!failed && !busy, |ui| {
        let c = &mut dlg.choice;
        ui.checkbox(&mut c.settings, "User Settings");
        ui.checkbox(
            &mut c.aircraft_db,
            "AirScout Database (Aircraft data and positions)",
        );
        ui.checkbox(
            &mut c.scoutbase_db,
            "ScoutBase Database (Elevation, propagation and station data)",
        );
        ui.checkbox(&mut c.tmp, "Tmp Directory");
        ui.checkbox(&mut c.tiles, "Map tile cache");
    });
    for e in &dlg.errors {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    ui.add_space(8.0);
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Deleting...");
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    ui.add_enabled_ui(!busy, |ui| {
        ui.horizontal(|ui| {
            if ui
                .button(if failed { "Retry" } else { "Continue" })
                .clicked()
            {
                dlg.apply();
            }
            if failed && ui.button("Continue anyway").clicked() {
                dlg.proceed_anyway();
            }
            if ui.button("Cancel").clicked() {
                dlg.cancel();
            }
        });
    });
}
