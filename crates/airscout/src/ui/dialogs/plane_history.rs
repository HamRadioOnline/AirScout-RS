//! `PlaneHistoryDlg`: export the position history of one plane, chosen by hex or by call
//! (PlaneHistoryDlg.cs). The lists come from the `PlaneLists` job; typing in one field clears
//! the other (`cb_Hex_TextChanged`, `cb_Call_TextChanged`).

use crate::exports::HexOrCall;
use chrono::{DateTime, Utc};
use eframe::egui;

/// Entries of a drop-down shown at once; the lists can hold many thousands.
const SHOWN: usize = 200;

/// What the user did with the dialog this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaneHistoryResult {
    Open,
    Close,
    Export(HexOrCall),
}

pub struct PlaneHistoryDlg {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// The lists arrived; Export stays disabled before.
    pub loaded: bool,
    /// Id of the lists job the dialog started.
    pub job_id: u64,
    /// The lists job failed; shown instead of the waiting text.
    pub error: Option<String>,
    pub hexes: Vec<String>,
    pub calls: Vec<String>,
    pub hex: String,
    pub call: String,
}

impl PlaneHistoryDlg {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self {
            from,
            to,
            loaded: false,
            job_id: 0,
            error: None,
            hexes: Vec::new(),
            calls: Vec::new(),
            hex: String::new(),
            call: String::new(),
        }
    }

    /// The lists are still to come.
    pub fn waiting(&self) -> bool {
        !self.loaded && self.error.is_none()
    }

    /// The lists job failed: stop waiting, show why.
    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn set_lists(&mut self, hexes: Vec<String>, calls: Vec<String>) {
        (self.hexes, self.calls, self.loaded) = (hexes, calls, true);
    }

    /// The hex if one is typed, else the call (as in `btn_Export_Click`); `None` if both are empty.
    pub fn key(&self) -> Option<HexOrCall> {
        let (h, c) = (self.hex.trim(), self.call.trim());
        if !h.is_empty() {
            Some(HexOrCall::Hex(h.to_string()))
        } else if !c.is_empty() {
            Some(HexOrCall::Call(c.to_string()))
        } else {
            None
        }
    }

    /// Draw the dialog; `can_export` is false while a job runs.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        can_export: bool,
        status: &str,
    ) -> PlaneHistoryResult {
        let mut result = PlaneHistoryResult::Open;
        let mut open = true;
        egui::Window::new("Plane History")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let fmt = "%Y-%m-%d %H:%M:%S";
                ui.label(format!(
                    "Range {} to {}",
                    self.from.format(fmt),
                    self.to.format(fmt)
                ));
                if let Some(e) = &self.error {
                    ui.label(format!("error: {e}"));
                } else if !self.loaded {
                    ui.label("Collecting data, please wait...");
                }
                let enabled = self.loaded;
                if combo(ui, "Hex", &mut self.hex, &self.hexes, enabled) {
                    self.call.clear();
                }
                ui.label("or");
                if combo(ui, "Call", &mut self.call, &self.calls, enabled) {
                    self.hex.clear();
                }
                ui.horizontal(|ui| {
                    let ready = can_export && self.loaded && self.key().is_some();
                    if ui.add_enabled(ready, egui::Button::new("Export")).clicked() {
                        if let Some(k) = self.key() {
                            result = PlaneHistoryResult::Export(k);
                        }
                    }
                    if ui.button("Close").clicked() {
                        result = PlaneHistoryResult::Close;
                    }
                });
                ui.label(status);
            });
        if !open {
            result = PlaneHistoryResult::Close;
        }
        result
    }
}

/// An editable field with a drop-down of the entries that start with its text. True if the
/// text changed.
fn combo(
    ui: &mut egui::Ui,
    label: &str,
    text: &mut String,
    items: &[String],
    enabled: bool,
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            let l = ui.label(label);
            changed |= ui
                .add(egui::TextEdit::singleline(text).desired_width(90.0))
                .labelled_by(l.id)
                .changed();
            egui::ComboBox::from_label(format!("{label} list"))
                .selected_text("")
                .width(24.0)
                .show_ui(ui, |ui| {
                    let prefix = text.trim().to_uppercase();
                    let mut picked = None;
                    for it in items.iter().filter(|i| i.starts_with(&prefix)).take(SHOWN) {
                        if ui.selectable_label(it == text, it).clicked() {
                            picked = Some(it.clone());
                        }
                    }
                    if let Some(p) = picked {
                        *text = p;
                        changed = true;
                    }
                });
        });
    });
    changed
}
