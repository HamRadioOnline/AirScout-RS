//! Status bar (`ss_Main`): status text, calculation counts, feed state, TRK, ROT and CAT fields.

use crate::app::AppState;
use crate::background::led_color;
use crate::cat_service::cat_colors;
use eframe::egui::{self, Color32, RichText};

/// C# `Color.Plum` (MapDlg.cs:3461).
pub const ALARM_PLUM: Color32 = Color32::from_rgb(221, 160, 221);

pub fn show(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        if let Some(e) = &state.settings_error {
            ui.label(
                RichText::new(format!(
                    "Settings not loaded, defaults in use and not saved: {e}"
                ))
                .color(Color32::LIGHT_RED),
            );
            ui.separator();
        }
        if let Some(a) = state.alarm_text() {
            ui.label(
                RichText::new(a)
                    .strong()
                    .background_color(ALARM_PLUM)
                    .color(Color32::BLACK),
            );
            ui.separator();
        }
        ui.label(state.status_line());
        ui.separator();
        ui.label(format!(
            "Planes: {} shown, {} active",
            state.tick.shown.len(),
            state.tick.active.len()
        ));
        ui.separator();
        if !state.calculations.is_empty() {
            ui.label(&state.calculations);
            ui.separator();
        }
        ui.label(format!("Feed: {}", state.feed_status));
        ui.separator();
        let field = |ui: &mut egui::Ui, text: &str, (fg, bg): ([u8; 3], [u8; 3])| {
            let r = ui.label(
                RichText::new(text)
                    .color(Color32::from_rgb(fg[0], fg[1], fg[2]))
                    .background_color(Color32::from_rgb(bg[0], bg[1], bg[2])),
            );
            ui.separator();
            r
        };
        field(
            ui,
            "TRK",
            crate::tracking::status_colors(state.track_status),
        );
        field(ui, "ROT", crate::tracking::rot_colors(state.rot_status));
        // `ss_Main` order TRK, ROT, CAT (MapDlg.Designer.cs:1100-1102)
        field(ui, "CAT", cat_colors(state.cat_status)).on_hover_text(state.cat_tooltip());
        for (led, st, text) in &state.leds {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 5.0, led_color(*st));
            resp.on_hover_text(format!("{}: {text}", led.label()));
            ui.label(led.label());
        }
    });
}
