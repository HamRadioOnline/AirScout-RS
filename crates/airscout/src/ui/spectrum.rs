//! Spectrum tab (`pm_Spectrum`, `gb_Spectrum_NearestInfo`): the maximum Spectrum Lab level over
//! time with a magenta band behind the samples taken while recording, a status line and the
//! nearest plane. The nearest plane's map overlay (`gm_Nearest`) is not ported.

use super::planes::nearest_box;
use crate::app::AppState;
use crate::speclab::{SpectrumState, SPECTRUM_MAX_POINTS};
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Line, Plot, PlotBounds, PlotPoints, Polygon};

const GOLDENROD: Color32 = Color32::from_rgb(218, 165, 32);
const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);

/// X ranges `[from, to)` of the runs of samples taken while recording.
pub fn recorded_runs(sp: &SpectrumState) -> Vec<(f64, f64)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, rec) in sp.recorded().enumerate() {
        match (rec, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s as f64, i as f64));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s as f64, sp.len() as f64));
    }
    runs
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    nearest_box(ui, state.tick.nearest.as_ref());
    let sp = &state.spectrum;
    if sp.is_error {
        ui.label(
            RichText::new(&sp.status)
                .color(Color32::WHITE)
                .background_color(Color32::RED),
        );
    } else if sp.status.is_empty() {
        ui.label("Spectrum Lab: no data");
    } else {
        ui.label(&sp.status);
    }
    let (y_min, y_max) = sp.y_range();
    let line: Vec<[f64; 2]> = sp
        .levels()
        .enumerate()
        .map(|(i, l)| [i as f64, l])
        .collect();
    let bands = recorded_runs(sp);
    Plot::new("spectrum")
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .auto_bounds(false)
        .x_axis_label("samples")
        .y_axis_label("dB")
        .show(ui, |pu| {
            pu.set_plot_bounds(PlotBounds::from_min_max(
                [0.0, y_min],
                [SPECTRUM_MAX_POINTS as f64, y_max],
            ));
            for (i, (a, b)) in bands.into_iter().enumerate() {
                pu.polygon(
                    Polygon::new(
                        format!("Recording {i}"),
                        vec![[a, y_min], [b, y_min], [b, y_max], [a, y_max]],
                    )
                    .fill_color(MAGENTA.gamma_multiply(0.08))
                    .stroke((0.0, MAGENTA)),
                );
            }
            pu.line(
                Line::new("Level", PlotPoints::from(line))
                    .color(GOLDENROD)
                    .width(3.0),
            );
        });
}
