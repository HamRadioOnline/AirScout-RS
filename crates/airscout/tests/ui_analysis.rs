//! Analysis tab, driven through egui_kittest (no rendering).
use airscout::analysis::AnalysisJob;
use airscout::app::{AppState, Services};
use airscout::history::HistoryData;
use airscout::jobs::JobEvent;
use airscout::services::{PathRequest, ServiceEvent};
use airscout::ui::analysis::{self, analysis_enabled};
use airscout::ui::control;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_settings::Settings;
use chrono::{DateTime, Duration, TimeZone, Utc};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Default)]
struct Rec(
    Rc<RefCell<Vec<(u64, AnalysisJob)>>>,
    Rc<std::cell::Cell<usize>>,
);

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {}
    fn start_feed(
        &mut self,
        _i: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn run_analysis(&mut self, id: u64, job: AnalysisJob) {
        self.0.borrow_mut().push((id, job));
    }
    fn cancel_analysis(&mut self) {
        self.1.set(self.1.get() + 1);
    }
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()
}

fn pos(t: i64) -> PlaneInfo {
    PlaneInfo::new(
        t0() + Duration::seconds(t),
        "C1",
        "",
        "AAAAAA",
        50.0,
        8.0,
        90.0,
        30000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

fn state(rec: &Rec) -> AppState {
    AppState::new(
        Settings::default(),
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn loaded(rec: &Rec) -> AppState {
    let mut st = state(rec);
    st.enter_history().unwrap();
    let id = rec.0.borrow().last().unwrap().0;
    let positions = vec![pos(0), pos(60)];
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::HistoryLoaded(Box::new(HistoryData {
            oldest: Some(positions[0].time),
            youngest: Some(positions[1].time),
            count: 2,
            positions,
        })),
    });
    st
}

fn harness(st: AppState) -> Harness<'static, AppState> {
    // tall: the tab scrolls in the app, a click outside the screen hits nothing
    Harness::builder()
        .with_size(egui::vec2(900.0, 1400.0))
        .build_ui_state(|ui, st: &mut AppState| analysis::show(ui, st), st)
}

#[test]
fn buttons_disabled_until_loaded() {
    let rec = Rec::default();
    let mut st = state(&rec);
    let b = analysis_enabled(&st);
    assert!(b.on && !b.off && !b.load && !b.save && !b.clear && !b.player);

    st.enter_history().unwrap();
    let b = analysis_enabled(&st);
    assert!(b.off && !b.on && !b.load && !b.save && !b.clear && !b.player);

    let mut st = loaded(&rec);
    let b = analysis_enabled(&st);
    assert!(b.off && b.load && b.save && b.clear && b.player);

    // a second job would cancel the first one: everything but OFF waits
    st.run_analysis_job(AnalysisJob::ClearPositions);
    let b = analysis_enabled(&st);
    assert!(b.off && !b.load && !b.save && !b.clear && !b.player);
    let id = rec.0.borrow().last().unwrap().0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("done".into()),
    });
    assert!(!analysis_enabled(&st).load, "the reload runs");
}

#[test]
fn on_disabled_while_playing() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    assert!(!analysis_enabled(&st).on);
}

#[test]
fn player_buttons_change_interval() {
    let rec = Rec::default();
    let mut h = harness(loaded(&rec));
    h.run();
    h.get_by_label("Fast forward").click();
    h.run();
    assert_eq!(h.state().history.as_ref().unwrap().interval_s, 10);
    h.get_by_label("Back").click();
    h.run();
    assert_eq!(h.state().history.as_ref().unwrap().interval_s, 9);
    h.get_by_label("Pause").click();
    h.run();
    assert_eq!(h.state().history.as_ref().unwrap().interval_s, 0);
}

#[test]
fn save_writes_to_tmp_and_status_shows_path() {
    let rec = Rec::default();
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(&rec);
    st.data_dir = Some(dir.path().to_path_buf());
    let mut h = harness(st);
    h.run();
    h.get_by_label("Save Positions").click();
    h.run();
    let (_, job) = rec.0.borrow().last().cloned().unwrap();
    let AnalysisJob::SavePositions { path, csv, .. } = job else {
        panic!("expected SavePositions");
    };
    assert!(!csv);
    assert_eq!(
        path,
        dir.path()
            .join("Tmp")
            .join("Plane Positions 2024_05_01_12_00_00 to 2024_05_01_12_01_00.json")
    );
    assert!(h.state().status.contains(&path.display().to_string()));
}

#[test]
fn clear_asks_first() {
    let rec = Rec::default();
    let mut h = harness(loaded(&rec));
    h.run();
    let n = rec.0.borrow().len();
    h.get_by_label("Clear Positions").click();
    h.run();
    assert_eq!(rec.0.borrow().len(), n, "no job before the confirmation");
    h.get_by_label("Yes").click();
    h.run();
    assert!(matches!(
        rec.0.borrow().last().unwrap().1,
        AnalysisJob::ClearPositions
    ));
}

