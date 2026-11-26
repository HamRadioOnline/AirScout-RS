//! `CrossingHistoryDlg`: calculate all crossings of the SINGLE path in the loaded history, show
//! them per second as a bar chart and export them as CSV (CrossingHistoryDlg.cs). The work is
//! done by the jobs `Crossings` and `ExportCrossings` (`crossing_history.rs`).

use crate::analysis::AnalysisJob;
use crate::app::AppState;
use crate::crossing_history::{crossing_range, CrossingParams, CrossingResult, STEP_S};
use crate::ui::analysis::analysis_enabled;
use as_settings::Analysis;
use chrono::{DateTime, Utc};
use eframe::egui::{self, Color32};
use egui_plot::{Bar, BarChart, Plot};
use std::sync::Arc;

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossingHistoryResult {
    Open,
    /// "Back" or the window's close button.
    Back,
    Calculate,
    Export,
}

pub struct CrossingHistoryDlg {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub step_s: i64,
    /// Id of the job the dialog started last.
    pub job_id: u64,
    /// The calculation runs.
    pub running: bool,
    /// The last calculation failed.
    pub error: Option<String>,
    /// The crossings of the last complete calculation; Export needs them.
    pub result: Option<Arc<CrossingResult>>,
    title: String,
}

impl CrossingHistoryDlg {
    /// The constructor of the C# dialog (:38-57) for the player range `min..max`.
    pub fn new(
        min: DateTime<Utc>,
        max: DateTime<Utc>,
        oldest: Option<DateTime<Utc>>,
        my_call: &str,
        dx_call: &str,
    ) -> Self {
        let (from, to, step_s) = crossing_range(min, max, oldest, STEP_S);
        Self {
            from,
            to,
            step_s,
            job_id: 0,
            running: false,
            error: None,
            result: None,
            title: format!("Path Crossing History: {my_call} >>> {dx_call}"),
        }
    }

    /// A calculation was started as job `id`; an earlier result is dropped (the chart is cleared).
    pub fn begin(&mut self, id: u64) {
        self.job_id = id;
        self.running = true;
        self.error = None;
        self.result = None;
    }

    pub fn set_result(&mut self, r: CrossingResult) {
        self.running = false;
        self.result = Some(Arc::new(r));
    }

    pub fn fail(&mut self, error: String) {
        self.running = false;
        self.error = Some(error);
    }

    /// Draw the dialog. `idle` is true while no other job runs; the two settings of the dialog
    /// are edited in place.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        idle: bool,
        analysis: &mut Analysis,
        status: &str,
    ) -> CrossingHistoryResult {
        let mut out = CrossingHistoryResult::Open;
        let mut open = true;
        egui::Window::new(self.title.clone())
            .open(&mut open)
            .collapsible(false)
            .default_width(640.0)
            .show(ctx, |ui| {
                let fmt = "%Y-%m-%d %H:%M:%S";
                ui.label(format!(
                    "Range {} to {}, step {} s",
                    self.from.format(fmt),
                    self.to.format(fmt),
                    self.step_s
                ));
                ui.checkbox(
                    &mut analysis.crossing_with_signal_level,
                    "Consider crossings with recorded signal level only",
                );
                ui.horizontal(|ui| {
                    let l = ui.label("Timespan within two crossings considered as ambiguous:");
                    ui.add(egui::DragValue::new(&mut analysis.crossing_ambiguous_gap_s))
                        .labelled_by(l.id);
                    ui.label("secs.");
                });
                ui.horizontal(|ui| {
                    let calc =
                        ui.add_enabled(idle && !self.running, egui::Button::new("Calculate"));
                    if calc.clicked() {
                        out = CrossingHistoryResult::Calculate;
                    }
                    let can_export = idle && !self.running && self.result.is_some();
                    if ui
                        .add_enabled(can_export, egui::Button::new("Export to CSV"))
                        .clicked()
                    {
                        out = CrossingHistoryResult::Export;
                    }
                    if ui.button("Back").clicked() {
                        out = CrossingHistoryResult::Back;
                    }
                });
                if let Some(e) = &self.error {
                    ui.label(format!("error: {e}"));
                }
                if let Some(r) = &self.result {
                    chart(ui, r);
                }
                ui.label(status);
            });
        if !open {
            out = CrossingHistoryResult::Back;
        }
        out
    }
}

