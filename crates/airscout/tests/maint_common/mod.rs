//! Shared fixtures of the maintenance tests: a recording `Services`, an app state, job runner.
#![allow(dead_code)]
use airscout::analysis::AnalysisJob;
use airscout::app::{AppState, Services};
use airscout::jobs::{JobCtx, JobEvent};
use airscout::maintenance::{run, MaintenanceJob};
use airscout::services::{PathRequest, Paths, ServiceEvent};
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_settings::Settings;
use as_stations::{GeoSource, Location, Qrv};
use chrono::Utc;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;

#[derive(Default)]
pub struct Log {
    pub maint: Vec<(u64, MaintenanceJob)>,
    pub analysis: Vec<(u64, AnalysisJob)>,
    pub cancelled: Vec<u64>,
    /// What `background_jobs` answers: > 0 blocks a delete/clear (#155).
    pub background_jobs: usize,
}

#[derive(Clone, Default)]
pub struct Rec(pub Rc<RefCell<Log>>);

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
    fn run_maintenance(&mut self, id: u64, job: MaintenanceJob) {
        self.0.borrow_mut().maint.push((id, job));
    }
    fn cancel_maintenance(&mut self, id: u64) {
        self.0.borrow_mut().cancelled.push(id);
    }
    fn background_jobs(&self) -> usize {
        self.0.borrow().background_jobs
    }
    fn run_analysis(&mut self, id: u64, job: AnalysisJob) {
        self.0.borrow_mut().analysis.push((id, job));
    }
}

pub fn state() -> (AppState, Rec) {
    let rec = Rec::default();
    let st = AppState::new(
        Settings::default(),
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    );
    (st, rec)
}

pub fn ids(rec: &Rec) -> usize {
    rec.0.borrow().maint.len()
}

pub fn run_job(paths: &Paths, job: MaintenanceJob) -> Vec<JobEvent> {
    let (tx, rx) = channel();
    let ctx = JobCtx::new(5, Arc::new(AtomicBool::new(false)), tx);
    run(&ctx, paths, job);
    rx.try_iter()
        .map(|e| match e {
            ServiceEvent::Job { id, event } => {
                assert_eq!(id, 5);
                event
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

pub fn loc(call: &str, l: &str) -> Location {
    Location::new(call, l, 50.0, 8.0, GeoSource::FromUser, 0, Utc::now())
}

pub fn qrv(call: &str, l: &str, band: Band) -> Qrv {
    Qrv::new(call, l, band, 10.0, 0.0, 10.0, Utc::now()).unwrap()
}

pub fn failed(ev: &[JobEvent]) -> Option<&String> {
    ev.iter().find_map(|e| match e {
        JobEvent::Failed(s) => Some(s),
        _ => None,
    })
}

pub fn harness() -> (Harness<'static, AppState>, Rec) {
    let (st, rec) = state();
    let h = Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    );
    (h, rec)
}

pub fn answer(h: &mut Harness<'static, AppState>, rec: &Rec, job: &MaintenanceJob) {
    let id = rec.0.borrow().maint.last().unwrap().0;
    let _ = job;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Done("ok".into()),
    });
}