#[test]
fn load_lists_tmp_files_and_runs_selection() {
    let rec = Rec::default();
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    for f in ["a.json", "b.csv", "c.hist", "d.txt"] {
        std::fs::write(tmp.join(f), "").unwrap();
    }
    let mut st = loaded(&rec);
    st.data_dir = Some(dir.path().to_path_buf());
    let mut h = harness(st);
    h.run();
    assert!(h.query_by_label("d.txt").is_none());
    h.get_by_label("b.csv").click();
    h.run();
    h.get_by_label("Load Positions").click();
    h.run();
    let AnalysisJob::LoadFiles(files) = rec.0.borrow().last().unwrap().1.clone() else {
        panic!("expected LoadFiles");
    };
    assert_eq!(files, vec![tmp.join("b.csv")]);
    assert_eq!(h.state().settings.analysis.load_file_name, "b.csv");
}

#[test]
fn play_button_disabled_in_history() {
    let rec = Rec::default();
    let mut h = Harness::new_ui_state(|ui, st: &mut AppState| control::show(ui, st), loaded(&rec));
    h.run();
    h.get_by_label("Play").click();
    h.run();
    assert!(!h.state().is_playing());
}

#[test]
fn url_dialog_needs_disclaimer_then_starts_download_job() {
    let rec = Rec::default();
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(&rec);
    st.data_dir = Some(dir.path().to_path_buf());
    st.settings.analysis.history_url = "https://example.invalid/archive".into();
    let mut h = harness(st);
    h.run();
    let n = rec.0.borrow().len();
    h.get_by_label("Load History from URL").click();
    h.run();
    // the disclaimer comes first; Decline closes everything without a job
    h.get_by_label("Decline").click();
    h.run();
    assert!(h.query_by_label("Start").is_none());
    assert!(!h.state().settings.analysis.history_disclaimer_accepted);
    h.get_by_label("Load History from URL").click();
    h.run();
    h.get_by_label("Accept").click();
    h.run();
    assert!(h.state().settings.analysis.history_disclaimer_accepted);
    h.get_by_label("Start").click();
    h.run();
    assert_eq!(rec.0.borrow().len(), n + 1);
    let jobs = rec.0.borrow();
    match &jobs.last().unwrap().1 {
        AnalysisJob::Download(r) => {
            assert_eq!(r.url_base, "https://example.invalid/archive");
            assert_eq!(r.dir, dir.path().join("Tmp").join("History"));
            assert_eq!(r.step_min, 5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn export_path_without_a_path_says_no_path() {
    let rec = Rec::default();
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(&rec);
    st.data_dir = Some(dir.path().to_path_buf());
    let n = rec.0.borrow().len();
    let mut h = harness(st);
    h.run();
    h.get_by_label("Export Path").click();
    h.run();
    assert_eq!(h.state().status, "no path");
    assert_eq!(rec.0.borrow().len(), n, "no job without a path");
}

#[test]
fn plane_history_dialog_loads_lists_then_exports() {
    use airscout::exports::HexOrCall;
    let rec = Rec::default();
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(&rec);
    st.data_dir = Some(dir.path().to_path_buf());
    let mut h = harness(st);
    h.run();
    h.get_by_label("Export Plane History").click();
    h.run();
    let (id, job) = rec.0.borrow().last().cloned().unwrap();
    assert!(matches!(job, AnalysisJob::PlaneLists { .. }), "{job:?}");
    // the lists arrive, the job ends
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::PlaneLists {
            hexes: vec!["AAAAAA".into()],
            calls: vec!["DLH123".into()],
        },
    });
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("Ready.".into()),
    });
    h.run();
    h.get_by_label("Export").click();
    h.run();
    // nothing typed yet: no export
    assert!(matches!(
        rec.0.borrow().last().unwrap().1,
        AnalysisJob::PlaneLists { .. }
    ));
    let dlg = h.state_mut().analysis_ui.plane_dlg.as_mut().unwrap();
    dlg.call = "DLH123".into();
    h.run();
    h.get_by_label("Export").click();
    h.run();
    let (_, job) = rec.0.borrow().last().cloned().unwrap();
    let AnalysisJob::ExportPlane { key, .. } = job else {
        panic!("expected ExportPlane");
    };
    assert_eq!(key, HexOrCall::Call("DLH123".into()));
}

/// The dialog of a running lists job, and the id of that job.
fn plane_dialog_open(rec: &Rec) -> (Harness<'static, AppState>, u64) {
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(rec);
    st.data_dir = Some(dir.keep());
    let mut h = harness(st);
    h.run();
    h.get_by_label("Export Plane History").click();
    h.run();
    let (id, job) = rec.0.borrow().last().cloned().unwrap();
    assert!(matches!(job, AnalysisJob::PlaneLists { .. }));
    (h, id)
}

