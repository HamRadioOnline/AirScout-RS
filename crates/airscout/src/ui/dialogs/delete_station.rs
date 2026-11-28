//! `DeleteSingleStationDlg`: find a station by call and remove one call/locator combination,
//! with its QRV entries, from the local station database.
//!
//! The C# Delete button has no handler (finding); the port deletes, after a question.

use crate::app::AppState;
use crate::maintenance::{locations_pattern, MaintenanceJob};
use eframe::egui::{self, Button, ComboBox, TextEdit};

/// Verbatim `label1.Text` of the resource file.
pub const HELP: &str = "Use this dialog to find and remove a single station/locator combination from the local station database. \nCAUTION: You can only remove own entries permanently. If a station/locator combination is part of the global database it would be re-inserted on next update.";

#[derive(Default)]
pub struct DeleteStationDlg {
    /// The call field, upper-cased and trimmed.
    pub call: String,
    /// The chosen locator.
    pub loc: String,
    /// The stations that match the call field (`call`, `loc`).
    pub found: Vec<(String, String)>,
    /// The pattern the `found` list answers; `None` asks for a new query.
    queried: Option<String>,
    pub error: Option<String>,
}

impl DeleteStationDlg {
    /// The pattern to look up, if the field has changed and holds two characters.
    pub fn wanted_query(&self) -> Option<String> {
        let p = locations_pattern(&self.call)?;
        (self.queried.as_deref() != Some(&p)).then_some(p)
    }

    pub fn set_locations(&mut self, v: Vec<(String, String)>) {
        self.found = v;
        self.queried = locations_pattern(&self.call);
        self.error = None;
        self.fix_loc();
    }

    /// Keep the locator among those of the chosen call, else take the first.
    fn fix_loc(&mut self) {
        if !self.locs().any(|l| *l == self.loc) {
            let first = self.locs().next().cloned().unwrap_or_default();
            self.loc = first;
        }
    }

    /// A station was deleted: look again.
    pub fn deleted(&mut self) {
        self.queried = None;
        self.loc.clear();
    }

    /// The locators of the chosen call.
    pub fn locs(&self) -> impl Iterator<Item = &String> {
        self.found
            .iter()
            .filter(|(c, _)| *c == self.call)
            .map(|(_, l)| l)
    }

    fn calls(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.found.iter().map(|(c, _)| c.as_str()).collect();
        v.dedup();
        v
    }
}

/// Draw the dialog; asks for the delete through the app (which asks the user first).
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.maint.station_dlg.take() else {
        return;
    };
    let enabled = state.maintenance_allowed();
    let (mut delete, mut close) = (false, false);
    let r = egui::Modal::new(egui::Id::unique("delete_station_dlg")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading("Delete Single Station");
        ui.label(HELP);
        ui.separator();
        egui::Grid::new("del_station")
            .num_columns(2)
            .show(ui, |ui| {
                let l = ui.label("Call:");
                let r = ui
                    .add(TextEdit::singleline(&mut dlg.call).desired_width(160.0))
                    .labelled_by(l.id);
                if r.changed() {
                    dlg.call = dlg.call.to_uppercase().trim().to_string();
                }
                ui.end_row();
                let l = ui.label("Matches:");
                ComboBox::from_id_salt("del_calls")
                    .selected_text(if dlg.calls().is_empty() {
                        "-"
                    } else {
                        "select a call"
                    })
                    .show_ui(ui, |ui| {
                        let calls: Vec<String> =
                            dlg.calls().into_iter().map(String::from).collect();
                        for c in calls {
                            if ui.selectable_label(dlg.call == c, &c).clicked() {
                                dlg.call = c;
                            }
                        }
                    })
                    .response
                    .labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Locator:");
                let locs: Vec<String> = dlg.locs().cloned().collect();
                ComboBox::from_id_salt("del_locs")
                    .selected_text(dlg.loc.clone())
                    .show_ui(ui, |ui| {
                        for loc in locs {
                            ui.selectable_value(&mut dlg.loc, loc.clone(), loc);
                        }
                    })
                    .response
                    .labelled_by(l.id);
                ui.end_row();
            });
        if let Some(e) = &dlg.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        ui.separator();
        ui.horizontal(|ui| {
            let ok = enabled && !dlg.call.is_empty() && !dlg.loc.is_empty();
            delete = ui.add_enabled(ok, Button::new("Delete")).clicked();
            close = ui.button("Cancel").clicked();
        });
    });
    // the locators of another call are no choice
    dlg.fix_loc();
    if close || r.should_close() {
        return;
    }
    if delete {
        let job = MaintenanceJob::DeleteStation {
            call: dlg.call.clone(),
            loc: dlg.loc.clone(),
        };
        if let Err(e) = state.request_maintenance(job) {
            dlg.error = Some(e);
        }
    }
    if let Some(p) = dlg.wanted_query() {
        if state
            .request_maintenance(MaintenanceJob::Locations(p))
            .is_ok()
        {
            // answered by `set_locations`; until then the list is as it was
            dlg.queried = locations_pattern(&dlg.call);
        }
    }
    state.maint.station_dlg = Some(dlg);
}
