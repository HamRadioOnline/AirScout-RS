//! Elevation tab: enabled sources and the download.

use super::text;
use as_elevation::ElevationModel;
use as_settings::Settings;
use eframe::egui::{Grid, Ui};

/// A button of the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElevationAction {
    /// The model's copyright text (`ElevationCopyrightDlg`).
    Copyright(ElevationModel),
    /// The model's coverage map. An explicit button: the C# starts the download on tab Enter.
    Coverage(ElevationModel),
}

/// The button that was clicked.
pub fn show(ui: &mut Ui, s: &mut Settings) -> Option<ElevationAction> {
    let e = &mut s.elevation_enabled;
    let mut action = None;
    ui.strong("Elevation models");
    for (model, on) in [
        (ElevationModel::Globe, &mut e.globe),
        (ElevationModel::Srtm3, &mut e.srtm3),
        (ElevationModel::Srtm1, &mut e.srtm1),
        (ElevationModel::Aster3, &mut e.aster3),
        (ElevationModel::Aster1, &mut e.aster1),
    ] {
        ui.horizontal(|ui| {
            ui.checkbox(on, model.dir_name());
            if ui.small_button("Copyright").clicked() {
                action = Some(ElevationAction::Copyright(model));
            }
            if ui.small_button("Show coverage").clicked() {
                action = Some(ElevationAction::Coverage(model));
            }
        });
    }
    if !(e.globe || e.srtm3 || e.srtm1 || e.aster3 || e.aster1) {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "No elevation model enabled: paths are computed without terrain.",
        );
    }
    ui.separator();
    ui.checkbox(
        &mut s.gui.elevation_download,
        "Download missing elevation data",
    );
    Grid::new("opt_elev").num_columns(2).show(ui, |ui| {
        text(ui, "Base URL", &mut s.gui.elevation_base_url);
    });
    action
}
