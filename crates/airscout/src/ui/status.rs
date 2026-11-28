//! Status bar (`ss_Main`): status text, calculation counts, database status, the eight database
//! LEDs, the TRK, ROT and CAT fields, and the Rust-only status fields (D-010).

use crate::app::AppState;
use crate::background::{led_color, Led, LedState};
use crate::cat_service::cat_colors;
use crate::ui::forms::trace::{self, Kind};
use as_elevation::ElevationModel;
use eframe::egui::{self, vec2, Color32, Rect, RichText, Sense};

/// C# `Color.Plum` (MapDlg.cs:3461).
pub const ALARM_PLUM: Color32 = Color32::from_rgb(221, 160, 221);

/// Height of the status strip (`ss_Main`, `StatusStrip.Height`).
pub const STATUS_HEIGHT: f32 = 24.0;

/// The eight database LEDs in Designer order (`ss_Main` indices 4..=11): the trace name, the
/// Designer `Text`, and the [`Led`] that supplies live status. The Rig LED has no Rust
/// counterpart and keeps a fixed default state and tooltip.
const LEDS: [(&str, &str, Option<Led>); 8] = [
    (
        "tsl_Database_LED_Aircraft",
        "Aircraft database status LED",
        Some(Led::Aircraft),
    ),
    (
        "tsl_Database_LED_Stations",
        "Station database status LED",
        Some(Led::Stations),
    ),
    ("tsl_Database_LED_Rig", "Rig database status LED", None),
    (
        "tsl_Database_LED_GLOBE",
        "GLOBE database status LED",
        Some(Led::Elevation(ElevationModel::Globe)),
    ),
    (
        "tsl_Database_LED_SRTM3",
        "SRTM3 database status LED",
        Some(Led::Elevation(ElevationModel::Srtm3)),
    ),
    (
        // D-011: the upstream text says "SRTM3", a copy-paste slip.
        "tsl_Database_LED_SRTM1",
        "SRTM1 database status LED",
        Some(Led::Elevation(ElevationModel::Srtm1)),
    ),
    (
        "tsl_Database_LED_ASTER3",
        "ASTER3 database status LED",
        Some(Led::Elevation(ElevationModel::Aster3)),
    ),
    (
        "tsl_Database_LED_ASTER1",
        "ASTER1 database status LED",
        Some(Led::Elevation(ElevationModel::Aster1)),
    ),
];

/// Draw the status strip at `rect`, in Designer order from left to right.
pub fn show(ui: &mut egui::Ui, state: &AppState, rect: Rect) {
    trace::scope(ui, "ss_Main", rect, |ui| {
        let top = rect.min.y;
        let mut fixed = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        fixed.shrink_clip_rect(rect);
        feed(&mut fixed, state, top);
        planes(&mut fixed, state, top);
        if let Some(e) = &state.settings_error {
            let text = format!("Settings not loaded, defaults in use and not saved: {e}");
            let resp = fixed.add(egui::Label::new(
                RichText::new(&text).color(Color32::LIGHT_RED),
            ));
            record(&fixed, "tsl_SettingsError", &text, resp.rect, top);
        }
        cat(&mut fixed, state, top);
        rot(&mut fixed, state, top);
        trk(&mut fixed, state, top);
        for (name, text, led_opt) in LEDS.iter().rev() {
            let (led_status, live) = led_state(state, *led_opt);
            let tooltip = match (led_opt, live) {
                (Some(led), Some(live)) => format!("{} Database Status\n\n{live}", led.label()),
                _ => text.to_string(),
            };
            led(&mut fixed, name, text, led_status, tooltip, top);
        }
        label(&mut fixed, "tsl_Database", "Database status.", top);
        let calculations = if state.calculations.is_empty() {
            "Calculations."
        } else {
            &state.calculations
        };
        label(&mut fixed, "tsl_Calculations", calculations, top);

        // tsl_Status springs across the space left by the right-aligned items.
        let mut spring = rect;
        spring.max.x = fixed.available_rect_before_wrap().max.x.max(rect.min.x);
        let (status, alarm) = status_text(state);
        if spring.width() > 0.0 {
            let text = if alarm {
                RichText::new(&status)
                    .strong()
                    .background_color(ALARM_PLUM)
                    .color(Color32::BLACK)
            } else {
                RichText::new(&status)
            };
            let mut message = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(spring)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            message.shrink_clip_rect(spring);
            message.add(egui::Label::new(text).truncate());
        }
        record(ui, "tsl_Status", &status, spring, top);
    });
}

