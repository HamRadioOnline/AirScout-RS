//! Validation of the Options draft and the field helpers the tabs share.

use super::{cat, network, track};
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets;
use as_geo::latlon::GRect;
use as_settings::{Area, Settings};
use eframe::egui::{self, emath::Numeric, Rect, RichText};
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
    e.extend(validate_planes(s));
    let b = &s.background;
    if b.update_periodically && b.update_period_min == 0 {
        e.push("Database: the update period must be at least 1 minute".to_string());
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

/// The Planes page allows equal minimum and maximum altitudes (OptionsDlg.cs:2215).
pub(super) fn validate_planes(s: &Settings) -> Option<String> {
    (s.planes_max_alt_m < s.planes_min_alt_m)
        .then(|| "MaxAlt must be greater than MinAlt.".to_string())
}

/// Reasons the area of interest is invalid (shared with the first-run wizard).
pub fn validate_area(s: &Settings) -> Vec<String> {
    validate_area_bounds(&s.area)
}

/// Reasons an [`Area`]'s bounds are invalid; empty if it is a proper rectangle.
pub fn validate_area_bounds(a: &Area) -> Vec<String> {
    let mut e = Vec::new();
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

/// The area of interest as a [`GRect`], or `None` when its bounds are invalid.
pub(super) fn covered_area(a: &Area) -> Option<GRect> {
    if !validate_area_bounds(a).is_empty() {
        return None;
    }
    Some(GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon))
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

/// A whole-number field at its Designer `rect` (an `Int32TextBox` or an integer
/// `NumericUpDown`). An edit is rounded to a whole number and clamped to `range`, also for an
/// `f64` setting; a stored value outside `range` is shown as is until the user edits it.
pub(super) fn whole_number<N: Numeric>(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    rect: Rect,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, "", rect);
    let (min, max) = (range.start().to_f64(), range.end().to_f64());
    ui.put(
        rect,
        egui::DragValue::from_get_set(|edit| {
            if let Some(edit) = edit.filter(|edit| edit.is_finite()) {
                *value = N::from_f64(edit.round().clamp(min, max));
            }
            value.to_f64()
        })
        .range(min..=max)
        .speed(1.0)
        .max_decimals(0)
        .custom_parser(|text| text.trim().parse::<f64>().ok().map(f64::round))
        .clamp_existing_to_range(false)
        .min_size(rect.size()),
    )
}

/// A multi-line Designer label in the small font, so its lines fit the Designer height.
pub(super) fn small_label(ui: &mut egui::Ui, name: &str, text: &str, rect: Rect) -> egui::Response {
    trace::record(ui, name, Kind::Text, text, rect);
    widgets::put_left(
        ui,
        rect,
        egui::Label::new(RichText::new(text).small()).extend(),
    )
}

/// A Designer label in the small font that wraps within its rect width, so it stays within the
/// Designer height.
pub(super) fn small_wrapped_label(
    ui: &mut egui::Ui,
    name: &str,
    text: &str,
    rect: Rect,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, text, rect);
    widgets::put_left(
        ui,
        rect,
        egui::Label::new(RichText::new(text).small()).wrap(),
    )
}
