//! Planes tab: three feed slots, the general feed settings, the database maintenance, the live
//! and database position filters and the debug switch. Each slot's combo opens the disclaimer
//! and settings dialogs (handled by the Options dialog). The plane info rows are on the Map tab
//! (`groupBox7`).

use super::fields::{small_label, small_wrapped_label, whole_number};
use crate::ui::forms::geometry::designer_rect;
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets;
use as_aircraft::PlaneCategory;
use as_settings::Settings;
use eframe::egui::{self, emath::Numeric, Button, ComboBox, Rect, TextWrapMode, Ui, UiBuilder};

/// The three feed slots (`Planes_PlaneFeed1/2/3`).
const SLOTS: [usize; 3] = [1, 2, 3];

/// What the feed controls asked for this frame.
#[derive(Debug, PartialEq)]
pub enum FeedAction {
    None,
    /// The user picked another feed for a slot; the draft is not changed yet.
    Select(usize, String),
    /// Open the settings of the feed in a slot.
    Settings(usize),
    Import(usize),
    Export(usize),
    Default(usize),
}

pub fn show(ui: &mut Ui, s: &mut Settings) -> FeedAction {
    let body = ui.max_rect();
    let action = feed_controls(ui, s);
    general_feed_settings(ui, s, body);
    database_maintenance(ui, s, body);
    live_filters(ui, s, body);
    position_filters(ui, s, body);
    debug_settings(ui, s, body);
    action
}

/// The Plane Feeds group, with all three rows visible at once.
fn feed_controls(ui: &mut Ui, s: &Settings) -> FeedAction {
    let group = designer_rect(ui.max_rect(), [12, 12], [645, 108]);
    let mut action = FeedAction::None;
    widgets::group_box(ui, "groupBox6", "Plane Feeds", group, |ui| {
        for (row, slot) in SLOTS.into_iter().enumerate() {
            let next = feed_slot_row(ui, s, group, slot, 17 + row as i32 * 27);
            if next != FeedAction::None {
                action = next;
            }
        }
    });
    ui.advance_cursor_after_rect(group);
    action
}

fn general_feed_settings(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [12, 126], [291, 151]);
    widgets::group_box(
        ui,
        "groupBox48",
        "General Plane Feed Settings",
        group,
        |ui| {
            let label = widgets::label(
                ui,
                "label22",
                "Interval [sec]:",
                designer_rect(group, [10, 26], [71, 13]),
            );
            feed_number(
                ui,
                "tb_Options_Planes_Interval",
                &mut s.planes_interval_s,
                group,
                22,
            )
            .labelled_by(label.id);
            widgets::checkbox(
                ui,
                "cb_Options_Planes_ExtendedPlausibilityCheck_Enabled",
                "Enable extended plausibility check of aircraft info",
                &mut s.planes_extended_plausibility,
                designer_rect(group, [13, 53], [278, 17]),
            );
            let text = "Max. allowed distance between \nreported and estimated position [km]:";
            let rect = designer_rect(group, [10, 80], [189, 26]);
            trace::record(ui, "label33", Kind::Text, text, rect);
            let label = widgets::put_left(
                ui,
                rect,
                egui::Label::new(egui::RichText::new(text).small()).extend(),
            );
            feed_number(ui, "int32TextBox1", &mut s.planes_max_error_dist, group, 84)
                .labelled_by(label.id);
            widgets::checkbox(
                ui,
                "cb_Options_Planes_LogErrors",
                "Log errors as warnings to AirScout's main logfile",
                &mut s.planes_log_errors,
                designer_rect(group, [12, 118], [279, 17]),
            );
        },
    );
    ui.advance_cursor_after_rect(group);
}

fn feed_number<N: Numeric>(
    ui: &mut Ui,
    name: &str,
    value: &mut N,
    group: Rect,
    y: i32,
) -> egui::Response {
    let rect = designer_rect(group, [199, y], [52, 22]);
    trace::record(ui, name, Kind::Text, "", rect);
    ui.put(
        rect,
        egui::DragValue::from_get_set(|edit| {
            if let Some(edit) = edit.filter(|edit| edit.is_finite()) {
                *value = N::from_f64(edit.round().clamp(10.0, 600.0));
            }
            value.to_f64()
        })
        .range(10..=600)
        .max_decimals(0)
        .custom_parser(|text| text.trim().parse::<i32>().ok().map(f64::from))
        .clamp_existing_to_range(false)
        .min_size(rect.size()),
    )
}

