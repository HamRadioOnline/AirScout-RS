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
use crate::ui::forms::geometry::{anchored, designer_rect, dock_layout, Anchor, Dock};
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets::{button, button_fill, group_box, label, put_left, scoped};
use as_aircraft::enums::LifeMode;
use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui::{self, pos2, vec2, Color32, Rect, Slider};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Extra height below the Designer 137 for the Rust file list inside `gb_Analysis_Database`.
const FILE_AREA: f32 = 160.0;

/// The `BackColor` of the ON/OFF buttons in `MapDlg.cs` (active and disabled).
const ON_ACTIVE: Color32 = Color32::from_rgb(154, 205, 50); // YellowGreen
const OFF_ACTIVE: Color32 = Color32::from_rgb(240, 128, 128); // LightCoral
const ONOFF_DISABLED: Color32 = Color32::from_rgb(128, 128, 128); // Gray

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
    let rect = ui.max_rect();
    let content = vec2(
        rect.width().max(844.0),
        rect.height().max(333.0 + FILE_AREA),
    );
    egui::ScrollArea::both()
        .id_salt("analysis_scroll")
        .show(ui, |ui| {
            let layout = Rect::from_min_size(ui.cursor().min, content);
            ui.set_min_size(content);
            scoped(ui, layout, |ui| draw(ui, state, b, layout));
        });
    clear_dialog(ui.ctx(), state);
    url_dialog(ui.ctx(), state);
    plane_dialog(ui.ctx(), state);
    crossing_history::show(ui.ctx(), state);
    traffic::show(ui.ctx(), state);
}

fn draw(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, page: Rect) {
    let docks = [
        (Dock::Fill, vec2(844.0, 196.0)),
        (Dock::Top, vec2(844.0, 137.0 + FILE_AREA)),
    ];
    let rects = dock_layout(page, &docks);
    let (panel2, panel1) = (rects[0], rects[1]);
    trace::scope(ui, "panel1", panel1, |ui| panel1_body(ui, state, b, panel1));
    trace::scope(ui, "panel2", panel2, |ui| panel2_body(ui, state, b, panel2));
}

fn panel1_body(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, panel1: Rect) {
    let docks = [
        (Dock::Fill, vec2(233.0, 137.0)),
        (Dock::Left, vec2(280.0, 137.0)),
        (Dock::Left, vec2(78.0, 137.0)),
        (Dock::Right, vec2(253.0, 137.0)),
    ];
    let r = dock_layout(panel1, &docks);
    mode_group(ui, state, b, r[2]);
    database_group(ui, state, b, r[1]);
    functions_group(ui, state, b, r[0]);
    time_group(ui, state, b, r[3]);
}

fn panel2_body(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, panel2: Rect) {
    let docks = [
        (Dock::Fill, vec2(591.0, 196.0)),
        (Dock::Right, vec2(253.0, 196.0)),
    ];
    let r = dock_layout(panel2, &docks);
    player_group(ui, state, b, r[0]);
    controls_group(ui, state, b, r[1]);
}

fn mode_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, rect: Rect) {
    group_box(ui, "gb_Analysis_Mode", "Analysis", rect, |ui| {
        let history = state.life_mode == LifeMode::History;
        // Exactly one of ON/OFF is enabled per mode, with the original BackColor
        // (MapDlg.cs:6526-6539): ON is YellowGreen in LIFE, OFF is LightCoral in
        // HISTORY, the disabled one is Gray.
        let on = ui.add_enabled_ui(b.on, |ui| {
            button_fill(
                ui,
                "btn_Analysis_ON",
                "ON",
                designer_rect(rect, [8, 20], [64, 52]),
                if b.on { ON_ACTIVE } else { ONOFF_DISABLED },
            )
        });
        if on.inner.clicked() && !history {
            if let Err(e) = state.enter_history() {
                state.status = e;
            }
        }
        let off = ui.add_enabled_ui(b.off, |ui| {
            button_fill(
                ui,
                "btn_Analysis_OFF",
                "OFF",
                designer_rect(rect, [8, 76], [64, 52]),
                if b.off { OFF_ACTIVE } else { ONOFF_DISABLED },
            )
        });
        if off.inner.clicked() && history {
            state.leave_history();
        }
    });
}

