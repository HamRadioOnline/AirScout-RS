//! `SetTimeDlg` (MapDlg.cs:3529): the time source and the offline time (UTC).

use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui;

const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// The values being edited.
pub struct SetTimeDlg {
    pub online: bool,
    pub text: String,
}

/// What the user did with the dialog this frame.
pub enum SetTimeResult {
    Open,
    Ok(bool, DateTime<Utc>),
    Cancel,
}

impl SetTimeDlg {
    pub fn new(online: bool, offline: DateTime<Utc>) -> Self {
        Self {
            online,
            text: offline.format(FORMAT).to_string(),
        }
    }

    /// The edited offline time, if the text is a valid UTC date and time.
    pub fn parsed(&self) -> Option<DateTime<Utc>> {
        NaiveDateTime::parse_from_str(self.text.trim(), FORMAT)
            .ok()
            .map(|t| t.and_utc())
    }

    pub fn show(&mut self, ctx: &egui::Context) -> SetTimeResult {
        let mut result = SetTimeResult::Open;
        egui::Modal::new(egui::Id::unique("set_time_dlg")).show(ctx, |ui| {
            ui.heading("Set time");
            ui.checkbox(&mut self.online, "Online (system clock)");
            ui.horizontal(|ui| {
                ui.label("Offline time (UTC, yyyy-mm-dd hh:mm:ss)");
                ui.text_edit_singleline(&mut self.text);
            });
            let parsed = self.parsed();
            if parsed.is_none() {
                ui.colored_label(egui::Color32::LIGHT_RED, "invalid date and time");
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(parsed.is_some(), egui::Button::new("OK"))
                    .clicked()
                {
                    if let Some(t) = parsed {
                        result = SetTimeResult::Ok(self.online, t);
                    }
                }
                if ui.button("Cancel").clicked() {
                    result = SetTimeResult::Cancel;
                }
            });
        });
        result
    }
}