/// Crossings per second: lo (everything but potential 100) blue, hi red.
fn chart(ui: &mut egui::Ui, r: &CrossingResult) {
    let (Some(first), Some(last)) = (r.count_hi.first(), r.count_hi.last()) else {
        ui.label("No crossings found.");
        return;
    };
    let origin = first.0.timestamp();
    // one second is invisible over hours: widen the bars with the span
    let width = (((last.0.timestamp() - origin) as f64) / 400.0).max(1.0);
    let bars = |v: &[(DateTime<Utc>, u32)]| -> Vec<Bar> {
        v.iter()
            .map(|(t, n)| Bar::new((t.timestamp() - origin) as f64, f64::from(*n)).width(width))
            .collect()
    };
    let start = first.0;
    Plot::new("crossing_history_chart")
        .height(220.0)
        .allow_scroll(false)
        .y_axis_label("crossings")
        .x_axis_formatter(move |mark, _| {
            let t = start + chrono::Duration::seconds(mark.value as i64);
            t.format("%Y-%m-%d\n%H:%M:%S").to_string()
        })
        .show(ui, |pu| {
            pu.bar_chart(BarChart::new("Count_Lo", bars(&r.count_lo)).color(Color32::BLUE));
            pu.bar_chart(BarChart::new("Count_Hi", bars(&r.count_hi)).color(Color32::RED));
        });
}

/// Crossing History button (MapDlg.cs:6827-6837): needs a loaded history with `max > min`.
pub fn open(state: &mut AppState) {
    if state.data_dir.is_none() {
        state.status = "No data directory".into();
        return;
    }
    let Some(h) = &state.history else { return };
    if h.max <= h.min || h.data.positions.is_empty() {
        return;
    }
    let s = &state.settings;
    let dlg = CrossingHistoryDlg::new(h.min, h.max, h.data.oldest, &s.my_call, &s.dx_call);
    state.status = "Press Calculate to start...".into();
    state.analysis_ui.crossing_dlg = Some(dlg);
}

/// Draw the dialog and act on what the user did; called every frame by the analysis tab.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.analysis_ui.crossing_dlg.take() else {
        return;
    };
    let Some(h) = &state.history else {
        // HISTORY was left: nothing to calculate on
        return;
    };
    let data = h.data.clone();
    let idle = analysis_enabled(state).crossing;
    let status = state.status_line();
    match dlg.show(ctx, idle, &mut state.settings.analysis, &status) {
        CrossingHistoryResult::Open => {}
        CrossingHistoryResult::Back => {
            // `FormClosing` cancels the worker; never a job that started or ended since
            if dlg.running {
                state.cancel_job(dlg.job_id);
            }
            return;
        }
        CrossingHistoryResult::Calculate => {
            if idle && !dlg.running {
                let s = &state.settings;
                let params = CrossingParams::from_settings(s, dlg.from, dlg.to, dlg.step_s);
                let job = AnalysisJob::Crossings {
                    data,
                    params: Box::new(params),
                    settings: Box::new(s.clone()),
                };
                state.status = "Calculating Path....".into();
                state.run_analysis_job(job);
                dlg.begin(state.current_job_id());
            }
        }
        CrossingHistoryResult::Export => {
            if let (true, Some(result)) = (idle, dlg.result.clone()) {
                let s = &state.settings;
                let job = AnalysisJob::ExportCrossings {
                    result,
                    gap_s: f64::from(s.analysis.crossing_ambiguous_gap_s),
                    from: dlg.from,
                    to: dlg.to,
                    my_call: s.my_call.clone(),
                    dx_call: s.dx_call.clone(),
                };
                state.status = "Checking ambiguous crossings...".into();
                state.run_analysis_job(job);
                state.analysis_ui.files = None;
            }
        }
    }
    state.analysis_ui.crossing_dlg = Some(dlg);
}
