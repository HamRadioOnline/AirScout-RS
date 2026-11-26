//! Coverage view, station map grid and their jobs through the app (egui_kittest, no network).
mod maint_common;
use airscout::coverage::{GridRequest, GRID_MIN_ZOOM};
use airscout::jobs::JobEvent;
use airscout::maintenance::MaintenanceJob;
use airscout::services::ServiceEvent;
use airscout::ui::dialogs::map_station::MapStationDlg;
use airscout::ui::options::{OptionsTab, ViewRequest};
use as_elevation::ElevationModel;
use as_geo::latlon::GRect;
use as_settings::Settings;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use maint_common::*;

fn coverage_jobs(rec: &Rec) -> Vec<(u64, MaintenanceJob)> {
    rec.0
        .borrow()
        .maint
        .iter()
        .filter(|(_, j)| matches!(j, MaintenanceJob::Coverage { .. }))
        .cloned()
        .collect()
}

fn grid_jobs(rec: &Rec) -> Vec<(u64, MaintenanceJob)> {
    rec.0
        .borrow()
        .maint
        .iter()
        .filter(|(_, j)| matches!(j, MaintenanceJob::ElevationGrid { .. }))
        .cloned()
        .collect()
}

fn send(h: &mut Harness<'static, airscout::app::AppState>, id: u64, event: JobEvent) {
    h.state_mut().on_event(ServiceEvent::Job { id, event });
}

#[test]
fn coverage_starts_only_from_the_button_and_close_cancels_it() {
    let (mut h, rec) = harness();
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Elevation;
    h.run();
    h.run();
    assert!(coverage_jobs(&rec).is_empty(), "no download on tab enter");
    // one button per model; the first is GLOBE
    h.get_all_by_label("Show coverage").next().unwrap().click();
    h.run();
    let jobs = coverage_jobs(&rec);
    assert_eq!(jobs.len(), 1);
    let (
        id,
        MaintenanceJob::Coverage {
            model,
            base_url,
            area,
        },
    ) = jobs[0].clone()
    else {
        unreachable!()
    };
    let s = Settings::default();
    assert_eq!(model, ElevationModel::Globe);
    assert_eq!(base_url, s.gui.elevation_base_url);
    assert_eq!(
        area,
        GRect::new(
            s.area.min_lat,
            s.area.min_lon,
            s.area.max_lat,
            s.area.max_lon
        )
    );
    // the view waits for the answer, then shows the status of the C#
    h.get_by_label("GLOBE elevation coverage");
    send(
        &mut h,
        id,
        JobEvent::Coverage {
            found: vec!["JO50AA".into()],
            missing: vec!["JO50AB".into(), "JO50AC".into()],
        },
    );
    h.run();
    h.get_by_label("GLOBE: 1 tile(s) found, 2 more tile(s) available and missing.");
    // the job is done: closing the view has nothing to cancel
    h.get_by_label("Close").click();
    h.run();
    assert!(rec.0.borrow().cancelled.is_empty());
    // a new press, closed before the answer: the job is cancelled
    h.get_all_by_label("Show coverage").next().unwrap().click();
    h.run();
    let id2 = coverage_jobs(&rec)[1].0;
    assert_ne!(id, id2);
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(rec.0.borrow().cancelled, vec![id2]);
    // the late answer of the cancelled job is dropped
    send(
        &mut h,
        id2,
        JobEvent::Coverage {
            found: vec![],
            missing: vec![],
        },
    );
    assert_eq!(coverage_jobs(&rec).len(), 2);
}

