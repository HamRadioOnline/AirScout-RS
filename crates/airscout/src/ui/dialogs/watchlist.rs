//! `WatchlistDlg`: pick the watchlist from all stored callsigns.

use eframe::egui::{self, ScrollArea};
use std::collections::BTreeSet;

type Station = (String, String);

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DlgResult {
    Open,
    Ok,
    Cancel,
}

/// State of the dialog: the left list (all callsigns), the right list (the new watchlist).
pub struct WatchlistDlg {
    /// All stations; `None` while the list loads.
    all: Option<Vec<Station>>,
    all_filter: String,
    all_sel: BTreeSet<Station>,
    selected: Vec<Station>,
    selected_filter: String,
    selected_sel: BTreeSet<Station>,
}

impl WatchlistDlg {
    /// A dialog whose right list starts as `current`.
    pub fn new(current: Vec<Station>) -> Self {
        let mut d = Self {
            all: None,
            all_filter: String::new(),
            all_sel: BTreeSet::new(),
            selected: current,
            selected_filter: String::new(),
            selected_sel: BTreeSet::new(),
        };
        d.selected.sort();
        d
    }

    /// The station list arrived.
    pub fn set_all(&mut self, list: Vec<Station>) {
        self.all = Some(list);
    }

    /// The right list, i.e. the watchlist to store on OK.
    pub fn selected(&self) -> &[Station] {
        &self.selected
    }

    /// Add the marked left rows that the right list lacks (`btn_Watchlist_Add_Click`).
    pub fn add(&mut self) {
        for s in std::mem::take(&mut self.all_sel) {
            if !self.selected.contains(&s) {
                self.selected.push(s);
            }
        }
        self.selected.sort();
    }

    /// Remove the marked right rows.
    pub fn remove(&mut self) {
        let sel = std::mem::take(&mut self.selected_sel);
        self.selected.retain(|s| !sel.contains(s));
    }

    pub fn remove_all(&mut self) {
        self.selected.clear();
        self.selected_sel.clear();
    }
}

/// Rows whose call starts with `filter` (case-insensitive, like the `LIKE 'x%'` filter).
fn filtered<'a>(list: &'a [Station], filter: &str) -> Vec<&'a Station> {
    let f = filter.trim().to_uppercase();
    list.iter()
        .filter(|(c, _)| f.is_empty() || c.to_uppercase().starts_with(&f))
        .collect()
}

/// One list with its filter box. Returns true if a row was double-clicked.
fn list_ui(
    ui: &mut egui::Ui,
    id: &str,
    filter_label: &str,
    filter: &mut String,
    rows: Option<&[Station]>,
    sel: &mut BTreeSet<Station>,
) -> bool {
    let mut double = false;
    let l = ui.label(filter_label);
    ui.add(egui::TextEdit::singleline(filter).desired_width(100.0))
        .labelled_by(l.id);
    match rows {
        None => {
            ui.label("loading…");
        }
        Some(list) => {
            let rows = filtered(list, filter);
            ScrollArea::vertical()
                .id_salt(id)
                .max_height(300.0)
                .show_rows(ui, ui.spacing().interact_size.y, rows.len(), |ui, range| {
                    for s in &rows[range] {
                        let marked = sel.contains(*s);
                        let r = ui.selectable_label(marked, format!("{} {}", s.0, s.1));
                        if r.clicked() {
                            if marked {
                                sel.remove(*s);
                            } else {
                                sel.insert((*s).clone());
                            }
                        }
                        if r.double_clicked() {
                            // the two clicks toggled the mark twice
                            sel.insert((*s).clone());
                            double = true;
                        }
                    }
                });
        }
    }
    double
}

/// Draw the dialog as a window.
pub fn show(ctx: &egui::Context, dlg: &mut WatchlistDlg) -> DlgResult {
    let mut result = DlgResult::Open;
    egui::Window::new("Watchlist")
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.strong("All callsigns");
                    let d = list_ui(
                        ui,
                        "all",
                        "Filter all",
                        &mut dlg.all_filter,
                        dlg.all.as_deref(),
                        &mut dlg.all_sel,
                    );
                    if d {
                        dlg.add();
                    }
                });
                ui.vertical(|ui| {
                    ui.add_space(ui.spacing().interact_size.y * 3.0);
                    ui.add_enabled_ui(dlg.all.is_some(), |ui| {
                        if ui.button("Add").clicked() {
                            dlg.add();
                        }
                    });
                    if ui.button("Remove").clicked() {
                        dlg.remove();
                    }
                    if ui.button("Remove all").clicked() {
                        dlg.remove_all();
                    }
                });
                ui.vertical(|ui| {
                    ui.strong("Watchlist");
                    let d = list_ui(
                        ui,
                        "selected",
                        "Filter watchlist",
                        &mut dlg.selected_filter,
                        Some(&dlg.selected),
                        &mut dlg.selected_sel,
                    );
                    if d {
                        dlg.remove();
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    result = DlgResult::Ok;
                }
                if ui.button("Cancel").clicked() {
                    result = DlgResult::Cancel;
                }
            });
        });
    result
}
