//! Path profile tab (`pv_Path_Paint`, MapDlg.cs:4705): terrain, minimum plane heights, line of
//! sight and the planes near the path.

use crate::app::AppState;
use crate::charts::{profile, ProfileSeries};
use crate::pipeline::ChartPlane;
use eframe::egui::{self, Color32};
use egui_plot::{Line, LineStyle, MarkerShape, Plot, PlotPoint, PlotPoints, Points, Polygon, Text};

const BROWN: Color32 = Color32::from_rgb(139, 90, 43);
const GOLD: Color32 = Color32::from_rgb(255, 215, 0);
const DARK_BLUE: Color32 = Color32::from_rgb(0, 0, 139);
const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);
const GRAY: Color32 = Color32::from_rgb(128, 128, 128);

/// Closed polygon between an upper and a lower edge.
fn band(upper: &[[f64; 2]], lower: &[[f64; 2]]) -> Vec<[f64; 2]> {
    upper.iter().chain(lower.iter().rev()).copied().collect()
}

fn plane_points(planes: &[ChartPlane], color: Color32) -> Vec<Points<'static>> {
    planes
        .iter()
        .map(|p| {
            Points::new(p.call.clone(), vec![[p.dist_km, p.alt_m]])
                .shape(MarkerShape::Square)
                .radius(4.0)
                .color(color)
        })
        .collect()
}

pub fn show(ui: &mut egui::Ui, state: &AppState) {
    let Some(entry) = state.single_path() else {
        ui.label("No path");
        return;
    };
    let s: ProfileSeries = profile(
        &entry.path,
        &entry.terrain,
        f64::from(state.settings.planes_max_alt_m),
    );
    if s.elevation.is_empty() {
        ui.label("Path profile not available yet");
        return;
    }
    let ground: Vec<[f64; 2]> = s.elevation.iter().map(|p| [p[0], 0.0]).collect();
    let min_h_lower: Vec<[f64; 2]> = s.min_h_area.iter().map(|(u, l)| [u[0], *l]).collect();
    let max_h_upper: Vec<[f64; 2]> = s.min_h_area.iter().map(|(u, _)| *u).collect();
    let end_x = s.elevation.last().map_or(0.0, |p| p[0]);
    let my = state.settings.my_call.clone();
    let dx = state.settings.dx_call.clone();
    let top = s
        .elevation_y_max
        .max(state.settings.planes_max_alt_m as f64);
    Plot::new("path_profile")
        .allow_scroll(false)
        .x_axis_label("km")
        .y_axis_label("m")
        .include_y(0.0)
        .include_y(top)
        .show(ui, |pu| {
            pu.polygon(
                Polygon::new("Elevation", band(&s.elevation, &ground))
                    .fill_color(BROWN.gamma_multiply(0.6))
                    .stroke((1.0, BROWN)),
            );
            pu.polygon(
                Polygon::new("Min H", band(&max_h_upper, &min_h_lower))
                    .fill_color(MAGENTA.gamma_multiply(0.25))
                    .stroke((0.0, MAGENTA)),
            );
            pu.line(Line::new("Min H1", PlotPoints::from(s.min_h1.clone())).color(Color32::RED));
            pu.line(Line::new("Min H2", PlotPoints::from(s.min_h2.clone())).color(GOLD));
            pu.line(
                Line::new("Max H", PlotPoints::from(s.max_h.clone()))
                    .color(DARK_BLUE)
                    .style(LineStyle::dotted_dense()),
            );
            pu.line(Line::new("LOS", PlotPoints::from(s.los.clone())).color(Color32::BLACK));
            for p in plane_points(&state.tick.chart_lo, GRAY)
                .into_iter()
                .chain(plane_points(&state.tick.chart_hi, MAGENTA))
            {
                pu.points(p);
            }
            pu.text(Text::new("My", PlotPoint::new(0.0, top), my));
            pu.text(Text::new("DX", PlotPoint::new(end_x, top), dx));
        });
}