/// `groupBox40`: position lifetime and history (`AircraftDatabase_MaxDaysLifetime`,
/// `Planes_KeepHistory`). The background maintainer reads the lifetime at startup.
fn database_maintenance(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [12, 313], [291, 105]);
    widgets::group_box(ui, "groupBox40", "Database Maintenance", group, |ui| {
        let label = widgets::label(
            ui,
            "label32",
            "Keep aircraft positions for:",
            designer_rect(group, [10, 21], [129, 13]),
        );
        whole_number(
            ui,
            "ud_Options_Planes_Position_DatabaseLifetime",
            &mut s.aircraft_database_max_days_lifetime,
            1..=365,
            designer_rect(group, [199, 14], [49, 22]),
        )
        .labelled_by(label.id);
        widgets::label(
            ui,
            "label53",
            "days.",
            designer_rect(group, [254, 23], [32, 13]),
        );
        small_wrapped_label(
            ui,
            "label55",
            "Aircraft positions older than above entered value will be deleted on next startup (0 = forever).",
            designer_rect(group, [11, 43], [275, 33]),
        );
        widgets::checkbox(
            ui,
            "cb_Options_Planes_KeepHistory",
            "Keep Plane Position History",
            &mut s.planes_keep_history,
            designer_rect(group, [18, 81], [156, 17]),
        )
        .on_hover_text(
            "Check this option to keep plane positions in database for history analysis.\nCAUTION! This will need lot of CPU performance and space on disk!",
        );
    });
    ui.advance_cursor_after_rect(group);
}

