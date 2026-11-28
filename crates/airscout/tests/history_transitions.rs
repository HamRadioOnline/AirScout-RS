//! State transitions across HISTORY, Options, exit and maintenance (final review I1, I2, M1).
use airscout::analysis::AnalysisJob;
use airscout::app::{AppState, Services};
use airscout::history::HistoryData;
use airscout::jobs::JobEvent;
use airscout::maintenance::MaintenanceJob;
use airscout::services::{PathRequest, ServiceEvent};
use airscout::ui::analysis::{self, analysis_enabled};
use as_aircraft::enums::LifeMode;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_settings::Settings;
use chrono::{DateTime, Duration, TimeZone, Utc};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Log {
    bg_starts: usize,
    bg_restarts: usize,
    runs: Vec<(u64, AnalysisJob)>,
    maint: Vec<(u64, MaintenanceJob)>,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

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
    fn start_background(&mut self, _s: &Settings, _g: u64) {
        self.0.borrow_mut().bg_starts += 1;
    }
    fn restart_background(&mut self, _s: &Settings, _g: u64) {
        self.0.borrow_mut().bg_restarts += 1;
    }
    fn run_analysis(&mut self, id: u64, job: AnalysisJob) {
        self.0.borrow_mut().runs.push((id, job));
    }
    fn run_maintenance(&mut self, id: u64, job: MaintenanceJob) {
        self.0.borrow_mut().maint.push((id, job));
    }
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()
}

fn pos(hex: &str, t: i64) -> PlaneInfo {
    PlaneInfo::new(
        t0() + Duration::seconds(t),
        "C1",
        "",
        hex,
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

fn data() -> HistoryData {
    let positions = vec![pos("AAAAAA", 0), pos("BBBBBB", 600)];
    HistoryData {
        oldest: Some(t0()),
        youngest: Some(t0() + Duration::seconds(900)),
        count: positions.len() as u64,
        positions,
    }
}

fn state(rec: &Rec) -> AppState {
    AppState::new(
        Settings::default(),
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

/// ON and the load delivered.
fn loaded(rec: &Rec, st: &mut AppState) {
    st.enter_history().unwrap();
    let id = rec.0.borrow().runs.last().unwrap().0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::HistoryLoaded(Box::new(data())),
    });
    assert!(!st.settings.time.online, "HISTORY replays offline");
}

// ---- I1: exit in HISTORY ----

#[test]
fn exit_in_history_saves_the_time_mode_from_before_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let rec = Rec::default();
    let mut st = state(&rec);
    st.settings_path = Some(path.clone());
    assert!(st.settings.time.online);
    loaded(&rec, &mut st);
    st.on_exit();
    let saved = Settings::load_or_default(&path).unwrap();
    assert!(saved.time.online, "the next session must show planes");
}

#[test]
fn startup_forces_online_time() {
    // MapDlg.cs:1958-1959: `Time_Mode_Online = true` on every start
    let rec = Rec::default();
    let mut st = state(&rec);
    st.settings.time.online = false;
    st.startup();
    assert!(st.settings.time.online);
}

// ---- I2: Options OK in HISTORY ----

#[test]
fn options_ok_in_history_keeps_workers_stopped_and_replay_time() {
    let rec = Rec::default();
    let mut st = state(&rec);
    // the dialog copy is taken before the load: online, no replay time
    let mut new = st.settings.clone();
    loaded(&rec, &mut st);
    let replay = t0() + Duration::seconds(300);
    st.settings.time.offline = replay;
    let starts = rec.0.borrow().bg_starts;
    new.planes_position_ttl_min = 7;
    st.apply_options(new);
    let l = rec.0.borrow();
    assert_eq!(
        l.bg_restarts, 0,
        "no worker (maintainer) restart in HISTORY"
    );
    assert_eq!(l.bg_starts, starts);
    drop(l);
    assert_eq!(st.settings.planes_position_ttl_min, 7, "the options apply");
    assert_eq!(st.settings.time.offline, replay, "the player stays");
    assert!(!st.settings.time.online, "the replay stays offline");
    assert_eq!(st.life_mode, LifeMode::History);
    // OFF restarts the workers and restores the time mode
    st.leave_history();
    assert_eq!(rec.0.borrow().bg_starts, starts + 1);
    assert!(st.settings.time.online);
}

#[test]
fn options_close_in_life_restarts_workers() {
    let rec = Rec::default();
    let mut st = state(&rec);
    let new = st.settings.clone();
    st.apply_options(new);
    st.options_closed();
    assert_eq!(rec.0.borrow().bg_restarts, 1);
}

// ---- M1: a maintenance job blocks ON and the analysis jobs ----

#[test]
fn maintenance_job_blocks_history_on() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    assert!(st.maint.running.is_some());
    assert!(!analysis_enabled(&st).on);
    let e = st.enter_history().unwrap_err();
    assert!(e.contains("maintenance"), "{e}");
    assert_eq!(st.life_mode, LifeMode::Life);
    assert!(rec.0.borrow().runs.is_empty(), "no load started");
    assert_eq!(st.analysis_blocked(), Some("a maintenance job is running"));
}

#[test]
fn maintenance_job_blocks_analysis_buttons_in_history() {
    let rec = Rec::default();
    let mut st = state(&rec);
    loaded(&rec, &mut st);
    let b = analysis_enabled(&st);
    assert!(b.load && b.save && b.clear && b.url && b.traffic && b.crossing);
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    let b = analysis_enabled(&st);
    assert!(!b.load && !b.save && !b.clear && !b.url);
    assert!(!b.traffic && !b.export_path && !b.crossing && !b.plane_history);
    assert!(b.off, "OFF stays usable");
    // the job ends: usable again
    let id = rec.0.borrow().maint.last().unwrap().0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("ok".into()),
    });
    assert!(st.maint.running.is_none());
    assert!(analysis_enabled(&st).load);
    assert_eq!(st.analysis_blocked(), None);
}

#[test]
fn analysis_tab_shows_why_it_is_blocked() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 1400.0))
        .build_ui_state(|ui, st: &mut AppState| analysis::show(ui, st), st);
    h.run();
    h.get_by_label_contains("a maintenance job is running");
}
