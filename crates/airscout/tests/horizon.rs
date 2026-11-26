//! Local obstructions and horizon dialogs, and the horizon calculation thread.
use airscout::app::{AppState, Services};
use airscout::horizon_service::{ground_elevation_raw, HorizonRequest, HorizonService};
use airscout::services::{open_data, PathRequest, ServiceEvent};
use airscout::ui::dialogs::horizon::{export_csv, horizon_csv, HorizonState};
use airscout::ui::options::stations::StationAction;
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_elevation::{ElevationModel, LocalObstruction};
use as_geo::latlon::GPoint;
use as_propagation::{HorizonParams, HorizonPoint, PropagationHorizon};
use as_settings::{ElevationEnabled, Settings};
use chrono::Utc;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

mod common;

const WAIT: Duration = Duration::from_secs(60);

fn request(id: u64) -> HorizonRequest {
    HorizonRequest {
        id,
        call: "DL2ALF".into(),
        pos: GPoint::new(50.5, 11.0),
        height_m: 14.0,
        with_obstruction: true,
        settings: Settings::default(),
    }
}

#[test]
fn horizon_service_reports_360_points_and_done() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let _h = HorizonService::start(paths, request(7), tx);
    let mut seen = [false; 360];
    let done = loop {
        match rx.recv_timeout(WAIT).expect("event") {
            ServiceEvent::HorizonPoint { id, az, point } => {
                assert_eq!(id, 7);
                assert!(!seen[az], "azimuth {az} reported twice");
                assert!(point.dist >= 0.0);
                seen[az] = true;
            }
            ServiceEvent::HorizonDone { id, horizon } => {
                assert_eq!(id, 7);
                break horizon;
            }
            ServiceEvent::HorizonFailed { error, .. } => panic!("horizon failed: {error}"),
            _ => {}
        }
    };
    assert!(seen.iter().all(|&s| s), "all 360 azimuths reported");
    // parameters as HorizonDlg passes them: 500 km, antenna height above the ground
    let p = done.params;
    assert_eq!((p.lat, p.lon, p.dist_km), (50.5, 11.0, 500.0));
    assert!(p.h >= 14.0);
    assert_eq!(p.step_width_m, ElevationModel::Globe.default_step_width_m());
}

#[test]
fn horizon_without_elevation_model_fails_without_computing() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    let mut req = request(1);
    req.settings.elevation_enabled = ElevationEnabled {
        globe: false,
        srtm3: false,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    let _h = HorizonService::start(paths, req, tx);
    match rx.recv_timeout(WAIT).expect("event") {
        ServiceEvent::HorizonFailed { id, error } => {
            assert_eq!(id, 1);
            assert!(error.contains("elevation model"), "{error}");
        }
        e => panic!("unexpected {e:?}"),
    }
}

#[test]
fn horizon_cancel_on_drop() {
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let (tx, rx) = channel();
    drop(HorizonService::start(paths, request(3), tx));
    // the thread ends (its sender goes away) without a result
    let mut points = 0;
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(ServiceEvent::HorizonPoint { .. }) => points += 1,
            Ok(ServiceEvent::HorizonDone { .. }) => panic!("a cancelled horizon must not finish"),
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(e) => panic!("thread did not end: {e}"),
        }
    }
    assert!(
        points < 360,
        "cancel stopped the calculation ({points} points)"
    );
}

fn synthetic() -> PropagationHorizon {
    let mut horizon = [HorizonPoint::default(); 360];
    for (i, p) in horizon.iter_mut().enumerate() {
        *p = HorizonPoint {
            dist: 10.0 + i as f64,
            eps_min: (i as f64 / 10.0).to_radians(),
            elv: 100 + i as i16,
        };
    }
    PropagationHorizon {
        params: HorizonParams {
            lat: 50.0,
            lon: 10.0,
            h: 214.0,
            dist_km: 500.0,
            qrg_ghz: 10.368,
            radius_km: 8500.0,
            f1_clearance: 1.0,
            step_width_m: 1000.0,
        },
        horizon,
        valid: true,
        local_obstructed: false,
        local_obstruction: None,
        last_updated: Utc::now(),
    }
}

#[test]
fn horizon_csv_columns() {
    let h = synthetic();
    let csv = horizon_csv(&h.horizon);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 361);
    assert_eq!(
        lines[0],
        "Bearing[deg];Eps_Min[deg];Distance[km];Elevation[m]"
    );
    assert_eq!(lines[1], "0;0.00000000;10.00000000;100.00000000");
    // 123 * 0.1 deg = 12.3 deg
    assert_eq!(lines[124], "123;12.30000000;133.00000000;223.00000000");
    assert!(lines.iter().all(|l| l.split(';').count() == 4));

    // the file goes to <dir>/horizon_<call>.csv; a '/' in the call does not make a directory
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    let path = export_csv(&tmp, "DL2ALF/P", &h.horizon).unwrap();
    assert_eq!(path, tmp.join("horizon_DL2ALF_P.csv"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), csv);
}