/// `groupBox26`: the live position filters. The circumcircle and minimal-altitude fields edit
/// the same settings as the main window's `gb_Map_Filter`.
fn live_filters(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [309, 126], [348, 181]);
    widgets::group_box(
        ui,
        "groupBox26",
        "Plane Live Position Filters",
        group,
        |ui| {
            widgets::label(
                ui,
                "label96",
                "You can change the values here or in the Main Window.",
                designer_rect(group, [8, 22], [274, 13]),
            );
            let label = small_label(
                ui,
                "label95",
                "Maximal Path Midpoint Circumcircle\n(Filter AUTO = 0)\n(Filter OFF = -1):",
                designer_rect(group, [8, 39], [180, 42]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_Filter_Max_Circumcircle",
                &mut s.gui.planes_filter_max_circumcircle_km,
                -1.0..=1000.0,
                designer_rect(group, [192, 59], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(
                ui,
                "label94",
                "km",
                designer_rect(group, [254, 66], [21, 13]),
            );
            let label = small_label(
                ui,
                "label93",
                "Minimal Altitude \n(Filter OFF = 0:",
                designer_rect(group, [11, 87], [83, 26]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_Filter_MinAlt",
                &mut s.gui.planes_filter_min_alt_m,
                0..=12_000,
                designer_rect(group, [192, 86], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(
                ui,
                "label92",
                "m",
                designer_rect(group, [254, 92], [15, 13]),
            );
            let label = widgets::label(
                ui,
                "label91",
                "Minimal Category:",
                designer_rect(group, [8, 119], [90, 13]),
            );
            let rect = designer_rect(group, [192, 115], [77, 24]);
            trace::record(ui, "cb_Options_Planes_Filter_Min_Cat", Kind::Text, "", rect);
            let g = &mut s.gui;
            widgets::scoped(ui, rect, |ui| {
                ComboBox::from_id_salt("cb_Options_Planes_Filter_Min_Cat")
                    .width(rect.width())
                    .truncate()
                    .selected_text(format!("{:?}", g.planes_filter_min_category))
                    .show_ui(ui, |ui| {
                        for c in PlaneCategory::ALL {
                            ui.selectable_value(
                                &mut g.planes_filter_min_category,
                                c,
                                format!("{c:?}"),
                            );
                        }
                    })
                    .response
                    .labelled_by(label.id);
            });
            let label = widgets::label(
                ui,
                "labelMaxDelta",
                "Max. el. delta path to plane",
                designer_rect(group, [8, 146], [318, 17]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_MaxDelta",
                &mut s.planes_max_delta_m,
                0.0..=15_000.0,
                designer_rect(group, [192, 144], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(
                ui,
                "label220",
                "m",
                designer_rect(group, [254, 146], [15, 13]),
            );
        },
    );
    ui.advance_cursor_after_rect(group);
}

/// `groupBox38`: the altitude window and the position lifetime of the stored planes
/// (`Planes_MinAlt`, `Planes_MaxAlt`, `Planes_Position_TTL`).
fn position_filters(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [309, 313], [348, 105]);
    widgets::group_box(
        ui,
        "groupBox38",
        "Plane Position Database Filters",
        group,
        |ui| {
            let label = widgets::label(
                ui,
                "label5",
                "Minimal Altitude:",
                designer_rect(group, [8, 17], [83, 13]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_MinAlt",
                &mut s.planes_min_alt_m,
                0..=20_000,
                designer_rect(group, [192, 14], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(ui, "label9", "m", designer_rect(group, [250, 15], [15, 13]));
            let label = widgets::label(
                ui,
                "label7",
                "Maximal Altitude:",
                designer_rect(group, [8, 43], [86, 13]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_MaxAlt",
                &mut s.planes_max_alt_m,
                0..=20_000,
                designer_rect(group, [192, 39], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(ui, "label8", "m", designer_rect(group, [250, 41], [15, 13]));
            let label = small_wrapped_label(
                ui,
                "label1",
                "Discard plane when last position report is older than:",
                designer_rect(group, [8, 67], [166, 31]),
            );
            whole_number(
                ui,
                "tb_Options_Planes_Positions_TTL",
                &mut s.planes_position_ttl_min,
                0..=30,
                designer_rect(group, [192, 65], [52, 22]),
            )
            .labelled_by(label.id);
            widgets::label(
                ui,
                "label2",
                "mins.",
                designer_rect(group, [250, 69], [31, 13]),
            );
        },
    );
    ui.advance_cursor_after_rect(group);
}

fn debug_settings(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [12, 424], [645, 48]);
    widgets::group_box(ui, "groupBox36", "Debug", group, |ui| {
        widgets::checkbox(
            ui,
            "cb_Options_TracePositions",
            "Trace aircraft positions to files (Caution: this will cause a lot of files in <AirScout's Temp directory>/PlanePositions!)",
            &mut s.planes_trace_positions,
            designer_rect(group, [12, 19], [621, 29]),
        );
    });
    ui.advance_cursor_after_rect(group);
}

fn feed_slot_row(ui: &mut Ui, s: &Settings, group: Rect, slot: usize, y: i32) -> FeedAction {
    let label = widgets::label(
        ui,
        &format!("label{}", 76 + slot),
        &format!("Plane Feed {slot}:"),
        designer_rect(group, [9, y + 6], [73, 13]),
    );
    let name = format!("cb_Options_PlaneFeed{slot}");
    let rect = designer_rect(group, [91, y + 1], [317, 24]);
    trace::record(ui, &name, Kind::Text, "", rect);
    let mut action = widgets::scoped(ui, rect, |ui| {
        let (action, response) = show_feed_combo(
            ui,
            s,
            slot,
            ComboBox::from_id_salt(name).width(rect.width()).truncate(),
        );
        response.labelled_by(label.id);
        action
    });

    let mut builder = UiBuilder::new().id_salt(slot);
    if as_feeds::registry::feed_by_name(s.feed_slot(slot)).is_none() {
        builder = builder.disabled();
    }
    ui.scope_builder(builder, |ui| {
        // Fit the default egui font within the narrow Designer buttons.
        ui.spacing_mut().button_padding.x = 2.0;
        for (caption, x, width, next) in [
            ("Settings", 414, 54, FeedAction::Settings(slot)),
            ("Import", 472, 44, FeedAction::Import(slot)),
            ("Export", 520, 54, FeedAction::Export(slot)),
            ("Default", 578, 54, FeedAction::Default(slot)),
        ] {
            let name = format!("btn_Options_PlaneFeed{slot}_{caption}");
            let rect = designer_rect(group, [x, y], [width, 23]);
            trace::record(ui, &name, Kind::Text, caption, rect);
            let button = Button::new(caption).wrap_mode(TextWrapMode::Extend);
            if ui.put(rect, button).clicked() {
                action = next;
            }
        }
    });
    action
}

/// The feed combo in the first-run wizard.
pub(crate) fn feed_combo(ui: &mut Ui, s: &Settings, slot: usize) -> FeedAction {
    show_feed_combo(ui, s, slot, ComboBox::from_label(format!("Feed {slot}"))).0
}

/// Slots 2 and 3 offer a `[none]` entry (an empty slot runs no worker).
fn show_feed_combo(
    ui: &mut Ui,
    s: &Settings,
    slot: usize,
    combo: ComboBox,
) -> (FeedAction, egui::Response) {
    let mut action = FeedAction::None;
    let current = s.feed_slot(slot);
    let selected = if current.is_empty() {
        "[none]".to_string()
    } else {
        current.to_string()
    };
    let response = combo
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if slot != 1
                && ui.selectable_label(current.is_empty(), "[none]").clicked()
                && !current.is_empty()
            {
                action = FeedAction::Select(slot, String::new());
            }
            for f in &as_feeds::registry::all_feeds() {
                let r = ui.selectable_label(current == f.name(), f.name());
                if r.on_hover_text(f.info()).clicked() && current != f.name() {
                    action = FeedAction::Select(slot, f.name().to_string());
                }
            }
        })
        .response;
    (action, response)
}

/// The info text of the feed in a slot (nothing for an empty slot). The first-start wizard
/// shows it; the Options page has it only as the hover text of each feed in the combo.
pub(crate) fn feed_info(ui: &mut Ui, s: &Settings, slot: usize) {
    if let Some(f) = as_feeds::registry::feed_by_name(s.feed_slot(slot)) {
        ui.weak(f.info());
    }
}