#[test]
fn closing_options_cancels_a_running_coverage_job() {
    let (mut h, rec) = harness();
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().on_view_request(ViewRequest::Coverage {
        model: ElevationModel::Globe,
        base_url: "http://127.0.0.1:1".into(),
        area: GRect::new(50.0, 10.0, 51.0, 12.0),
    });
    let id = coverage_jobs(&rec)[0].0;
    // a second request (another model) supersedes the first
    h.state_mut().on_view_request(ViewRequest::Coverage {
        model: ElevationModel::Srtm3,
        base_url: "http://127.0.0.1:1".into(),
        area: GRect::new(50.0, 10.0, 51.0, 12.0),
    });
    assert_eq!(rec.0.borrow().cancelled, vec![id]);
    let id2 = coverage_jobs(&rec)[1].0;
    // an answer of the superseded job is dropped, the newer one is taken
    send(&mut h, id, JobEvent::Failed("old".into()));
    assert!(!h.state().status.contains("old"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert!(rec.0.borrow().cancelled.contains(&id2));
}

#[test]
fn newer_grid_job_supersedes_the_older() {
    let (mut h, rec) = harness();
    let m = ElevationModel::Srtm1;
    let a = GRect::new(50.0, 10.0, 50.001, 10.001);
    let b = GRect::new(50.1, 10.1, 50.101, 10.101);
    h.state_mut()
        .on_view_request(ViewRequest::Grid(GridRequest::Run { rect: a, model: m }));
    h.state_mut()
        .on_view_request(ViewRequest::Grid(GridRequest::Run { rect: b, model: m }));
    let jobs = grid_jobs(&rec);
    assert_eq!(jobs.len(), 2);
    assert_eq!(rec.0.borrow().cancelled, vec![jobs[0].0]);
    assert_eq!(
        jobs[1].1,
        MaintenanceJob::ElevationGrid { rect: b, model: m }
    );
    // Cancel stops the newest one
    h.state_mut()
        .on_view_request(ViewRequest::Grid(GridRequest::Cancel));
    assert_eq!(rec.0.borrow().cancelled, vec![jobs[0].0, jobs[1].0]);
    // nothing running: Cancel is a no-op
    h.state_mut()
        .on_view_request(ViewRequest::Grid(GridRequest::Cancel));
    assert_eq!(rec.0.borrow().cancelled.len(), 2);
}

struct Dlg {
    dlg: MapStationDlg,
    settings: Settings,
}

fn dlg_harness(overlay: bool) -> Harness<'static, Dlg> {
    Harness::new_ui_state(
        |ui, st: &mut Dlg| {
            let _ = st.dlg.show(ui.ctx(), &st.settings);
        },
        Dlg {
            dlg: MapStationDlg::new("TE1ST", 50.0, 10.0, None)
                .with_elevation(overlay, ElevationModel::Srtm1),
            settings: Settings::default(),
        },
    )
}

#[test]
fn map_dialog_grid_follows_zoom_and_checkbox() {
    let mut h = dlg_harness(true);
    h.run();
    assert!(h.state_mut().dlg.take_grid_request().is_none(), "zoom 12");
    h.get_by_label("Overlay elevation grid (zoom ≥ 17)");
    h.state_mut().dlg.set_zoom(GRID_MIN_ZOOM + 0.5);
    h.run();
    let Some(GridRequest::Run { rect, model }) = h.state_mut().dlg.take_grid_request() else {
        panic!("zoom 17.5 asks for the grid");
    };
    assert_eq!(model, ElevationModel::Srtm1);
    assert!(rect.min_lat < 50.0 && rect.max_lat > 50.0, "{rect:?}");
    assert!(
        h.state_mut().dlg.take_grid_request().is_none(),
        "asked once"
    );
    // the answer is drawn (no crash, cells kept)
    h.state_mut()
        .dlg
        .set_grid(vec![(GRect::new(50.0, 10.0, 50.0003, 10.0006), 120)]);
    h.run();
    assert_eq!(h.state().dlg.grid_cells().len(), 1);
    // unticking cancels and drops the grid
    h.get_by_label("Overlay elevation grid (zoom ≥ 17)").click();
    h.run();
    assert!(!h.state().dlg.overlay());
    assert_eq!(
        h.state_mut().dlg.take_grid_request(),
        Some(GridRequest::Cancel)
    );
    assert!(h.state().dlg.grid_cells().is_empty());
}

#[test]
fn map_dialog_without_overlay_asks_for_nothing() {
    let mut h = dlg_harness(false);
    h.state_mut().dlg.set_zoom(18.0);
    h.run();
    assert!(h.state_mut().dlg.take_grid_request().is_none());
}

#[test]
fn grid_result_reaches_the_map_dialog_of_the_options() {
    let (mut h, rec) = harness();
    h.state_mut().settings.gui.stations_map_overlay_elevation = true;
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Stations;
    h.run();
    h.get_all_by_label_contains("map...")
        .next()
        .unwrap()
        .click();
    h.run();
    let o = h.state_mut().options_dlg.as_mut().unwrap();
    o.map_dialog().expect("map open").set_zoom(18.0);
    h.run();
    h.run();
    let jobs = grid_jobs(&rec);
    assert_eq!(jobs.len(), 1, "one job for one view");
    let id = jobs[0].0;
    send(
        &mut h,
        id,
        JobEvent::ElevationGrid(vec![(GRect::new(50.0, 10.0, 50.0003, 10.0006), 7)]),
    );
    let o = h.state_mut().options_dlg.as_mut().unwrap();
    assert_eq!(o.map_dialog().unwrap().grid_cells().len(), 1);
    // Cancel of the map cancels a job that still runs
    h.get_all_by_label("Cancel").last().unwrap().click();
    h.run();
    h.run();
    assert!(h
        .state_mut()
        .options_dlg
        .as_mut()
        .unwrap()
        .map_dialog()
        .is_none());
}
