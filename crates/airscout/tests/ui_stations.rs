//! Stations tab of the Options dialog and the station map dialog (no rendering).
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::dialogs::map_station::{MapStationDlg, MapStationResult, MapStationState};
use airscout::ui::options::stations::station_key;
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::Settings;
use as_stations::Qrv;
use chrono::Utc;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Db {
    stored: Vec<Qrv>,
    writes: Vec<Qrv>,
    loads: usize,
}

type SharedDb = Rc<RefCell<Db>>;

struct Fake(SharedDb);

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
    fn qrv_get_all(&self, call: &str, loc: &str) -> Vec<Qrv> {
        let mut db = self.0.borrow_mut();
        db.loads += 1;
        db.stored
            .iter()
            .filter(|q| q.call == call && q.loc == loc)
            .cloned()
            .collect()
    }
    fn qrv_write(&mut self, qrvs: &[Qrv]) -> Result<usize, String> {
        self.0.borrow_mut().writes.extend_from_slice(qrvs);
        Ok(qrvs.len())
    }
}

fn harness(settings: Settings, db: SharedDb) -> Harness<'static, AppState> {
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake(db)),
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

fn open_stations(h: &mut Harness<'static, AppState>) {
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Stations;
    h.run();
}

fn dlg<'a>(h: &'a mut Harness<'static, AppState>) -> &'a mut airscout::ui::options::OptionsDlg {
    h.state_mut().options_dlg.as_mut().unwrap()
}

#[test]
fn locator_and_latlon_sync() {
    let mut h = harness(Settings::default(), SharedDb::default());
    open_stations(&mut h);

    // lat/lon -> locator
    {
        let d = dlg(&mut h);
        d.draft.my_lat = 52.5;
        d.draft.my_lon = 13.4;
    }
    h.run();
    // a position more precise than 6 characters gets a longer locator (Locator_MaxLength 10)
    let want = maidenhead::loc_from_lat_lon(52.5, 13.4, false, 5, true)
        .unwrap()
        .unwrap();
    assert_eq!(want.len(), 10);
    assert_eq!(dlg(&mut h).stations.my.loc, want);

    // locator -> lat/lon (centre of the cell), and the locator text stays as typed
    {
        let d = dlg(&mut h);
        d.stations
            .my
            .set_loc("JO62qm", &mut d.draft.my_lat, &mut d.draft.my_lon);
    }
    h.run();
    let (lat, lon) =
        maidenhead::lat_lon_from_loc("JO62QM", PositionInRectangle::MiddleMiddle).unwrap();
    let d = dlg(&mut h);
    assert_eq!((d.draft.my_lat, d.draft.my_lon), (lat, lon));
    assert_eq!(d.stations.my.loc, "JO62qm");

    // an invalid locator leaves the position alone
    d.stations
        .my
        .set_loc("JO6", &mut d.draft.my_lat, &mut d.draft.my_lon);
    assert_eq!((d.draft.my_lat, d.draft.my_lon), (lat, lon));
}

#[test]
fn invalid_call_marked() {
    let mut h = harness(Settings::default(), SharedDb::default());
    open_stations(&mut h);
    assert!(h.query_by_label("My call is not valid").is_none());
    dlg(&mut h).draft.my_call = "!!".into();
    h.run();
    assert!(h.query_by_label("My call is not valid").is_some());
    assert!(h.query_by_label("DX call is not valid").is_none());
    assert_eq!(station_key("!!", 50.0, 10.0), None);
}

fn stored_144(call: &str, loc: &str, gain: f64) -> Qrv {
    Qrv::new(call, loc, Band::B144M, 12.0, gain, 100.0, Utc::now()).unwrap()
}

fn set_gain_144(h: &mut Harness<'static, AppState>, gain: f64) {
    let set = &mut dlg(h).stations.my.qrv;
    let row = set.rows.iter_mut().find(|r| r.band == Band::B144M).unwrap();
    row.gain = gain;
}

