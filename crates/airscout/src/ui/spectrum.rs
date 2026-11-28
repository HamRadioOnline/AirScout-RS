//! Spectrum tab (`tp_Spectrum`): the maximum Spectrum Lab level over time with a magenta band
//! behind the samples taken while recording, a status line, and the nearest plane info and map.

use super::{map::MapView, planes::nearest_texts};
use crate::app::AppState;
use crate::speclab::{SpectrumState, SPECTRUM_MAX_POINTS};
use crate::ui::forms::geometry::{anchored, designer_rect, Anchor};
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets::{group_box, label, label_truncated, put_left, scoped};
use as_aircraft::PlaneInfo;
use eframe::egui::{self, vec2, Color32, Rect, RichText};
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

pub fn show(ui: &mut egui::Ui, state: &AppState, map: &mut MapView) {
    let rect = ui.max_rect();
    let content = vec2(rect.width().max(844.0), rect.height().max(333.0));
    egui::ScrollArea::both()
        .id_salt("spectrum_scroll")
        .show(ui, |ui| {
            let page = Rect::from_min_size(ui.cursor().min, content);
            ui.set_min_size(content);
            draw(ui, state, map, page);
        });
}

fn draw(ui: &mut egui::Ui, state: &AppState, map: &mut MapView, page: Rect) {
    let designer = vec2(844.0, 333.0);

    let nearest = anchored(
        designer,
        page,
        [489, 54],
        [175, 270],
        Anchor {
            top: true,
            bottom: true,
            left: false,
            right: true,
        },
    );
    group_box(
        ui,
        "gb_Spectrum_NearestInfo",
        "Nearest Plane Info",
        nearest,
        |ui| {
            nearest_info(ui, state.tick.nearest.as_ref(), nearest);
        },
    );

    let status = anchored(
        designer,
        page,
        [8, 6],
        [830, 40],
        Anchor {
            top: true,
            bottom: false,
            left: true,
            right: true,
        },
    );
    group_box(ui, "gb_Spectrum_Status", "Status", status, |ui| {
        status_text(ui, &state.spectrum, status);
    });

    let map_group = anchored(
        designer,
        page,
        [670, 54],
        [168, 273],
        Anchor {
            top: true,
            bottom: true,
            left: false,
            right: true,
        },
    );
    group_box(
        ui,
        "gb_NearestPlaneMap",
        "Nearest Plane Map",
        map_group,
        |ui| {
            let rect = Rect::from_min_max(
                map_group.min + vec2(3.0, 16.0),
                map_group.max - vec2(3.0, 3.0),
            );
            trace::scope(ui, "gm_Nearest", rect, |ui| {
                scoped(ui, rect, |ui| map.show_nearest(ui, state));
            });
        },
    );

    let spectrum = anchored(
        designer,
        page,
        [8, 54],
        [475, 273],
        Anchor {
            top: true,
            bottom: true,
            left: true,
            right: true,
        },
    );
    group_box(ui, "gb_Spectrum", "Spectrum", spectrum, |ui| {
        plot(ui, &state.spectrum, spectrum);
    });
}

/// `tb_Spectrum_Status`: the status line, white on red when it is an error.
fn status_text(ui: &mut egui::Ui, sp: &SpectrumState, group: Rect) {
    // The Designer size `[129, 20]` is the AutoSize of "0"; stretch the width
    // across the status box (MapDlg.cs:6487-6488) and keep it on one line.
    let rect = Rect::from_min_size(
        group.min + vec2(6.0, 14.0),
        vec2((group.width() - 12.0).max(129.0), 20.0),
    );
    let text: RichText = if sp.is_error {
        RichText::new(&sp.status)
            .color(Color32::WHITE)
            .background_color(Color32::RED)
    } else if sp.status.is_empty() {
        RichText::new("Spectrum Lab: no data")
    } else {
        RichText::new(&sp.status)
    };
    trace::record(ui, "tb_Spectrum_Status", Kind::Text, &sp.status, rect);
    put_left(ui, rect, egui::Label::new(text).truncate());
}

fn plot(ui: &mut egui::Ui, sp: &SpectrumState, group: Rect) {
    let area = Rect::from_min_max(group.min + vec2(2.0, 18.0), group.max - vec2(2.0, 2.0));
    scoped(ui, area, |ui| {
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
    });
}

/// `gb_Spectrum_NearestInfo`: retain the last fix's texts, as in MapDlg.cs:7809–7822.
fn nearest_info(ui: &mut egui::Ui, nearest: Option<&PlaneInfo>, group: Rect) {
    struct Row {
        caption_name: &'static str,
        caption: &'static str,
        value_name: &'static str,
        size: [i32; 2],
        y: i32,
        i: usize,
    }
    let values_id = ui.make_persistent_id("nearest_values");
    let vals = ui.data_mut(|data| {
        if let Some(plane) = nearest {
            let vals = nearest_texts(plane);
            data.insert_temp(values_id, vals.clone());
            vals
        } else {
            data.get_temp(values_id)
                .unwrap_or_else(|| std::array::from_fn(|_| "0".to_string()))
        }
    });
    let rows = [
        Row {
            caption_name: "label21",
            caption: "Call:",
            value_name: "lbl_Nearest_Call",
            size: [27, 13],
            y: 26,
            i: 0,
        },
        Row {
            caption_name: "label2",
            caption: "Type:",
            value_name: "lbl_Nearest_Type",
            size: [34, 13],
            y: 44,
            i: 1,
        },
        Row {
            caption_name: "label5",
            caption: "Category:",
            value_name: "lbl_Nearest_Cat",
            size: [52, 13],
            y: 60,
            i: 2,
        },
        Row {
            caption_name: "label4",
            caption: "Altitude:",
            value_name: "lbl_Nearest_Alt",
            size: [45, 13],
            y: 79,
            i: 3,
        },
        Row {
            caption_name: "label7",
            caption: "Angle:",
            value_name: "lbl_Nearest_Angle",
            size: [37, 13],
            y: 97,
            i: 4,
        },
        Row {
            caption_name: "label22",
            caption: "Distance:",
            value_name: "lbl_Nearest_Dist",
            size: [52, 13],
            y: 114,
            i: 5,
        },
    ];
    for row in rows {
        label(
            ui,
            row.caption_name,
            row.caption,
            designer_rect(group, [6, row.y], row.size),
        );
        // Designer `[14, 14]` is the AutoSize of "0"; grow the width across the
        // box and keep the value on a single (>=18pt) line (MapDlg.cs:6489-6496).
        let value_rect = Rect::from_min_size(
            group.min + vec2(82.0, row.y as f32),
            vec2((group.width() - 88.0).max(14.0), 18.0),
        );
        label_truncated(ui, row.value_name, &vals[row.i], value_rect);
    }
}
