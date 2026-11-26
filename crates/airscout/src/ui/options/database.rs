//! Database tab: when the station and aircraft databases update.

use super::num;
use crate::maintenance::{shorten_path, DbId};
use as_settings::Settings;
use eframe::egui::{Button, Grid, Ui};

/// What a button of the Database tab asks for; the app does it (and asks first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintAction {
    DeleteElevationPaths,
    DeletePropagationPaths,
    DeleteMapTiles,
    DeleteStation,
    Maintenance(DbId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateMode {
    Never,
    OnStartup,
    Periodically,
}

/// `enabled` is false while playing or while a job runs: the buttons that act are disabled then.
/// `waiting` is the number of background workers still stopping; it is shown so the user sees why
/// the destructive buttons are disabled (#155).
pub fn show(
    ui: &mut Ui,
    s: &mut Settings,
    sizes: &[(DbId, String, f64)],
    enabled: bool,
    waiting: Option<usize>,
) -> Option<MaintAction> {
    let b = &mut s.background;
    let mut mode = if b.update_periodically {
        UpdateMode::Periodically
    } else if b.update_on_startup {
        UpdateMode::OnStartup
    } else {
        UpdateMode::Never
    };
    ui.strong("Update databases");
    ui.radio_value(&mut mode, UpdateMode::Never, "Never");
    ui.radio_value(&mut mode, UpdateMode::OnStartup, "On startup");
    ui.radio_value(&mut mode, UpdateMode::Periodically, "Periodically");
    b.update_on_startup = mode == UpdateMode::OnStartup;
    b.update_periodically = mode == UpdateMode::Periodically;
    ui.add_enabled_ui(b.update_periodically, |ui| {
        Grid::new("opt_db").num_columns(2).show(ui, |ui| {
            num(ui, "Period [min]", &mut b.update_period_min, 1..=100_000);
        });
    });
    ui.separator();
    ui.checkbox(&mut b.calculations_enable, "Background calculations");
    ui.separator();
    maintenance(ui, sizes, enabled, waiting)
}

/// Database files, sizes and the buttons of the maintenance (`tab_Options_Database_Enter`,
/// `OptionsDlg.cs:296-420`). A size is shown in MB with no decimals.
fn maintenance(
    ui: &mut Ui,
    sizes: &[(DbId, String, f64)],
    enabled: bool,
    waiting: Option<usize>,
) -> Option<MaintAction> {
    let mut action = None;
    if let Some(n) = waiting {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            if n == 1 {
                "waiting for 1 background worker to stop".to_owned()
            } else {
                format!("waiting for {n} background workers to stop")
            },
        );
    }
    ui.horizontal_wrapped(|ui| {
        for (text, a) in [
            (
                "Delete All Elevation Paths",
                MaintAction::DeleteElevationPaths,
            ),
            (
                "Delete All Propagation Paths",
                MaintAction::DeletePropagationPaths,
            ),
            ("Delete All Map Tiles", MaintAction::DeleteMapTiles),
            ("Delete Single Station", MaintAction::DeleteStation),
        ] {
            if ui.add_enabled(enabled, Button::new(text)).clicked() {
                action = Some(a);
            }
        }
    });
    ui.separator();
    ui.strong("Databases");
    if sizes.is_empty() {
        ui.label("reading database sizes ...");
    }
    Grid::new("opt_db_files").striped(true).show(ui, |ui| {
        for (db, path, mb) in sizes {
            ui.label(db.label());
            ui.label(shorten_path(path)).on_hover_text(path);
            ui.label(format!("{mb:.0} MB"));
            if ui
                .add_enabled(enabled, Button::new("Database Maintenance"))
                .on_hover_text(db.description())
                .clicked()
            {
                action = Some(MaintAction::Maintenance(*db));
            }
            ui.end_row();
        }
    });
    let total: f64 = sizes.iter().map(|(_, _, mb)| mb).sum();
    ui.label(format!("Total size: {total:.0} MB"));
    action
}
