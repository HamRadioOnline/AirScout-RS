//! Path tab: per-band propagation settings and the global path limits.

use super::num;
use as_settings::Settings;
use eframe::egui::{DragValue, Grid, Role, Ui, WidgetInfo};

pub fn show(ui: &mut Ui, s: &mut Settings) {
    const HEADS: [&str; 6] = [
        "K-Factor",
        "F1-Clearance",
        "Ground Clearance [m]",
        "Max Distance [km]",
        "Max Squint [deg]",
        "Max Elevation [deg]",
    ];
    Grid::new("opt_bands").striped(true).show(ui, |ui| {
        ui.strong("Band");
        for h in HEADS {
            ui.strong(h);
        }
        ui.end_row();
        for (band, b) in &mut s.band_settings {
            ui.label(band.name());
            let cells = [
                (&mut b.k_factor, 0.1..=10.0, 0.01),
                (&mut b.f1_clearance, 0.0..=1.0, 0.01),
                (&mut b.ground_clearance, 0.0..=1000.0, 1.0),
                (&mut b.max_distance, 0.0..=1000.0, 1.0),
                (&mut b.max_squint, 0.0..=180.0, 1.0),
                (&mut b.max_elevation, 0.0..=90.0, 1.0),
            ];
            for ((v, range, speed), head) in cells.into_iter().zip(HEADS) {
                let r = ui.add(DragValue::new(v).speed(speed).range(range));
                // a column header alone would not tell which band the cell belongs to
                let name = format!("{} {head}", band.name());
                r.widget_info(|| WidgetInfo::labeled(Role::SpinButton, true, name.clone()));
            }
            ui.end_row();
        }
    });
    ui.separator();
    ui.checkbox(&mut s.path_best_case_elevation, "Best case elevation");
    Grid::new("opt_path").num_columns(2).show(ui, |ui| {
        num(
            ui,
            "Max path length [km]",
            &mut s.path_max_length_km,
            1.0..=20000.0,
        );
        num(
            ui,
            "Near field suppression [m]",
            &mut s.path_near_field_suppression_m,
            0.0..=100_000.0,
        );
    });
}
