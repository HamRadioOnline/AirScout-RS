//! Plane list tab (not in the original; stage 3 item of the spec) and the nearest-plane box
//! (`gb_Spectrum_NearestInfo`).

use super::map_geom::toggle_selection;
use crate::app::AppState;
use crate::pipeline::TickResult;
use crate::style::{plane_color, rgb};
use as_aircraft::crossing::minutes_to_crossing;
use as_aircraft::PlaneInfo;
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};

/// One table row; all values are display-ready.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneRow {
    pub hex: String,
    pub call: String,
    pub type_: String,
    pub cat: String,
    pub alt_m: i64,
    pub dist_km: f64,
    pub minutes: Option<i32>,
    pub potential: i32,
    pub squint_deg: f64,
    pub color: [u8; 3],
}

/// Rows of the shown planes: highest potential first, then smallest crossing distance.
pub fn plane_rows(tick: &TickResult) -> Vec<PlaneRow> {
    let mut rows: Vec<PlaneRow> = tick
        .shown
        .iter()
        .map(|p| PlaneRow {
            hex: p.hex.clone(),
            call: p.call.clone(),
            type_: type_text(p),
            cat: p.category.short().to_string(),
            alt_m: p.alt_m() as i64,
            dist_km: p.int_qrb,
            minutes: minutes_to_crossing(p),
            potential: p.potential,
            squint_deg: p.squint.to_degrees(),
            color: rgb(plane_color(p.potential)),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.potential
            .cmp(&a.potential)
            .then(a.dist_km.total_cmp(&b.dist_km))
    });
    rows
}

fn type_text(p: &PlaneInfo) -> String {
    if p.model.is_empty() {
        p.type_code.clone()
    } else {
        p.model.clone()
    }
}

fn color32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    nearest_box(ui, state.tick.nearest.as_ref());
    let rows = plane_rows(&state.tick);
    ui.label(format!("{} planes", rows.len()));
    if rows.is_empty() {
        ui.label("No planes in range");
        return;
    }
    let selected = state.selected.clone();
    let mut clicked: Option<String> = None;
    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().at_least(80.0))
        .columns(Column::auto().at_least(40.0), 7)
        .header(18.0, |mut h| {
            for t in [
                "Call",
                "Type",
                "Cat",
                "Alt [m]",
                "Dist [km]",
                "Min",
                "Pot",
                "Squint",
            ] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(18.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                let col = color32(r.color);
                row.col(|ui| {
                    let on = selected.as_deref() == Some(r.hex.as_str());
                    if ui
                        .selectable_label(on, RichText::new(&r.call).color(col))
                        .clicked()
                    {
                        clicked = Some(r.hex.clone());
                    }
                });
                let cells = [
                    r.type_.clone(),
                    r.cat.clone(),
                    r.alt_m.to_string(),
                    format!("{:.0}", r.dist_km),
                    r.minutes
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    r.potential.to_string(),
                    format!("{:.1}", r.squint_deg),
                ];
                for c in cells {
                    row.col(|ui| {
                        ui.label(RichText::new(c).color(col));
                    });
                }
            });
        });
    if let Some(hex) = clicked {
        toggle_selection(&mut state.selected, &hex);
    }
}

/// Call, type, category, altitude, angle and distance of the nearest plane.
pub(super) fn nearest_box(ui: &mut egui::Ui, nearest: Option<&PlaneInfo>) {
    ui.horizontal(|ui| match nearest {
        None => {
            ui.label("Nearest: none");
        }
        Some(p) => {
            ui.label(format!("Nearest: {}", p.call));
            ui.label(format!("Type: {}", type_text(p)));
            ui.label(format!("Cat: {}", p.category.short()));
            ui.label(format!("Alt: {}m", p.alt_m() as i64));
            ui.label(format!("Angle: {:.0}°", p.angle.to_degrees()));
            ui.label(format!("Dist: {:.0}km", p.int_qrb));
        }
    });
}
