//! Validation of the Options draft and the field helpers the tabs share.

use super::{cat, network, track};
use as_settings::Settings;
use eframe::egui::{self, emath::Numeric};
use std::ops::RangeInclusive;

fn positive(x: f64) -> bool {
    x > 0.0
}

fn non_negative(x: f64) -> bool {
    x >= 0.0
}

/// Reasons the draft cannot be applied; empty if it is valid.
pub fn validate(s: &Settings) -> Vec<String> {
    let mut e = validate_area(s);
    let b = &s.background;
    if b.update_periodically && b.update_period_min == 0 {
        e.push("Database: the update period must be at least 1 minute".to_string());
    }
    if s.gui.map_update_s == 0 {
        e.push("Map: the update interval must be at least 1 s".to_string());
    }
    if !non_negative(s.gui.alarm_distance_km) {
        e.push("Alarm: the distance must not be negative".to_string());
    }
    if !positive(s.path_max_length_km) {
        e.push("Path: the maximum length must be above 0 km".to_string());
    }
    for (band, bs) in &s.band_settings {
        if !positive(bs.k_factor) {
            e.push(format!("Path {}: K-Factor must be above 0", band.name()));
        }
    }
    e.extend(network::validate(s));
    e.extend(track::validate(s));
    e.extend(cat::validate(s));
    e
}

/// Reasons the area of interest is invalid (shared with the first-run wizard).
pub fn validate_area(s: &Settings) -> Vec<String> {
    let mut e = Vec::new();
    let a = &s.area;
    if !(-90.0..=90.0).contains(&a.min_lat) || !(-90.0..=90.0).contains(&a.max_lat) {
        e.push("Area: latitude must be within -90..90".to_string());
    } else if a.min_lat.partial_cmp(&a.max_lat) != Some(std::cmp::Ordering::Less) {
        e.push("Area: min latitude must be below max latitude".to_string());
    }
    if !(-180.0..=180.0).contains(&a.min_lon) || !(-180.0..=180.0).contains(&a.max_lon) {
        e.push("Area: longitude must be within -180..180".to_string());
    } else if a.min_lon.partial_cmp(&a.max_lon) != Some(std::cmp::Ordering::Less) {
        e.push("Area: min longitude must be below max longitude".to_string());
    }
    e
}

/// A labelled number field in a two-column grid.
pub(crate) fn num<N: Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut N,
    range: RangeInclusive<N>,
) {
    num_step(ui, label, value, range, 1.0);
}

/// Like [`num`], with the drag step (`NumericUpDown.Increment`).
pub(crate) fn num_step<N: Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    step: f64,
) {
    let l = ui.label(label);
    ui.add(egui::DragValue::new(value).range(range).speed(step))
        .labelled_by(l.id);
    ui.end_row();
}

/// A labelled text field in a two-column grid.
pub(crate) fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(300.0))
        .labelled_by(l.id);
    ui.end_row();
}
