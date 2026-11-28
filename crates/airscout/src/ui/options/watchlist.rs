//! Watchlist tab.

use super::num;
use crate::wintest_watch::options_watchlist_buttons_enabled;
use as_settings::Settings;
use eframe::egui::{Grid, Ui};

pub fn show(ui: &mut Ui, s: &mut Settings) {
    Grid::new("opt_watch").num_columns(2).show(ui, |ui| {
        num(ui, "Max entries", &mut s.watchlist_max_count, 1..=100_000);
    });
    ui.add_enabled(
        s.server.activate,
        eframe::egui::Checkbox::new(
            &mut s.watchlist_sync_with_kst,
            "Keep in sync with KST user list (needs wtKST > V3.1 and network functions activated):",
        ),
    );
    ui.label(format!("{} entries", s.watchlist.0.len()));
    let clear = ui.add_enabled(
        options_watchlist_buttons_enabled(s.watchlist_sync_with_kst),
        eframe::egui::Button::new("Clear watchlist"),
    );
    if clear.clicked() {
        s.watchlist.0.clear();
    }
}