fn database_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, rect: Rect) {
    let tmp = tmp_dir(state);
    if b.load && state.analysis_ui.files.is_none() {
        state.analysis_ui.files = tmp.as_deref().map(list_position_files);
        select_remembered(state);
    }
    group_box(ui, "gb_Analysis_Database", "Database", rect, |ui| {
        let load_enabled = b.load && !state.analysis_ui.selected.is_empty();
        let load = ui.add_enabled_ui(load_enabled, |ui| {
            button(
                ui,
                "btn_Analysis_Planes_Load",
                "Load Positions from File",
                designer_rect(rect, [8, 21], [128, 23]),
            )
        });
        if load.inner.clicked() {
            load_selected(state, tmp.as_deref());
        }
        let url = ui.add_enabled_ui(b.url, |ui| {
            button(
                ui,
                "btn_Analysis_Planes_History",
                "Load History from URL",
                designer_rect(rect, [142, 21], [128, 23]),
            )
        });
        if url.inner.clicked() {
            state.analysis_ui.url_dlg = Some(HistoryUrlDlg::new(&state.settings.analysis));
        }
        let save_btn = ui.add_enabled_ui(b.save, |ui| {
            button(
                ui,
                "btn_Analysis_Planes_Save",
                "Save Positions to File",
                designer_rect(rect, [8, 50], [128, 23]),
            )
        });
        if save_btn.inner.clicked() {
            save(state, tmp.as_deref());
        }
        let clear = ui.add_enabled_ui(b.clear, |ui| {
            button(
                ui,
                "btn_Analysis_Planes_Clear",
                "Clear Positions",
                designer_rect(rect, [8, 79], [128, 23]),
            )
        });
        if clear.inner.clicked() {
            state.analysis_ui.confirm_clear = true;
        }
        // tb_Analysis_Status: a TextBox (not text-checked), drawn as a plain label.
        let status = state
            .analysis_blocked()
            .map(|s| s.to_string())
            .unwrap_or_else(|| state.status_line());
        put_left(
            ui,
            designer_rect(rect, [8, 111], [262, 20]),
            egui::Label::new(status).truncate(),
        );
        // Rust-only file list, after the original controls.
        let list = Rect::from_min_max(
            pos2(rect.min.x + 8.0, rect.min.y + 135.0),
            pos2(rect.max.x - 8.0, rect.max.y - 4.0),
        );
        scoped(ui, list, |ui| file_list(ui, state, tmp.as_deref(), b));
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

fn load_selected(state: &mut AppState, tmp: Option<&Path>) {
    let Some(tmp) = tmp else {
        state.status = "No data directory".into();
        return;
    };
    let files: Vec<PathBuf> = std::mem::take(&mut state.analysis_ui.selected)
        .into_iter()
        .collect();
    if files.is_empty() {
        return;
    }
    if let Some(n) = files[0].file_name().and_then(|n| n.to_str()) {
        state.settings.analysis.load_file_name = n.to_string();
    }
    state.status = format!("Loading {} file(s) from {}", files.len(), tmp.display());
    state.run_analysis_job(AnalysisJob::LoadFiles(files));
}

fn file_list(ui: &mut egui::Ui, state: &mut AppState, tmp: Option<&Path>, b: AnalysisButtons) {
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
        let u = &mut state.analysis_ui;
        ui.selectable_value(&mut u.csv, false, "JSON");
        ui.selectable_value(&mut u.csv, true, "CSV");
    });
    let files = state.analysis_ui.files.clone().unwrap_or_default();
    if files.is_empty() {
        ui.label(format!(
            "Put *.json, *.csv or *.hist files into {}",
            tmp.display()
        ));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("analysis_files")
        .show(ui, |ui| {
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
        });
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

fn functions_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, rect: Rect) {
    group_box(ui, "gb_Analysis_Functions", "Functions", rect, |ui| {
        let traffic = ui.add_enabled_ui(b.traffic, |ui| {
            button(
                ui,
                "btn_Analysis_Planes_ShowTraffic",
                "Show All Traffic",
                designer_rect(rect, [6, 21], [113, 23]),
            )
        });
        if traffic.inner.clicked() {
            traffic::open(state);
        }
        let path = ui.add_enabled_ui(b.export_path, |ui| {
            button(
                ui,
                "btn_Analysis_Path_SaveToFile",
                "Export Path",
                designer_rect(rect, [6, 50], [113, 23]),
            )
        });
        if path.inner.clicked() {
            export_path(state);
        }
        let crossing = ui.add_enabled_ui(b.crossing, |ui| {
            button(
                ui,
                "btn_Analysis_CrossingHistory",
                "Crossing History",
                designer_rect(rect, [6, 79], [113, 23]),
            )
        });
        if crossing.inner.clicked() {
            crossing_history::open(state);
        }
        let plane = ui.add_enabled_ui(b.plane_history, |ui| {
            button(
                ui,
                "btn_Analysis_Plane_History",
                "Export Plane History",
                designer_rect(rect, [6, 108], [113, 23]),
            )
        });
        if plane.inner.clicked() {
            open_plane_dialog(state);
        }
    });
}

fn time_group(ui: &mut egui::Ui, state: &mut AppState, _b: AnalysisButtons, rect: Rect) {
    group_box(ui, "gb_Analysis_Time", "Time", rect, |ui| {
        let time = state.settings.time.offline.format(DATE_FORMAT).to_string();
        ui.put(
            designer_rect(rect, [6, 27], [242, 35]),
            egui::Label::new(time),
        );
        label(
            ui,
            "label14",
            "Stepwidth:",
            designer_rect(rect, [7, 84], [57, 13]),
        );
        let step = state
            .history
            .as_ref()
            .map_or(0, |h| h.interval_s)
            .to_string();
        ui.put(
            designer_rect(rect, [84, 82], [96, 20]),
            egui::Label::new(step),
        );
        label(
            ui,
            "label15",
            "sec.",
            designer_rect(rect, [210, 85], [27, 13]),
        );
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

fn player_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, rect: Rect) {
    group_box(ui, "gb_Analysis_Player", "Player", rect, |ui| {
        ui.add_enabled_ui(b.player, |ui| range_fields(ui, state, rect));
        ui.add_enabled_ui(b.player, |ui| time_slider(ui, state, rect));
    });
}

fn controls_group(ui: &mut egui::Ui, state: &mut AppState, b: AnalysisButtons, rect: Rect) {
    group_box(ui, "gb_Analysis_Controls", "Controls", rect, |ui| {
        // The original buttons are bitmaps (D-007): draw ⏮ ◀ ⏸ ▶ ⏭ as vector
        // bars/triangles, keep the original name as hover text and label.
        let buttons: [(&str, &str, [i32; 2], PlayerButton); 5] = [
            (
                "btn_Analysis_Rewind",
                "Rewind",
                [6, 153],
                PlayerButton::Rewind,
            ),
            ("btn_Analysis_Back", "Back", [55, 153], PlayerButton::Back),
            (
                "btn_Analysis_Pause",
                "Pause",
                [104, 153],
                PlayerButton::Pause,
            ),
            (
                "btn_Analysis_Forward",
                "Forward",
                [153, 153],
                PlayerButton::Forward,
            ),
            (
                "btn_Analysis_FastForward",
                "Fast forward",
                [202, 153],
                PlayerButton::FastForward,
            ),
        ];
        for (name, tooltip, loc, btn) in buttons {
            let resp = ui.add_enabled_ui(b.player, |ui| {
                let rect = designer_rect(rect, loc, [47, 29]);
                trace::record(ui, name, Kind::Text, "", rect);
                let resp = ui.put(rect, egui::Button::new("").min_size(vec2(47.0, 29.0)));
                let fg = if resp.enabled() {
                    ui.visuals().widgets.inactive.fg_stroke.color
                } else {
                    ui.visuals().widgets.noninteractive.fg_stroke.color
                };
                draw_player_glyph(ui.painter(), rect, btn, fg);
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, resp.enabled(), tooltip)
                });
                resp.on_hover_text(tooltip)
            });
            if resp.inner.clicked() {
                state.player(btn);
            }
        }
    });
}

/// One element of a player-button glyph (⏮ ◀ ⏸ ▶ ⏭ as bars and triangles).
#[derive(Clone, Copy)]
enum GlyphPart {
    Bar,
    TriL,
    TriR,
}

/// Draw the player symbol for `kind` (D-007) inside `rect` with the egui painter.
fn draw_player_glyph(painter: &egui::Painter, rect: Rect, kind: PlayerButton, color: Color32) {
    let scale = rect.height().min(rect.width());
    let hh = scale * 0.16; // half-height of bars and triangles
    let bar_w = scale * 0.10;
    let tri_w = scale * 0.24;
    let gap = scale * 0.08;
    let parts: &[GlyphPart] = match kind {
        PlayerButton::Rewind => &[GlyphPart::Bar, GlyphPart::TriL, GlyphPart::TriL],
        PlayerButton::Back => &[GlyphPart::TriL],
        PlayerButton::Pause => &[GlyphPart::Bar, GlyphPart::Bar],
        PlayerButton::Forward => &[GlyphPart::TriR],
        PlayerButton::FastForward => &[GlyphPart::TriR, GlyphPart::TriR, GlyphPart::Bar],
    };
    let width = |p: GlyphPart| match p {
        GlyphPart::Bar => bar_w,
        GlyphPart::TriL | GlyphPart::TriR => tri_w,
    };
    let total: f32 =
        parts.iter().map(|p| width(*p)).sum::<f32>() + gap * parts.len().saturating_sub(1) as f32;
    let cy = rect.center().y;
    let mut x = rect.center().x - total * 0.5;
    for &part in parts {
        let w = width(part);
        let cx = x + w * 0.5;
        match part {
            GlyphPart::Bar => {
                painter.rect_filled(
                    Rect::from_center_size(pos2(cx, cy), vec2(bar_w, hh * 2.0)),
                    1.0,
                    color,
                );
            }
            GlyphPart::TriL => tri(painter, cx, cy, tri_w, hh, -1.0, color),
            GlyphPart::TriR => tri(painter, cx, cy, tri_w, hh, 1.0, color),
        }
        x += w + gap;
    }
}

fn tri(painter: &egui::Painter, cx: f32, cy: f32, w: f32, hh: f32, dir: f32, color: Color32) {
    let apex_x = cx + dir * w * 0.5;
    let base_x = cx - dir * w * 0.5;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(base_x, cy - hh),
            pos2(base_x, cy + hh),
            pos2(apex_x, cy),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

fn range_fields(ui: &mut egui::Ui, state: &mut AppState, group: Rect) {
    let Some(h) = &state.history else { return };
    let range = (h.min, h.max);
    let u = &mut state.analysis_ui;
    if u.synced != Some(range) {
        u.min_text = range.0.format(DATE_FORMAT).to_string();
        u.max_text = range.1.format(DATE_FORMAT).to_string();
        u.synced = Some(range);
    }
    let mut commit = false;
    let (min_rect, max_rect, _play) = player_rects(group);
    let min_resp = ui.put(
        min_rect,
        egui::TextEdit::singleline(&mut u.min_text).desired_width(min_rect.width()),
    );
    min_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Min"));
    commit |= min_resp.lost_focus();
    let max_resp = ui.put(
        max_rect,
        egui::TextEdit::singleline(&mut u.max_text).desired_width(max_rect.width()),
    );
    max_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Max"));
    commit |= max_resp.lost_focus();
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

fn time_slider(ui: &mut egui::Ui, state: &mut AppState, group: Rect) {
    let Some(h) = &state.history else { return };
    let span = (h.max - h.min).num_seconds().max(1);
    let mut at = (state.settings.time.offline - h.min)
        .num_seconds()
        .clamp(0, span);
    let (_min, _max, play) = player_rects(group);
    ui.spacing_mut().slider_width = play.width();
    let resp = ui.put(play, Slider::new(&mut at, 0..=span).show_value(false));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Time"));
    if resp.changed() {
        state.scroll_to(at);
    }
    let marker = Rect::from_min_max(
        pos2(play.min.x, play.max.y + 1.0),
        pos2(play.max.x, play.max.y + 7.0),
    );
    marker_strip(ui, state, marker);
}

/// The anchored player date/time controls: `dtp_Analysis_MinValue`
/// (Bottom|Left), `dtp_Analysis_MaxValue` (Bottom|Right) and the stretched
/// `sb_Analysis_Play` between them (MapDlg.cs:6857-6863).
pub fn player_rects(group: Rect) -> (Rect, Rect, Rect) {
    let designer = vec2(591.0, 196.0);
    let min = anchored(
        designer,
        group,
        [6, 160],
        [123, 20],
        Anchor {
            top: false,
            bottom: true,
            left: true,
            right: false,
        },
    );
    let max = anchored(
        designer,
        group,
        [463, 160],
        [122, 20],
        Anchor {
            top: false,
            bottom: true,
            left: false,
            right: true,
        },
    );
    // MapDlg.cs:6859-6861: Left = dtp_Analysis_MinValue.Right + 10, Width =
    // dtp_Analysis_MaxValue.Left - dtp_Analysis_MinValue.Width - 20, Height =
    // dtp_Analysis_MinValue.Height (leaving the upstream's 4 px right gap).
    let max_left_in_group = max.left() - group.left();
    let play = Rect::from_min_size(
        pos2(min.right() + 10.0, min.top()),
        vec2(max_left_in_group - min.width() - 20.0, min.height()),
    );
    (min, max, play)
}

/// One tick per pixel where a report lies (`MapDlg.cs:6839-6855`), under the slider.
fn marker_strip(ui: &mut egui::Ui, state: &mut AppState, rect: Rect) {
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
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
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
