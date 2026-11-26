//! Analysis tab (`tp_Analysis`, MapDlg designer): HISTORY mode on/off, the position database,
//! the player and its time slider. Logic lives in `app/history.rs`; this file draws.

use crate::analysis::AnalysisJob;
use crate::app::AppState;
use crate::exports::HexOrCall;
use crate::player::{self, PlayerButton};
use crate::ui::dialogs::crossing_history::{self, CrossingHistoryDlg};
use crate::ui::dialogs::history_url::{HistoryUrlDlg, HistoryUrlResult};
use crate::ui::dialogs::plane_history::{PlaneHistoryDlg, PlaneHistoryResult};
use crate::ui::dialogs::traffic::{self, TrafficDlg};
use as_aircraft::enums::LifeMode;
use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui::{self, Color32, Sense, Slider};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Which controls are usable; pure function of the state, see [`analysis_enabled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisButtons {
    pub on: bool,
    pub off: bool,
    pub load: bool,
    pub save: bool,
    pub clear: bool,
    pub url: bool,
    pub traffic: bool,
    pub export_path: bool,
    pub crossing: bool,
    pub plane_history: bool,
    /// Player buttons, range fields and the time slider.
    pub player: bool,
}

/// ON only while paused in LIFE; everything else only once the history is loaded and no job
/// runs (a second job would cancel the running one silently). A maintenance job blocks ON and
/// the database jobs, not the player or OFF.
pub fn analysis_enabled(state: &AppState) -> AnalysisButtons {
    let history = state.life_mode == LifeMode::History;
    let free = state.analysis_blocked().is_none();
    let loaded = state
        .history
        .as_ref()
        .is_some_and(|h| !h.loading && !h.busy && h.data.oldest.is_some());
    let idle = free
        && state
            .history
            .as_ref()
            .is_some_and(|h| !h.loading && !h.busy);
    let data = free && loaded;
    AnalysisButtons {
        on: state.life_mode == LifeMode::Life && !state.is_playing() && free,
        off: history,
        load: idle,
        save: data,
        clear: idle,
        url: idle,
        traffic: data,
        export_path: data,
        crossing: data,
        plane_history: data,
        player: loaded,
    }
}

/// Widget state that is not part of the app logic.
#[derive(Default)]
pub struct AnalysisUi {
    /// Files of `<data>/Tmp/`; `None` until listed.
    pub(crate) files: Option<Vec<PathBuf>>,
    selected: BTreeSet<PathBuf>,
    csv: bool,
    confirm_clear: bool,
    /// The open Load History from URL dialog.
    url_dlg: Option<HistoryUrlDlg>,
    /// The open Plane History dialog; the app fills its lists when the job reports them.
    pub plane_dlg: Option<PlaneHistoryDlg>,
    /// The open Crossing History dialog.
    pub crossing_dlg: Option<CrossingHistoryDlg>,
    /// The open Traffic dialog.
    pub traffic_dlg: Option<TrafficDlg>,
    min_text: String,
    max_text: String,
    /// The range the texts show; they are refreshed when the state's range differs.
    synced: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Marker bins of the strip, valid for `bins_key`.
    bins: Vec<bool>,
    bins_key: Option<(u64, usize, DateTime<Utc>, DateTime<Utc>)>,
}

fn tmp_dir(state: &AppState) -> Option<PathBuf> {
    state.data_dir.as_ref().map(|d| d.join("Tmp"))
}

/// The `*.json`, `*.csv` and `*.hist` files of `dir`, sorted by name. Creates `dir`.
pub fn list_position_files(dir: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| ["json", "csv", "hist"].contains(&e.to_lowercase().as_str()))
        })
        .collect();
    v.sort();
    v
}

/// `Plane Positions <min> to <max>.<ext>` in `dir` (MapDlg.cs:6611-6636).
pub fn save_path(dir: &Path, min: DateTime<Utc>, max: DateTime<Utc>, csv: bool) -> PathBuf {
    let f = "%Y_%m_%d_%H_%M_%S";
    let ext = if csv { "csv" } else { "json" };
    dir.join(format!(
        "Plane Positions {} to {}.{ext}",
        min.format(f),
        max.format(f)
    ))
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let b = analysis_enabled(state);
    egui::ScrollArea::vertical().show(ui, |ui| {
        mode_group(ui, state, b);
        ui.horizontal_top(|ui| {
            database_group(ui, state, b);
            functions_group(ui, state, b);
        });
        player_group(ui, state, b);
        if let Some(why) = state.analysis_blocked() {
            ui.label(why);
        }
        ui.label(state.status_line());
    });
    clear_dialog(ui.ctx(), state);
    url_dialog(ui.ctx(), state);
    plane_dialog(ui.ctx(), state);
    crossing_history::show(ui.ctx(), state);
    traffic::show(ui.ctx(), state);
}

fn mode_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons) {
    ui.horizontal(|ui| {
        ui.strong("Mode");
        let history = state.life_mode == LifeMode::History;
        if ui
            .add_enabled(b.on || history, egui::RadioButton::new(history, "ON"))
            .clicked()
            && !history
        {
            if let Err(e) = state.enter_history() {
                state.status = e;
            }
        }
        if ui
            .add_enabled(b.off || !history, egui::RadioButton::new(!history, "OFF"))
            .clicked()
            && history
        {
            state.leave_history();
        }
    });
}

fn database_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons) {
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.strong("Database");
            let tmp = tmp_dir(state);
            if b.load && state.analysis_ui.files.is_none() {
                state.analysis_ui.files = tmp.as_deref().map(list_position_files);
                select_remembered(state);
            }
            load_list(ui, state, tmp.as_deref(), b);
            // no separator: it would stretch the group over the full width and push the
            // Functions group out of the window
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let u = &mut state.analysis_ui;
                ui.selectable_value(&mut u.csv, false, "JSON");
                ui.selectable_value(&mut u.csv, true, "CSV");
                if ui
                    .add_enabled(b.save, egui::Button::new("Save Positions"))
                    .clicked()
                {
                    save(state, tmp.as_deref());
                }
            });
            if ui
                .add_enabled(b.clear, egui::Button::new("Clear Positions"))
                .clicked()
            {
                state.analysis_ui.confirm_clear = true;
            }
            if ui
                .add_enabled(b.url, egui::Button::new("Load History from URL"))
                .clicked()
            {
                state.analysis_ui.url_dlg = Some(HistoryUrlDlg::new(&state.settings.analysis));
            }
        });
    });
}

fn select_remembered(state: &mut AppState) {
    let name = state.settings.analysis.load_file_name.clone();
    let u = &mut state.analysis_ui;
    let remembered = u.files.iter().flatten().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == name)
    });
    if let Some(p) = remembered.cloned() {
        u.selected.insert(p);
    }
}

fn load_list(ui: &mut egui::Ui, state: &mut AppState, tmp: Option<&Path>, b: AnalysisButtons) {
    let Some(tmp) = tmp else {
        ui.label("No data directory");
        return;
    };
    ui.horizontal(|ui| {
        if ui
            .add_enabled(b.load, egui::Button::new("Refresh list"))
            .clicked()
        {
            state.analysis_ui.files = Some(list_position_files(tmp));
        }
        let any = !state.analysis_ui.selected.is_empty();
        if ui
            .add_enabled(b.load && any, egui::Button::new("Load Positions"))
            .clicked()
        {
            let files: Vec<PathBuf> = std::mem::take(&mut state.analysis_ui.selected)
                .into_iter()
                .collect();
            if let Some(n) = files[0].file_name().and_then(|n| n.to_str()) {
                state.settings.analysis.load_file_name = n.to_string();
            }
            state.status = format!("Loading {} file(s) from {}", files.len(), tmp.display());
            state.run_analysis_job(AnalysisJob::LoadFiles(files));
        }
    });
    let files = state.analysis_ui.files.clone().unwrap_or_default();
    if files.is_empty() {
        ui.label(format!(
            "Put *.json, *.csv or *.hist files into {}",
            tmp.display()
        ));
    }
    for f in files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let mut on = state.analysis_ui.selected.contains(&f);
        if ui
            .add_enabled(b.load, egui::Checkbox::new(&mut on, name))
            .changed()
        {
            if on {
                state.analysis_ui.selected.insert(f);
            } else {
                state.analysis_ui.selected.remove(&f);
            }
        }
    }
}

fn save(state: &mut AppState, tmp: Option<&Path>) {
    let (Some(tmp), Some(h)) = (tmp, &state.history) else {
        state.status = "No data directory".into();
        return;
    };
    let csv = state.analysis_ui.csv;
    let (from, to) = (h.min, h.max);
    let path = save_path(tmp, from, to, csv);
    state.status = format!("Saving positions to {}", path.display());
    state.run_analysis_job(AnalysisJob::SavePositions {
        path,
        from,
        to,
        csv,
    });
    // the new file shows up in the load list
    state.analysis_ui.files = None;
}

fn functions_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons) {
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.strong("Functions");
            if ui
                .add_enabled(b.traffic, egui::Button::new("Show All Traffic"))
                .clicked()
            {
                traffic::open(state);
            }
            if ui
                .add_enabled(b.export_path, egui::Button::new("Export Path"))
                .clicked()
            {
                export_path(state);
            }
            if ui
                .add_enabled(b.crossing, egui::Button::new("Crossing History"))
                .clicked()
            {
                crossing_history::open(state);
            }
            if ui
                .add_enabled(b.plane_history, egui::Button::new("Export Plane History"))
                .clicked()
            {
                open_plane_dialog(state);
            }
        });
    });
}

/// Export Path (MapDlg.cs:6694): the table of the SINGLE path, written by a job.
fn export_path(state: &mut AppState) {
    if state.data_dir.is_none() {
        state.status = "No data directory".into();
        return;
    }
    let Some(entry) = state.single_path().cloned() else {
        state.status = "no path".into();
        return;
    };
    let s = &state.settings;
    let job = AnalysisJob::ExportPath {
        entry: Box::new(entry),
        model: s.elevation_model,
        max_alt_m: f64::from(s.planes_max_alt_m),
        my_call: s.my_call.clone(),
        dx_call: s.dx_call.clone(),
    };
    state.status = "Exporting path...".into();
    state.run_analysis_job(job);
    state.analysis_ui.files = None;
}

/// Export Plane History (MapDlg.cs:6944): the dialog for the player's range; its lists come
/// from a job.
fn open_plane_dialog(state: &mut AppState) {
    if state.data_dir.is_none() {
        state.status = "No data directory".into();
        return;
    }
    let Some(h) = &state.history else { return };
    let (from, to) = (h.min, h.max);
    let mut dlg = PlaneHistoryDlg::new(from, to);
    state.run_analysis_job(AnalysisJob::PlaneLists { from, to });
    dlg.job_id = state.current_job_id();
    state.analysis_ui.plane_dlg = Some(dlg);
}

fn plane_dialog(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.analysis_ui.plane_dlg.take() else {
        return;
    };
    let can_export = analysis_enabled(state).plane_history;
    match dlg.show(ctx, can_export, &state.status) {
        PlaneHistoryResult::Open => {}
        PlaneHistoryResult::Close => {
            // the lists job still runs: cancel it (`PlaneHistoryDlg_FormClosing`), but never a
            // job that started or ended since
            if dlg.waiting() {
                state.cancel_job(dlg.job_id);
            }
            return;
        }
        PlaneHistoryResult::Export(key) => {
            if can_export {
                let (from, to) = (dlg.from, dlg.to);
                state.status = match &key {
                    HexOrCall::Hex(k) | HexOrCall::Call(k) => format!("Exporting {k}..."),
                };
                state.run_analysis_job(AnalysisJob::ExportPlane { key, from, to });
                state.analysis_ui.files = None;
            }
        }
    }
    state.analysis_ui.plane_dlg = Some(dlg);
}

fn player_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons) {
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.strong("Player");
            let step = state.history.as_ref().map_or(0, |h| h.interval_s);
            ui.label(format!(
                "Time {}",
                state.settings.time.offline.format(DATE_FORMAT)
            ));
            ui.label(format!("Step width {step} s"));
            ui.horizontal(|ui| {
                for (btn, label) in [
                    (PlayerButton::Rewind, "Rewind"),
                    (PlayerButton::Back, "Back"),
                    (PlayerButton::Pause, "Pause"),
                    (PlayerButton::Forward, "Forward"),
                    (PlayerButton::FastForward, "Fast forward"),
                ] {
                    if ui.add_enabled(b.player, egui::Button::new(label)).clicked() {
                        state.player(btn);
                    }
                }
            });
            ui.add_enabled_ui(b.player, |ui| range_fields(ui, state));
            ui.add_enabled_ui(b.player, |ui| time_slider(ui, state));
        });
    });
}

fn range_fields(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(h) = &state.history else { return };
    let range = (h.min, h.max);
    let u = &mut state.analysis_ui;
    if u.synced != Some(range) {
        u.min_text = range.0.format(DATE_FORMAT).to_string();
        u.max_text = range.1.format(DATE_FORMAT).to_string();
        u.synced = Some(range);
    }
    let mut commit = false;
    ui.horizontal(|ui| {
        let l = ui.label("Min");
        let r = ui.text_edit_singleline(&mut u.min_text).labelled_by(l.id);
        commit |= r.lost_focus();
        let l = ui.label("Max");
        let r = ui.text_edit_singleline(&mut u.max_text).labelled_by(l.id);
        commit |= r.lost_focus();
    });
    if !commit {
        return;
    }
    let parse = |t: &str| {
        NaiveDateTime::parse_from_str(t.trim(), DATE_FORMAT)
            .map(|n| n.and_utc())
            .map_err(|_| format!("invalid date \"{t}\", expected YYYY-MM-DD HH:MM:SS"))
    };
    let result = parse(&u.min_text)
        .and_then(|a| parse(&u.max_text).map(|b| (a, b)))
        .and_then(|(a, b)| state.set_history_range(a, b));
    if let Err(e) = result {
        state.status = e;
    }
    // show what the state holds (clamped or unchanged)
    state.analysis_ui.synced = None;
}

fn time_slider(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(h) = &state.history else { return };
    let span = (h.max - h.min).num_seconds().max(1);
    let mut at = (state.settings.time.offline - h.min)
        .num_seconds()
        .clamp(0, span);
    ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(100.0);
    if ui
        .add(
            Slider::new(&mut at, 0..=span)
                .show_value(false)
                .text("Time"),
        )
        .changed()
    {
        state.scroll_to(at);
    }
    marker_strip(ui, state);
}

/// One tick per pixel where a report lies (`MapDlg.cs:6839-6855`), under the slider.
fn marker_strip(ui: &mut egui::Ui, state: &mut AppState) {
    let width = ui.spacing().slider_width;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), Sense::hover());
    let Some(h) = &state.history else { return };
    let px = rect.width().max(1.0) as usize;
    let key = Some((h.job_id, px, h.min, h.max));
    let u = &mut state.analysis_ui;
    if u.bins_key != key {
        u.bins = player::marker_bins(&h.markers, h.min, h.max, px);
        u.bins_key = key;
    }
    let color = if ui.visuals().dark_mode {
        Color32::LIGHT_BLUE
    } else {
        Color32::DARK_BLUE
    };
    for (i, _) in u.bins.iter().enumerate().filter(|(_, on)| **on) {
        let x = rect.left() + i as f32 + 0.5;
        ui.painter().line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, color),
        );
    }
}

/// Load History from URL: Start runs the download job (`history_download`).
fn url_dialog(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.analysis_ui.url_dlg.take() else {
        return;
    };
    let result = dlg.show(ctx);
    if dlg.accepted {
        state.settings.analysis.history_disclaimer_accepted = true;
    }
    match result {
        HistoryUrlResult::Open => {}
        HistoryUrlResult::Cancel => return,
        HistoryUrlResult::Start => {
            let req = state
                .data_dir
                .clone()
                .ok_or_else(|| "No data directory".to_string())
                .and_then(|d| dlg.request(&d, &state.settings));
            match req {
                Ok(req) if dlg.can_start() && analysis_enabled(state).url => {
                    dlg.store(&mut state.settings.analysis);
                    state.status = format!(
                        "Downloading history from {}",
                        crate::background::strip_query(&req.url_base)
                    );
                    state.run_analysis_job(AnalysisJob::Download(Box::new(req)));
                    state.analysis_ui.files = None;
                    return;
                }
                Ok(_) => dlg.error = Some("a job is running".into()),
                Err(e) => dlg.error = Some(e),
            }
        }
    }
    state.analysis_ui.url_dlg = Some(dlg);
}

/// The confirmation of Clear Positions. The C# also leaves `AllPositions` and the UI untouched
/// afterwards (MapDlg.cs:6673-6676); the port reloads the history instead.
fn clear_dialog(ctx: &egui::Context, state: &mut AppState) {
    if !state.analysis_ui.confirm_clear {
        return;
    }
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("clear_positions_dlg")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.heading("Clear Positions");
        ui.label("Delete all stored plane positions from the database?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some(true);
            }
            if ui.button("No").clicked() {
                answer = Some(false);
            }
        });
    });
    if let Some(yes) = answer {
        state.analysis_ui.confirm_clear = false;
        if yes && analysis_enabled(state).clear {
            state.run_analysis_job(AnalysisJob::ClearPositions);
        }
    }
}
