//! `HistoryFromURLDlg`: URL, directory, date and step of a history archive download.
//! There is no default URL; Start stays disabled until one is entered and the disclaimer of
//! the archive server is accepted.

use super::disclaimer::{DisclaimerDlg, DisclaimerResult};
use crate::history_download::{DownloadRequest, Filter};
use as_settings::{Analysis, Settings};
use chrono::NaiveDate;
use eframe::egui;
use std::path::{Path, PathBuf};

const DATE_FORMAT: &str = "%Y-%m-%d";

/// Shown before the first use (label of `HistoryFromURLDlg.Designer.cs:94`).
pub const DISCLAIMER: &str = "The history archive is provided by a third party that you name. \
The terms of that server apply, not those of AirScout-RS. A day archive can be about 8 GB and \
the download needs about 50 GB of free disk space. Download only from servers you may use.";

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryUrlResult {
    Open,
    Start,
    Cancel,
}

/// The fields of the dialog.
pub struct HistoryUrlDlg {
    pub url: String,
    /// Empty means `<data>/Tmp/History`.
    pub dir: String,
    pub date: String,
    pub step_min: u32,
    pub accepted: bool,
    disclaimer: Option<DisclaimerDlg>,
    pub error: Option<String>,
}

impl HistoryUrlDlg {
    pub fn new(a: &Analysis) -> Self {
        Self {
            url: a.history_url.clone(),
            dir: a.history_directory.clone(),
            date: a.history_date.format(DATE_FORMAT).to_string(),
            step_min: a.history_step_min.max(1),
            accepted: a.history_disclaimer_accepted,
            disclaimer: (!a.history_disclaimer_accepted)
                .then(|| DisclaimerDlg::new("History archive", DISCLAIMER)),
            error: None,
        }
    }

    /// Start needs a URL and the accepted disclaimer.
    pub fn can_start(&self) -> bool {
        self.accepted && !self.url.trim().is_empty()
    }

    /// The request for `data_dir`, or why the fields are not usable.
    pub fn request(&self, data_dir: &Path, s: &Settings) -> Result<DownloadRequest, String> {
        let date = NaiveDate::parse_from_str(self.date.trim(), DATE_FORMAT)
            .map_err(|_| "date must be YYYY-MM-DD".to_string())?;
        let dir = match self.dir.trim() {
            "" => data_dir.join("Tmp").join("History"),
            d => PathBuf::from(d),
        };
        Ok(DownloadRequest {
            url_base: self.url.trim().to_string(),
            date,
            step_min: self.step_min.max(1),
            dir,
            filter: Filter::from_settings(s),
        })
    }

    /// Remember the fields (the settings keep them for the next time).
    pub fn store(&self, a: &mut Analysis) {
        a.history_url = self.url.trim().to_string();
        a.history_directory = self.dir.trim().to_string();
        if let Ok(d) = NaiveDate::parse_from_str(self.date.trim(), DATE_FORMAT) {
            a.history_date = d;
        }
        a.history_step_min = self.step_min.max(1);
        a.history_disclaimer_accepted = self.accepted;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> HistoryUrlResult {
        if let Some(d) = &self.disclaimer {
            match d.show(ctx) {
                DisclaimerResult::Open => {}
                DisclaimerResult::Accept => {
                    self.accepted = true;
                    self.disclaimer = None;
                }
                DisclaimerResult::Decline => return HistoryUrlResult::Cancel,
            }
            return HistoryUrlResult::Open;
        }
        let mut result = HistoryUrlResult::Open;
        egui::Modal::new(egui::Id::unique("history_url_dlg")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading("Load History from URL");
            egui::Grid::new("history_url_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    let l = ui.label("URL");
                    ui.text_edit_singleline(&mut self.url).labelled_by(l.id);
                    ui.end_row();
                    let l = ui.label("Directory");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dir).hint_text("<data>/Tmp/History"),
                    )
                    .labelled_by(l.id);
                    ui.end_row();
                    let l = ui.label("Date");
                    ui.add(egui::TextEdit::singleline(&mut self.date).hint_text("YYYY-MM-DD"))
                        .labelled_by(l.id);
                    ui.end_row();
                    let l = ui.label("Step [min]");
                    ui.add(egui::DragValue::new(&mut self.step_min).range(1..=1440))
                        .labelled_by(l.id);
                    ui.end_row();
                });
            ui.label("A day archive can be about 8 GB; about 50 GB of free disk space are needed.");
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.can_start(), egui::Button::new("Start"))
                    .on_disabled_hover_text("Enter a URL and accept the disclaimer")
                    .clicked()
                {
                    result = HistoryUrlResult::Start;
                }
                if ui.button("Cancel").clicked() {
                    result = HistoryUrlResult::Cancel;
                }
            });
        });
        result
    }
}