#[derive(Default)]
struct Rec {
    stored: Vec<(ElevationModel, LocalObstruction)>,
    horizon_requests: Vec<HorizonRequest>,
    cancels: usize,
    /// Stored station position the fake knows.
    station: Option<GPoint>,
}

struct Fake(Rc<RefCell<Rec>>);

impl Services for Fake {
    fn request_path(&mut self, _req: PathRequest) {}
    fn start_feed(
        &mut self,
        _id: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn station_position(&self, _call: &str, _loc: &str) -> Option<GPoint> {
        self.0.borrow().station
    }
    fn obstruction_load(
        &self,
        _model: ElevationModel,
        lat: f64,
        lon: f64,
    ) -> Result<LocalObstruction, String> {
        let mut o = LocalObstruction::new(lat, lon, None, None);
        o.distance[90] = 500.0;
        o.height[90] = 40.0;
        Ok(o)
    }
    fn obstruction_store(
        &mut self,
        model: ElevationModel,
        o: &LocalObstruction,
    ) -> Result<bool, String> {
        self.0.borrow_mut().stored.push((model, o.clone()));
        Ok(true)
    }
    fn start_horizon(&mut self, req: HorizonRequest) -> Result<(), String> {
        self.0.borrow_mut().horizon_requests.push(req);
        Ok(())
    }
    fn cancel_horizon(&mut self) {
        self.0.borrow_mut().cancels += 1;
    }
}

fn harness(rec: Rc<RefCell<Rec>>) -> Harness<'static, AppState> {
    let st = AppState::new(
        Settings::default(),
        None,
        Box::new(Fake(rec)),
        Arc::new(PlaneInfoCache::new()),
    );
    Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    )
}

#[test]
fn obstruction_ok_stores_cancel_does_not() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    let s = Settings::default();

    // Cancel stores nothing, even after an edit
    h.state_mut()
        .open_station_dialog(StationAction::Obstructions, &s);
    h.run();
    h.state_mut().obstructions_dlg.as_mut().unwrap().height[10] = 25.0;
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().obstructions_dlg.is_none());
    assert!(rec.borrow().stored.is_empty());

    // OK stores the edited obstruction for the effective model at My position
    h.state_mut()
        .open_station_dialog(StationAction::Obstructions, &s);
    h.run();
    {
        let d = h.state_mut().obstructions_dlg.as_mut().unwrap();
        assert_eq!(d.distance[90], 500.0, "the stored obstruction is shown");
        d.height[10] = 25.0;
    }
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().obstructions_dlg.is_none());
    let rec = rec.borrow();
    assert_eq!(rec.stored.len(), 1);
    let (model, o) = &rec.stored[0];
    assert_eq!(*model, s.elevation_enabled.effective_model());
    assert_eq!((o.lat, o.lon), (s.my_lat, s.my_lon));
    assert_eq!((o.height[10], o.distance[90]), (25.0, 500.0));
}

#[test]
fn obstruction_clear_all_zeroes_the_grid() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    h.state_mut()
        .open_station_dialog(StationAction::Obstructions, &Settings::default());
    h.run();
    h.get_by_label("Clear all").click();
    h.run();
    let d = h.state().obstructions_dlg.as_ref().unwrap();
    assert!(d.distance.iter().chain(&d.height).all(|&v| v == 0.0));
}

#[test]
fn obstruction_without_model_shows_error_and_stores_nothing() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    let mut s = Settings::default();
    s.elevation_enabled.globe = false;
    h.state_mut()
        .open_station_dialog(StationAction::Obstructions, &s);
    h.run();
    assert!(h.state().obstructions_dlg.as_ref().unwrap().error.is_some());
    h.get_by_label("Cancel").click();
    h.run();
    assert!(rec.borrow().stored.is_empty());
}

fn point(i: usize) -> HorizonPoint {
    HorizonPoint {
        dist: i as f64,
        eps_min: 0.01,
        elv: 5,
    }
}