#[test]
fn closing_the_dialog_cancels_its_waiting_lists_job() {
    let rec = Rec::default();
    let (mut h, _) = plane_dialog_open(&rec);
    let before = rec.1.get();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.1.get(), before + 1);
    assert!(h.state().analysis_ui.plane_dlg.is_none());
}

#[test]
fn failed_lists_job_stops_waiting_and_close_cancels_nothing() {
    let rec = Rec::default();
    let (mut h, id) = plane_dialog_open(&rec);
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Failed("db broken".into()),
    });
    h.run();
    let dlg = h.state().analysis_ui.plane_dlg.as_ref().unwrap();
    assert_eq!(dlg.error.as_deref(), Some("db broken"));
    assert!(h
        .query_by_label("Collecting data, please wait...")
        .is_none());
    let before = rec.1.get();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.1.get(), before, "the job is over: nothing to cancel");
}

#[test]
fn closing_a_stale_dialog_leaves_a_newer_job_alone() {
    let rec = Rec::default();
    let (mut h, id) = plane_dialog_open(&rec);
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Failed("db broken".into()),
    });
    // the user starts another job while the dialog is still open
    h.state_mut()
        .run_analysis_job(AnalysisJob::LoadFiles(Vec::new()));
    h.run();
    let before = rec.1.get();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.1.get(), before, "the newer job is not cancelled");
    assert!(h.state().history.as_ref().unwrap().busy, "it still runs");
}

// ---- Crossing History dialog (Task 10)

fn crossing_result() -> airscout::crossing_history::CrossingResult {
    use as_propagation::{PathParams, PropagationPath};
    let path = PropagationPath::new(
        PathParams {
            lat1: 50.0,
            lon1: 8.0,
            h1: 300.0,
            lat2: 50.0,
            lon2: 9.0,
            h2: 300.0,
            qrg_ghz: 10.368,
            radius_km: 8000.0,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.0,
        0.0,
        true,
    );
    let mut c = pos(0);
    c.potential = 100;
    airscout::crossing_history::CrossingResult {
        crossings: vec![c],
        count_hi: vec![(t0(), 1)],
        count_lo: vec![(t0(), 0)],
        path,
    }
}

/// The history is loaded, the dialog open, nothing calculated yet.
fn crossing_dialog_open(rec: &Rec) -> Harness<'static, AppState> {
    let dir = tempfile::tempdir().unwrap();
    let mut st = loaded(rec);
    st.data_dir = Some(dir.keep());
    let mut h = harness(st);
    h.run();
    h.get_by_label("Crossing History").click();
    h.run();
    h
}

#[test]
fn crossing_dialog_calculates_then_exports() {
    let rec = Rec::default();
    let mut h = crossing_dialog_open(&rec);
    let n = rec.0.borrow().len();
    assert!(h.state().analysis_ui.crossing_dlg.is_some());
    assert_eq!(rec.0.borrow().len(), n, "opening starts no job");
    // the range: cut to 00:00, then up to the oldest report
    let dlg = h.state().analysis_ui.crossing_dlg.as_ref().unwrap();
    assert_eq!(
        (dlg.from, dlg.to, dlg.step_s),
        (t0(), t0() + Duration::seconds(60), 60)
    );
    // Export is not possible before a run
    h.get_by_label("Export to CSV").click();
    h.run();
    assert_eq!(rec.0.borrow().len(), n);

    h.get_by_label("Consider crossings with recorded signal level only")
        .click();
    h.run();
    h.get_by_label("Calculate").click();
    h.run();
    let (id, job) = rec.0.borrow().last().cloned().unwrap();
    let AnalysisJob::Crossings {
        params,
        settings,
        data,
    } = job
    else {
        panic!("expected Crossings");
    };
    assert_eq!((params.step_s, params.ttl_min, params.from), (60, 5, t0()));
    assert!(params.with_signal_level && settings.analysis.crossing_with_signal_level);
    assert_eq!(data.positions.len(), 2);
    assert!(h.state().analysis_ui.crossing_dlg.as_ref().unwrap().running);

    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Crossings(Box::new(crossing_result())),
    });
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("Calculation done, 1 crossings.".into()),
    });
    h.run();
    let dlg = h.state().analysis_ui.crossing_dlg.as_ref().unwrap();
    assert!(!dlg.running && dlg.result.is_some());
    h.get_by_label("Export to CSV").click();
    h.run();
    let (_, job) = rec.0.borrow().last().cloned().unwrap();
    let AnalysisJob::ExportCrossings {
        gap_s,
        result,
        my_call,
        ..
    } = job
    else {
        panic!("expected ExportCrossings");
    };
    assert_eq!((gap_s, result.crossings.len()), (10.0, 1));
    assert_eq!(my_call, Settings::default().my_call);
}

