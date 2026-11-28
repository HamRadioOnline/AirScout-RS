//! `DatabaseMaintenanceDlg` and `DatabaseEditDlg` (read only): the tables of a database with
//! description and row count, "Delete All" and "View". Also draws the Yes/No question of every
//! maintenance job that removes data.
//!
//! Not reproduced (GUI): the C# view loads all rows into an editable grid and writes nothing
//! back; the port shows the first 10 000 rows read only.

use crate::app::AppState;
use crate::maintenance::{DbId, MaintenanceJob, VIEW_LIMIT};
use eframe::egui::{self, Align, Button, Color32, Grid, Key, RichText, TextEdit};
use egui_extras::{Column, TableBuilder};

/// What the user did in the table list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbAction {
    None,
    Close,
    Clear(String),
    View(String),
}

/// The read-only grid of one table and its search.
pub struct ViewState {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    search: String,
    found: Option<(usize, usize)>,
    scroll_to: Option<usize>,
    message: String,
}

/// The next cell after `from` (row by row) whose text contains `needle`, ignoring case; the
/// search wraps to the start and may end on `from` itself (`DatabaseEditDlg.FindNext`, :35-86).
/// The flag tells that it wrapped.
pub fn find_next(
    rows: &[Vec<String>],
    needle: &str,
    from: Option<(usize, usize)>,
) -> Option<((usize, usize), bool)> {
    if needle.is_empty() {
        return None;
    }
    let needle = needle.to_uppercase();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let total = rows.len() * width;
    let start = from.map_or(0, |(r, c)| r * width + c + 1);
    (0..total).find_map(|k| {
        let at = (start + k) % total;
        let (r, c) = (at / width, at % width);
        let hit = rows[r]
            .get(c)
            .is_some_and(|t| t.to_uppercase().contains(&needle));
        hit.then_some(((r, c), start + k >= total))
    })
}

impl ViewState {
    fn find_next(&mut self) {
        match find_next(&self.rows, &self.search, self.found) {
            Some((cell, wrapped)) => {
                self.found = Some(cell);
                self.scroll_to = Some(cell.0);
                self.message = if wrapped {
                    "Reached the end of the table, continued at the beginning".into()
                } else {
                    String::new()
                };
            }
            None => {
                self.found = None;
                self.message = format!("No occurrences found for: {}", self.search);
            }
        }
    }
}

pub struct DbMaintDlg {
    pub db: DbId,
    /// Name, description and row count of each table.
    pub tables: Vec<(String, String, u64)>,
    pub error: Option<String>,
    /// The table whose `View` job was asked for.
    pub pending_view: Option<String>,
    pub view: Option<ViewState>,
}

impl DbMaintDlg {
    pub fn new(db: DbId) -> Self {
        Self {
            db,
            tables: Vec::new(),
            error: None,
            pending_view: None,
            view: None,
        }
    }

    pub fn set_tables(&mut self, t: Vec<(String, String, u64)>) {
        self.error = None;
        self.tables = t;
    }

    pub fn fail(&mut self, e: String) {
        self.pending_view = None;
        self.error = Some(e);
    }

    pub fn set_view(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        let Some(table) = self.pending_view.take() else {
            return;
        };
        self.error = None;
        self.view = Some(ViewState {
            table,
            columns,
            rows,
            search: String::new(),
            found: None,
            scroll_to: None,
            message: String::new(),
        });
    }

