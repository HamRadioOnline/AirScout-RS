//! Planes tab: three feed slots, altitude window, position lifetime, filter and the plane info
//! rows. Each slot's combo opens the disclaimer and settings dialogs (handled by the Options
//! dialog).

use super::num;
use as_aircraft::PlaneCategory;
use as_settings::Settings;
use eframe::egui::{Button, ComboBox, Grid, Ui};

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
}

pub fn show(ui: &mut Ui, s: &mut Settings) -> FeedAction {
    let action = feed_controls(ui, s);
    ui.separator();
    Grid::new("opt_planes").num_columns(2).show(ui, |ui| {
        num(ui, "Min alt [m]", &mut s.planes_min_alt_m, 0..=100_000);
        num(ui, "Max alt [m]", &mut s.planes_max_alt_m, 0..=100_000);
        num(
            ui,
            "Position TTL [min]",
            &mut s.planes_position_ttl_min,
            1..=1440,
        );
        num(
            ui,
            "Max delta [m]",
            &mut s.planes_max_delta_m,
            0.0..=100_000.0,
        );
    });
    let g = &mut s.gui;
    ComboBox::from_label("Filter min category")
        .selected_text(format!("{:?}", g.planes_filter_min_category))
        .show_ui(ui, |ui| {
            for c in PlaneCategory::ALL {
                ui.selectable_value(&mut g.planes_filter_min_category, c, format!("{c:?}"));
            }
        });
    ui.separator();
    ui.strong("Plane info window");
    let w = &mut g.info_win;
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut w.metric, "Metric");
        ui.checkbox(&mut w.imperial, "Imperial");
        ui.checkbox(&mut w.position, "Position");
        ui.checkbox(&mut w.time, "Time");
        ui.checkbox(&mut w.epsilon, "Epsilon");
        ui.checkbox(&mut w.squint, "Squint");
        ui.checkbox(&mut w.track, "Track");
        ui.checkbox(&mut w.type_, "Type");
        ui.checkbox(&mut w.alt, "Alt");
        ui.checkbox(&mut w.dist, "Dist");
        ui.checkbox(&mut w.speed, "Speed");
        ui.checkbox(&mut w.angle, "Angle");
        ui.checkbox(&mut w.my_azimuth, "My azimuth");
        ui.checkbox(&mut w.my_elevation, "My elevation");
        ui.checkbox(&mut w.my_doppler, "My doppler");
        ui.checkbox(&mut w.dx_azimuth, "DX azimuth");
        ui.checkbox(&mut w.dx_elevation, "DX elevation");
        ui.checkbox(&mut w.dx_doppler, "DX doppler");
    });
    action
}

/// Three rows "Feed 1/2/3", each with a selector, a settings button and the feed's info.
fn feed_controls(ui: &mut Ui, s: &Settings) -> FeedAction {
    let mut action = FeedAction::None;
    for slot in SLOTS {
        let a = feed_slot_row(ui, s, slot);
        if action == FeedAction::None {
            action = a;
        }
    }
    action
}

fn feed_slot_row(ui: &mut Ui, s: &Settings, slot: usize) -> FeedAction {
    let mut action = FeedAction::None;
    ui.push_id(slot, |ui| {
        ui.horizontal(|ui| {
            action = feed_combo(ui, s, slot);
            let has_feed = !s.feed_slot(slot).is_empty();
            if ui
                .add_enabled(has_feed, Button::new(format!("Feed settings {slot}...")))
                .clicked()
            {
                action = FeedAction::Settings(slot);
            }
        });
        feed_info(ui, s, slot);
    });
    action
}

/// The feed combo of one slot (also used by the first-run wizard for slot 1). Slots 2 and 3
/// offer a `[none]` entry (an empty slot runs no worker); slot 1 is always a feed.
pub(crate) fn feed_combo(ui: &mut Ui, s: &Settings, slot: usize) -> FeedAction {
    let mut action = FeedAction::None;
    let current = s.feed_slot(slot);
    let selected = if current.is_empty() {
        "[none]".to_string()
    } else {
        current.to_string()
    };
    ComboBox::from_label(format!("Feed {slot}"))
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
        });
    action
}

/// The info text of the feed in a slot (nothing for an empty slot).
pub(crate) fn feed_info(ui: &mut Ui, s: &Settings, slot: usize) {
    if let Some(f) = as_feeds::registry::feed_by_name(s.feed_slot(slot)) {
        ui.weak(f.info());
    }
}