#[test]
fn horizon_dialog_draws_out_of_order_points_and_cancels_on_close() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    h.state_mut().open_station_dialog(
        StationAction::Horizon(airscout::ui::options::stations::Who::My),
        &Settings::default(),
    );
    h.run();
    // no computation before Calculate
    assert!(rec.borrow().horizon_requests.is_empty());
    h.get_by_label("Calculate").click();
    h.run();
    let id = {
        let r = rec.borrow();
        assert_eq!(r.horizon_requests.len(), 1);
        let q = &r.horizon_requests[0];
        assert_eq!(q.call, "DL2ALF");
        assert!(q.with_obstruction);
        // no stored QRV: the band default antenna height
        assert_eq!(
            q.height_m,
            as_stations::default_antenna_height(Settings::default().band)
        );
        q.id
    };

    // azimuths arrive in any order; events of another request are dropped
    for az in [200, 5, 359, 0] {
        h.state_mut().on_event(ServiceEvent::HorizonPoint {
            id,
            az,
            point: point(az),
        });
    }
    h.state_mut().on_event(ServiceEvent::HorizonPoint {
        id: id + 1,
        az: 9,
        point: point(9),
    });
    h.run();
    let d = h.state().horizon_dlg.as_ref().unwrap();
    assert_eq!(d.state, HorizonState::Running);
    assert_eq!(d.points_drawn(), 4);

    // the complete horizon replaces the live points and enables the export
    h.state_mut().on_event(ServiceEvent::HorizonDone {
        id,
        horizon: Box::new(synthetic()),
    });
    h.run();
    let d = h.state().horizon_dlg.as_ref().unwrap();
    assert_eq!(d.state, HorizonState::Done);
    assert_eq!(d.points_drawn(), 360);

    // polar <-> cartesian is kept in the settings
    assert!(h.state().settings.gui.horizon_plot_polar);
    h.get_by_label("Cartesian").click();
    h.run();
    assert!(!h.state().settings.gui.horizon_plot_polar);

    // done: closing does not cancel anything
    h.get_by_label("Close").click();
    h.run();
    assert!(h.state().horizon_dlg.is_none());
    assert_eq!(rec.borrow().cancels, 0);
}

#[test]
fn closing_a_running_horizon_cancels_it() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    h.state_mut().open_station_dialog(
        StationAction::Horizon(airscout::ui::options::stations::Who::Dx),
        &Settings::default(),
    );
    h.run();
    h.get_by_label("Calculate").click();
    h.run();
    // DX has no local obstruction
    assert!(!rec.borrow().horizon_requests[0].with_obstruction);
    h.get_by_label("Close").click();
    h.run();
    assert!(h.state().horizon_dlg.is_none());
    assert_eq!(rec.borrow().cancels, 1);
}

#[test]
fn horizon_without_model_is_an_error_and_does_not_start() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    let mut s = Settings::default();
    s.elevation_enabled.globe = false;
    h.state_mut().open_station_dialog(
        StationAction::Horizon(airscout::ui::options::stations::Who::My),
        &s,
    );
    h.run();
    assert!(h.state().horizon_dlg.as_ref().unwrap().error().is_some());
    assert!(rec.borrow().horizon_requests.is_empty());
}

#[test]
fn stations_tab_buttons_open_the_dialogs() {
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Stations;
    h.run();
    h.get_by_label("Local obstructions...").click();
    h.run();
    assert!(h.state().obstructions_dlg.is_some());
    assert!(h.state().horizon_dlg.is_none());
}

#[test]
fn horizon_uses_the_stored_station_position_else_the_locator_centre() {
    use as_geo::maidenhead::{self, PositionInRectangle};
    let rec = Rc::new(RefCell::new(Rec::default()));
    let mut h = harness(rec.clone());
    // a position that is not the centre of its locator cell
    let s = Settings {
        my_lat: 50.51,
        my_lon: 11.03,
        ..Settings::default()
    };
    let loc = maidenhead::loc_from_lat_lon(s.my_lat, s.my_lon, false, 3, true)
        .unwrap()
        .unwrap();
    let (clat, clon) =
        maidenhead::lat_lon_from_loc(&loc, PositionInRectangle::MiddleMiddle).unwrap();
    let who = airscout::ui::options::stations::Who::My;

    // unknown station: the centre of the locator cell
    h.state_mut()
        .open_station_dialog(StationAction::Horizon(who), &s);
    h.run();
    h.get_by_label("Calculate").click();
    h.run();
    assert_eq!(
        rec.borrow().horizon_requests[0].pos,
        GPoint::new(clat, clon)
    );
    h.get_by_label("Close").click();
    h.run();

    // stored station: its position
    rec.borrow_mut().station = Some(GPoint::new(50.123, 11.456));
    h.state_mut()
        .open_station_dialog(StationAction::Horizon(who), &s);
    h.run();
    h.get_by_label("Calculate").click();
    h.run();
    assert_eq!(
        rec.borrow().horizon_requests[1].pos,
        GPoint::new(50.123, 11.456)
    );
}

#[test]
fn horizon_height_uses_the_effective_model_only() {
    // GLOBE has data, SRTM3 has none: the raw lookup in SRTM3 reads 0, the fallback walk of the
    // path code would read the GLOBE value
    let dir = common::data_dir_with_globe();
    let paths = open_data(dir.path()).unwrap();
    let pos = GPoint::new(50.5, 11.0);
    let globe = ground_elevation_raw(&paths.store, ElevationModel::Globe, pos).unwrap();
    assert!(globe > 0.0);
    assert_eq!(
        ground_elevation_raw(&paths.store, ElevationModel::Srtm3, pos).unwrap(),
        0.0
    );
}