    /// Draw the table list (and the view, if open). `enabled`: the buttons that act.
    pub fn show(&mut self, ctx: &egui::Context, enabled: bool) -> DbAction {
        let mut action = DbAction::None;
        let r = egui::Modal::new(egui::Id::unique("db_maintenance_dlg")).show(ctx, |ui| {
            ui.set_width(760.0);
            ui.heading(self.db.label());
            ui.label(self.db.description());
            ui.separator();
            if self.tables.is_empty() && self.error.is_none() {
                ui.label("reading tables ...");
            }
            Grid::new("db_tables").striped(true).show(ui, |ui| {
                for h in ["Table", "Description", "Rows", "", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for (name, desc, rows) in &self.tables {
                    ui.label(name);
                    ui.label(desc);
                    ui.label(rows.to_string());
                    if ui.add_enabled(enabled, Button::new("Delete All")).clicked() {
                        action = DbAction::Clear(name.clone());
                    }
                    if ui.add_enabled(enabled, Button::new("View")).clicked() {
                        action = DbAction::View(name.clone());
                    }
                    ui.end_row();
                }
            });
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.separator();
            if ui.button("Close").clicked() {
                action = DbAction::Close;
            }
        });
        if self.view.is_none() && r.should_close() {
            action = DbAction::Close;
        }
        if let Some(v) = &mut self.view {
            if show_view(ctx, v) {
                self.view = None;
            }
        }
        action
    }
}

/// The read-only grid with Find Next (F3). True once it was closed.
fn show_view(ctx: &egui::Context, v: &mut ViewState) -> bool {
    let mut close = false;
    let r = egui::Modal::new(egui::Id::unique("db_view_dlg")).show(ctx, |ui| {
        ui.set_width(860.0);
        ui.heading(format!("{} (read only)", v.table));
        if v.rows.len() >= VIEW_LIMIT {
            ui.label(format!("Only the first {VIEW_LIMIT} rows are shown."));
        }
        ui.horizontal(|ui| {
            let l = ui.label("Search:");
            let r = ui
                .add(TextEdit::singleline(&mut v.search).desired_width(220.0))
                .labelled_by(l.id);
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let f3 = ui.input(|i| i.key_pressed(Key::F3));
            if ui.button("Find Next").clicked() || enter || f3 {
                v.find_next();
            }
            ui.label(&v.message);
        });
        ui.separator();
        let h = 18.0;
        let found = v.found;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .max_scroll_height(360.0)
            .columns(
                Column::auto().at_least(50.0).resizable(true),
                v.columns.len(),
            );
        if let Some(r) = v.scroll_to.take() {
            table = table.scroll_to_row(r, Some(Align::Center));
        }
        table
            .header(h, |mut head| {
                for c in &v.columns {
                    head.col(|ui| {
                        ui.strong(c);
                    });
                }
            })
            .body(|body| {
                body.rows(h, v.rows.len(), |mut row| {
                    let i = row.index();
                    for (j, cell) in v.rows[i].iter().enumerate() {
                        row.col(|ui| {
                            if found == Some((i, j)) {
                                ui.label(
                                    RichText::new(cell)
                                        .background_color(Color32::YELLOW)
                                        .color(Color32::BLACK),
                                );
                            } else {
                                ui.label(cell);
                            }
                        });
                    }
                });
            });
        ui.separator();
        close = ui.button("Close").clicked();
    });
    close || r.should_close()
}

/// The maintenance dialogs and the question, drawn over the Options dialog; the question last,
/// so it is on top.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let enabled = state.maintenance_allowed();
    if let Some(mut dlg) = state.maint.db_dlg.take() {
        let action = dlg.show(ctx, enabled);
        let db = dlg.db;
        let job = match &action {
            DbAction::Clear(t) => Some(MaintenanceJob::ClearTable(db, t.clone())),
            DbAction::View(t) => {
                dlg.pending_view = Some(t.clone());
                Some(MaintenanceJob::View(db, t.clone()))
            }
            _ => None,
        };
        let open = action != DbAction::Close;
        state.maint.db_dlg = open.then_some(dlg);
        if let Some(job) = job {
            if let Err(e) = state.request_maintenance(job) {
                if let Some(d) = &mut state.maint.db_dlg {
                    d.fail(e);
                }
            }
        }
    }
    super::delete_station::show(ctx, state);
    confirmation(ctx, state);
}

/// The Yes/No question of a job that removes data (`MessageBox ... YesNo`).
fn confirmation(ctx: &egui::Context, state: &mut AppState) {
    let Some((title, text)) = state
        .maint
        .confirm
        .as_ref()
        .and_then(MaintenanceJob::confirmation)
    else {
        return;
    };
    let (mut yes, mut no) = (false, false);
    let r = egui::Modal::new(egui::Id::unique("maint_confirm")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(title);
        ui.label(text);
        ui.horizontal(|ui| {
            yes = ui.button("Yes").clicked();
            no = ui.button("No").clicked();
        });
    });
    if yes {
        if let Err(e) = state.confirm_maintenance() {
            state.status = e;
        }
    } else if no || r.should_close() {
        state.cancel_confirmation();
    }
}