/// The dialog's "Back" (the player has one too; the dialog is drawn last).
fn click_back(h: &mut Harness<'static, AppState>) {
    let backs: Vec<_> = h.get_all_by_label("Back").collect();
    assert_eq!(backs.len(), 2, "the player's and the dialog's");
    backs.last().unwrap().click();
    h.run();
}

#[test]
fn back_cancels_the_running_calculation_and_only_that() {
    let rec = Rec::default();
    let mut h = crossing_dialog_open(&rec);
    h.get_by_label("Calculate").click();
    h.run();
    let before = rec.1.get();
    click_back(&mut h);
    assert_eq!(rec.1.get(), before + 1);
    assert!(h.state().analysis_ui.crossing_dlg.is_none());
    assert!(!h.state().history.as_ref().unwrap().busy);

    // a finished calculation: nothing to cancel
    let mut h = crossing_dialog_open(&rec);
    h.get_by_label("Calculate").click();
    h.run();
    let id = rec.0.borrow().last().unwrap().0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Crossings(Box::new(crossing_result())),
    });
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("done".into()),
    });
    h.run();
    let before = rec.1.get();
    click_back(&mut h);
    assert_eq!(rec.1.get(), before);
}

#[test]
fn failed_calculation_shows_the_error_and_can_run_again() {
    let rec = Rec::default();
    let mut h = crossing_dialog_open(&rec);
    h.get_by_label("Calculate").click();
    h.run();
    let id = rec.0.borrow().last().unwrap().0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Failed("no elevation model enabled".into()),
    });
    h.run();
    let dlg = h.state().analysis_ui.crossing_dlg.as_ref().unwrap();
    assert!(!dlg.running);
    assert_eq!(dlg.error.as_deref(), Some("no elevation model enabled"));
    assert!(!h.state().history.as_ref().unwrap().busy);
    let n = rec.0.borrow().len();
    h.get_by_label("Calculate").click();
    h.run();
    assert_eq!(rec.0.borrow().len(), n + 1);
}

// ---- Traffic dialog (Task 11)

fn traffic_dialog_open(rec: &Rec) -> Harness<'static, AppState> {
    let mut h = harness(loaded(rec));
    h.run();
    h.get_by_label("Show All Traffic").click();
    h.run();
    h
}

#[test]
fn traffic_dialog_starts_with_the_history_range_and_shows_routes() {
    let rec = Rec::default();
    let mut h = traffic_dialog_open(&rec);
    assert!(h.state().analysis_ui.traffic_dlg.is_some());
    h.get_by_label("Show").click();
    h.run();
    let (id, job) = rec.0.borrow().last().cloned().unwrap();
    // the loaded positions span t0 .. t0 + 60 s; the unchanged fields keep their seconds
    match job {
        AnalysisJob::Traffic { from, to } => {
            assert_eq!((from, to), (t0(), t0() + Duration::seconds(60)));
        }
        other => panic!("{other:?}"),
    }
    let route = vec![as_geo::latlon::GPoint::new(50.0, 8.0); 2];
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Routes(vec![route.clone(), route]),
    });
    h.run();
    let dlg = h.state().analysis_ui.traffic_dlg.as_ref().unwrap();
    assert_eq!(dlg.route_count(), 2);
    assert_eq!(dlg.opacity, 10);
}

#[test]
fn traffic_close_cancels_only_its_running_job() {
    let rec = Rec::default();
    let mut h = traffic_dialog_open(&rec);
    h.get_by_label("Show").click();
    h.run();
    let before = rec.1.get();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.1.get(), before + 1);
    assert!(h.state().analysis_ui.traffic_dlg.is_none());
    assert!(!h.state().history.as_ref().unwrap().busy);

    // a finished job: nothing to cancel
    let mut h = traffic_dialog_open(&rec);
    h.get_by_label("Show").click();
    h.run();
    let id = rec.0.borrow().last().unwrap().0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("1 routes".into()),
    });
    h.run();
    let before = rec.1.get();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.1.get(), before);
}

#[test]
fn failed_traffic_job_shows_the_error_and_can_run_again() {
    let rec = Rec::default();
    let mut h = traffic_dialog_open(&rec);
    h.get_by_label("Show").click();
    h.run();
    let id = rec.0.borrow().last().unwrap().0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Failed("db broken".into()),
    });
    h.run();
    let dlg = h.state().analysis_ui.traffic_dlg.as_ref().unwrap();
    assert_eq!(dlg.error.as_deref(), Some("db broken"));
    assert!(!h.state().history.as_ref().unwrap().busy);
    let n = rec.0.borrow().len();
    h.get_by_label("Show").click();
    h.run();
    assert_eq!(rec.0.borrow().len(), n + 1);
}