/// Record a status item at the strip top, so the fidelity order check sorts by x alone (the
/// drawn items are vertically centred and would otherwise fall into different 10 pt buckets).
fn record(ui: &egui::Ui, name: &str, text: &str, drawn: Rect, top: f32) {
    let rect = Rect::from_min_max(
        egui::pos2(drawn.min.x, top),
        egui::pos2(drawn.max.x, top + STATUS_HEIGHT),
    );
    trace::record(ui, name, Kind::Text, text, rect);
}

/// The text of `tsl_Status` and whether it is an alarm (Plum background).
fn status_text(state: &AppState) -> (String, bool) {
    if let Some(a) = state.alarm_text() {
        return (a, true);
    }
    let s = state.status_line();
    if s.is_empty() {
        ("No Messages.".to_string(), false)
    } else {
        (s, false)
    }
}

/// The state and live status text of `led`, defaulting to `Unknown` with no live text.
fn led_state(state: &AppState, led: Option<Led>) -> (LedState, Option<&str>) {
    led.and_then(|l| {
        state
            .leds
            .iter()
            .find(|(x, _, _)| *x == l)
            .map(|(_, s, text)| (*s, (!text.is_empty()).then_some(text.as_str())))
    })
    .unwrap_or_default()
}

/// A plain status label, recorded under its Designer name.
fn label(ui: &mut egui::Ui, name: &str, text: &str, top: f32) {
    let resp = ui.add(egui::Label::new(text));
    record(ui, name, text, resp.rect, top);
}

/// A coloured field (`TRK`, `ROT`), recorded under its Designer name.
fn field(ui: &mut egui::Ui, name: &str, text: &str, (fg, bg): ([u8; 3], [u8; 3]), top: f32) {
    let resp = ui.add(egui::Label::new(
        RichText::new(text)
            .color(Color32::from_rgb(fg[0], fg[1], fg[2]))
            .background_color(Color32::from_rgb(bg[0], bg[1], bg[2])),
    ));
    record(ui, name, text, resp.rect, top);
}

fn trk(ui: &mut egui::Ui, state: &AppState, top: f32) {
    field(
        ui,
        "tsl_Track",
        "TRK",
        crate::tracking::status_colors(state.track_status),
        top,
    );
}

fn rot(ui: &mut egui::Ui, state: &AppState, top: f32) {
    field(
        ui,
        "tsl_Rot",
        "ROT",
        crate::tracking::rot_colors(state.rot_status),
        top,
    );
}

fn cat(ui: &mut egui::Ui, state: &AppState, top: f32) {
    let (fg, bg) = cat_colors(state.cat_status);
    let resp = ui
        .add(
            egui::Label::new(
                RichText::new("CAT")
                    .color(Color32::from_rgb(fg[0], fg[1], fg[2]))
                    .background_color(Color32::from_rgb(bg[0], bg[1], bg[2])),
            )
            .sense(egui::Sense::hover()),
        )
        .on_hover_text(state.cat_tooltip());
    record(ui, "tsl_CAT", "CAT", resp.rect, top);
}

/// A database LED whose tooltip shows its current status and whose trace uses Designer text.
fn led(ui: &mut egui::Ui, name: &str, text: &str, state: LedState, tooltip: String, top: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 5.0, led_color(state));
    resp.on_hover_text(tooltip);
    record(ui, name, text, rect, top);
}

fn planes(ui: &mut egui::Ui, state: &AppState, top: f32) {
    let text = format!(
        "Planes: {} shown, {} active",
        state.tick.shown.len(),
        state.tick.active.len()
    );
    label(ui, "tsl_Planes", &text, top);
}

fn feed(ui: &mut egui::Ui, state: &AppState, top: f32) {
    let text = format!("Feed: {}", state.feed_status);
    label(ui, "tsl_Feed", &text, top);
}
