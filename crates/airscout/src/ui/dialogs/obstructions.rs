//! `LocalObstructionsDlg`: the local obstruction of My station as a grid of 360 bearings with
//! the distance [m] and height [m] of the obstacle. OK stores it; Cancel changes nothing.

use super::watchlist::DlgResult;
use as_elevation::{ElevationModel, LocalObstruction};
use eframe::egui::{self, DragValue, Role, WidgetInfo};
use egui_extras::{Column, TableBuilder};

/// Upper bound of the number fields: nothing on earth is farther or higher.
const MAX_VALUE: f64 = 1_000_000.0;

pub struct ObstructionsDlg {
    /// Model the obstruction is stored for (the effective one).
    pub model: ElevationModel,
    pub lat: f64,
    pub lon: f64,
    /// Distance [m] per bearing.
    pub distance: [f64; 360],
    /// Height [m] per bearing.
    pub height: [f64; 360],
    /// Why there is nothing to edit; OK is disabled then.
    pub error: Option<String>,
}

impl ObstructionsDlg {
    /// Dialog on the stored (or default) obstruction.
    pub fn new(model: ElevationModel, o: &LocalObstruction) -> Self {
        Self {
            model,
            lat: o.lat,
            lon: o.lon,
            distance: o.distance,
            height: o.height,
            error: None,
        }
    }

    /// A dialog that only shows why it cannot be used.
    pub fn failed(lat: f64, lon: f64, error: String) -> Self {
        Self {
            model: ElevationModel::None,
            lat,
            lon,
            distance: [0.0; 360],
            height: [0.0; 360],
            error: Some(error),
        }
    }

    /// `btn_ClearAll_Click`: distance and height of every bearing to 0.
    pub fn clear_all(&mut self) {
        self.distance = [0.0; 360];
        self.height = [0.0; 360];
    }

    /// The edited obstruction, for storing.
    pub fn result(&self) -> LocalObstruction {
        LocalObstruction::new(self.lat, self.lon, Some(self.distance), Some(self.height))
    }

    pub fn show(&mut self, ctx: &egui::Context) -> DlgResult {
        let mut result = DlgResult::Open;
        egui::Modal::new(egui::Id::unique("obstructions_dlg")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Local obstructions");
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            } else {
                ui.label(format!(
                    "Station at {:.6}, {:.6}, elevation model {}",
                    self.lat,
                    self.lon,
                    self.model.dir_name()
                ));
                self.grid(ui);
            }
            ui.horizontal(|ui| {
                let ok = ui.add_enabled(self.error.is_none(), egui::Button::new("OK"));
                if ok.clicked() {
                    result = DlgResult::Ok;
                }
                if ui
                    .add_enabled(self.error.is_none(), egui::Button::new("Clear all"))
                    .clicked()
                {
                    self.clear_all();
                }
                if ui.button("Cancel").clicked() {
                    result = DlgResult::Cancel;
                }
            });
        });
        result
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        TableBuilder::new(ui)
            .striped(true)
            .max_scroll_height(320.0)
            .column(Column::auto().at_least(70.0))
            .columns(Column::auto().at_least(110.0), 2)
            .header(18.0, |mut h| {
                for t in ["Bearing [deg]", "Distance [m]", "Height [m]"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, 360, |mut row| {
                    let i = row.index();
                    row.col(|ui| {
                        ui.label(i.to_string());
                    });
                    let cells = [
                        (&mut self.distance[i], "distance"),
                        (&mut self.height[i], "height"),
                    ];
                    for (v, what) in cells {
                        row.col(|ui| {
                            let r = ui.add(DragValue::new(v).range(0.0..=MAX_VALUE));
                            let name = format!("{i} {what}");
                            r.widget_info(|| {
                                WidgetInfo::labeled(Role::SpinButton, true, name.clone())
                            });
                        });
                    }
                });
            });
    }
}