#[test]
fn qrv_written_only_on_ok() {
    let s = Settings::default();
    let loc = s.my_loc().unwrap();
    let db = SharedDb::default();
    db.borrow_mut()
        .stored
        .push(stored_144("DL2ALF", &loc, 10.0));
    let mut h = harness(s, db.clone());

    // Cancel: nothing is written
    open_stations(&mut h);
    {
        let set = &dlg(&mut h).stations.my.qrv;
        assert_eq!(set.key, Some(("DL2ALF".into(), loc.clone())));
        let row = set.rows.iter().find(|r| r.band == Band::B144M).unwrap();
        assert_eq!(row.gain, 10.0, "the stored value is shown");
    }
    set_gain_144(&mut h, 17.0);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert!(db.borrow().writes.is_empty());

    // OK: only the changed row is written
    open_stations(&mut h);
    set_gain_144(&mut h, 17.0);
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    let db = db.borrow();
    assert_eq!(db.writes.len(), 1);
    let w = &db.writes[0];
    assert_eq!(
        (w.call.as_str(), w.loc.as_str(), w.band),
        ("DL2ALF", loc.as_str(), Band::B144M)
    );
    assert_eq!(w.antenna_gain, 17.0);
}

#[test]
fn qrv_not_reloaded_every_frame() {
    let db = SharedDb::default();
    let mut h = harness(Settings::default(), db.clone());
    open_stations(&mut h);
    let n = db.borrow().loads;
    h.run();
    h.run();
    assert_eq!(db.borrow().loads, n);
    // a changed call reloads
    dlg(&mut h).draft.dx_call = "DL1ABC".into();
    h.run();
    assert_eq!(db.borrow().loads, n + 1);
}

#[test]
fn map_station_drag_updates_fields() {
    let mut s = MapStationState::new("DL2ALF", 50.0, 10.0);
    s.drag_to(GPoint::new(52.5, 13.4));
    assert_eq!((s.lat, s.lon), (52.5, 13.4));
    let want = maidenhead::loc_from_lat_lon(52.5, 13.4, false, 3, false)
        .unwrap()
        .unwrap();
    assert_eq!(s.loc, want);
    // the polygon is the locator cell around the marker
    let poly = s.polygon();
    assert_eq!(poly.len(), 4);
    let (lo_lat, hi_lat) = (poly[0].lat.min(poly[2].lat), poly[0].lat.max(poly[2].lat));
    let (lo_lon, hi_lon) = (poly[0].lon.min(poly[2].lon), poly[0].lon.max(poly[2].lon));
    assert!((lo_lat..=hi_lat).contains(&52.5) && (lo_lon..=hi_lon).contains(&13.4));
    // not finite: ignored; out of range: clamped
    s.drag_to(GPoint::new(f64::NAN, 1.0));
    assert_eq!((s.lat, s.lon), (52.5, 13.4));
    s.drag_to(GPoint::new(95.0, 200.0));
    assert!(s.lat < 90.0 && s.lon < 180.0);
    // a typed locator moves the marker to its centre
    assert!(s.set_loc("JO50aa"));
    assert_eq!(s.loc, "JO50AA");
    assert!(!s.set_loc("JO5"));
    assert_eq!(s.loc, "JO50AA");
}

struct DlgHarness {
    dlg: MapStationDlg,
    settings: Settings,
    result: MapStationResult,
}

#[test]
fn map_dialog_ok_returns_position_cancel_does_not() {
    for (button, want) in [
        ("OK", MapStationResult::Ok(GPoint::new(51.0, 11.0))),
        ("Cancel", MapStationResult::Cancel),
    ] {
        let mut h = Harness::new_ui_state(
            |ui, st: &mut DlgHarness| {
                let r = st.dlg.show(ui.ctx(), &st.settings);
                if r != MapStationResult::Open {
                    st.result = r;
                }
            },
            DlgHarness {
                dlg: MapStationDlg::new("DL2ALF", 50.0, 10.0, None),
                settings: Settings::default(),
                result: MapStationResult::Open,
            },
        );
        h.run();
        h.state_mut().dlg.state.drag_to(GPoint::new(51.0, 11.0));
        h.get_by_label(button).click();
        h.run();
        assert_eq!(h.state().result, want);
    }
}

#[test]
fn map_button_opens_station_map() {
    let mut h = harness(Settings::default(), SharedDb::default());
    open_stations(&mut h);
    h.get_by_label("My map...").click();
    h.run();
    assert!(h.query_by_label("Station DL2ALF").is_some());
}
