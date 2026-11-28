//! Stations tab of the Options dialog and the station map dialog (no rendering).
use airscout::app::{AppState, RealServices, Services};
use airscout::services::{open_data, PathRequest, ServiceEvent};
use airscout::ui::dialogs::map_station::{MapStationDlg, MapStationResult, MapStationState};
use airscout::ui::options::stations::{station_key, QrvEdit, Who};
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::Settings;
use as_stations::{GeoSource, Location, Qrv};
use chrono::{DateTime, Utc};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Db {
    stored: Vec<Qrv>,
    writes: Vec<Qrv>,
    loads: usize,
    details: Vec<(u64, Who, GPoint)>,
    locations: Vec<Location>,
    fail_location_write: bool,
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
    fn location_find(&self, call: &str) -> Option<GPoint> {
        (call == "TE1ST").then_some(GPoint::new(50.01, 10.03))
    }
    fn request_options_station_details(
        &mut self,
        id: u64,
        who: Who,
        _enabled: as_settings::ElevationEnabled,
        pos: GPoint,
    ) {
        self.0.borrow_mut().details.push((id, who, pos));
    }
    fn write_options_station_edits(
        &mut self,
        locations: &[Location],
        edits: &[QrvEdit],
    ) -> Result<(), String> {
        let mut db = self.0.borrow_mut();
        if db.fail_location_write {
            return Err("synthetic write failure".into());
        }
        db.locations.extend_from_slice(locations);
        db.writes.extend(edits.iter().map(|edit| match edit {
            QrvEdit::Upsert(row) => row.clone(),
            QrvEdit::Delete { .. } => panic!("unexpected QRV deletion"),
        }));
        Ok(())
    }
    fn qrv_get_all(&self, call: &str, loc: &str) -> Vec<Qrv> {
        let mut db = self.0.borrow_mut();
        db.loads += 1;
        db.stored
            .iter()
            .filter(|q| q.call == call && q.loc == loc)
            .cloned()
            .collect()
    }
}

fn harness(settings: Settings, db: SharedDb) -> Harness<'static, AppState> {
    service_harness(settings, Box::new(Fake(db)))
}

fn service_harness(settings: Settings, services: Box<dyn Services>) -> Harness<'static, AppState> {
    let st = AppState::new(settings, None, services, Arc::new(PlaneInfoCache::new()));
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

    // An incomplete locator clears coordinates until it can be resolved.
    d.stations
        .my
        .set_loc("JO6", &mut d.draft.my_lat, &mut d.draft.my_lon);
    assert!(d.draft.my_lat.is_nan() && d.draft.my_lon.is_nan());
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

#[test]
fn qrv_written_only_on_ok() {
    let mut s = synthetic_stations();
    s.band = Band::B144M;
    let db = SharedDb::default();
    db.borrow_mut()
        .stored
        .push(Qrv::new("TE1ST", "JO50AA", s.band, 12.0, 10.0, 100.0, Utc::now()).unwrap());
    let mut h = harness(s, db.clone());

    for action in ["Cancel", "OK"] {
        open_stations(&mut h);
        assert_eq!(
            qrv_field(&h, Who::My, "Antenna Gain:").value().as_deref(),
            Some("10")
        );
        edit_qrv(&mut h, Who::My, ["12", "17", "100"]);
        h.get_by_label(action).click();
        h.run();
        assert!(h.state().options_dlg.is_none());
        assert_eq!(db.borrow().writes.len(), usize::from(action == "OK"));
    }
    let db = db.borrow();
    let w = &db.writes[0];
    assert_eq!(
        (w.call.as_str(), w.loc.as_str(), w.band),
        ("TE1ST", "JO50AA", Band::B144M)
    );
    assert_eq!(w.antenna_gain, 17.0);
}

fn qrv_field<'a>(
    h: &'a Harness<'static, AppState>,
    who: Who,
    label: &'a str,
) -> egui_kittest::Node<'a> {
    h.query_all_by_label(label)
        .filter(|node| node.accesskit_node().role() == eframe::egui::accesskit::Role::TextInput)
        .nth(usize::from(who == Who::Dx))
        .unwrap_or_else(|| panic!("missing {who:?} QRV input for {label}"))
}

const QRV_LABELS: [&str; 3] = ["Antenna Height:", "Antenna Gain:", "Power:"];

fn edit_qrv(h: &mut Harness<'static, AppState>, who: Who, values: [&str; 3]) {
    for (label, value) in QRV_LABELS.into_iter().zip(values) {
        qrv_field(h, who, label).focus();
        h.run();
        qrv_field(h, who, label).set_text_value_accesskit(value);
        h.run();
    }
}

#[test]
fn band_step_preserves_qrv_edits() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    let band = dlg(&mut h).draft.band;
    for label in QRV_LABELS {
        assert_eq!(qrv_field(&h, Who::My, label).value().as_deref(), Some(""));
    }
    edit_qrv(&mut h, Who::My, ["10", "12", "50"]);
    edit_qrv(&mut h, Who::Dx, ["0", "0", "0"]);
    h.get_by_label("▲").click();
    h.run();
    assert_eq!(dlg(&mut h).draft.band, band.next());
    h.get_by_label("▼").click();
    h.run();
    assert_eq!(dlg(&mut h).draft.band, band);
    for (label, value) in QRV_LABELS.into_iter().zip(["10", "12", "50"]) {
        assert_eq!(
            qrv_field(&h, Who::My, label).value().as_deref(),
            Some(value)
        );
        assert_eq!(qrv_field(&h, Who::Dx, label).value().as_deref(), Some("0"));
    }
    assert!(db.borrow().writes.is_empty());
    h.get_by_label("OK").click();
    h.run();
    let db = db.borrow();
    assert_eq!(db.writes.len(), 2);
    assert_eq!(db.writes[0].call, "TE1ST");
    assert_eq!(db.writes[1].call, "TE2ST");
    assert_eq!(db.writes[1].power, 0.0);
}

#[test]
fn partial_qrv_refuses_tab_change() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    edit_qrv(&mut h, Who::My, ["", "", "50"]);
    let band = dlg(&mut h).draft.band;
    assert!(!dlg(&mut h).try_select_tab(OptionsTab::Map));
    h.get_by_label("▲").click();
    h.run();
    assert_eq!(dlg(&mut h).draft.band, band);
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(dlg(&mut h).tab, OptionsTab::Stations);
    assert!(db.borrow().writes.is_empty() && db.borrow().locations.is_empty());

    edit_qrv(&mut h, Who::My, ["10", "inf", "50"]);
    assert!(!dlg(&mut h).try_select_tab(OptionsTab::Map));
    edit_qrv(&mut h, Who::My, ["10", "12", "50"]);
    assert!(dlg(&mut h).try_select_tab(OptionsTab::Map));
}

#[test]
fn invalid_station_position_blocks_band_tab_and_ok() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    let d = dlg(&mut h);
    d.stations
        .dx
        .set_loc("JO6", &mut d.draft.dx_lat, &mut d.draft.dx_lon);
    let band = d.draft.band;
    h.run();
    assert!(!dlg(&mut h).try_select_tab(OptionsTab::Map));
    h.get_by_label("▲").click();
    h.run();
    assert_eq!(dlg(&mut h).draft.band, band);
    h.get_by_label("OK").click();
    h.run();
    assert!(dlg(&mut h).status_texts().1.contains("valid DX station"));
    assert!(db.borrow().locations.is_empty());
}

#[test]
fn ok_from_other_tab_ignores_untouched_invalid_station() {
    // The main window leaves the DX position at NaN for a call the station database does not
    // know (`cb_DXCall_TextChanged`). C# checks the stations only when the Stations page is
    // left (`tab_Options_Stations_Validating`, `OptionsDlg.cs:868-873`), so OK on another page
    // still applies an unrelated edit.
    let mut s = synthetic_stations();
    s.dx_lat = f64::NAN;
    s.dx_lon = f64::NAN;
    let airports = s.gui.map_show_airports;
    let db = SharedDb::default();
    let mut h = harness(s, db.clone());
    h.get_by_label("Options").click();
    h.run();
    assert_eq!(dlg(&mut h).tab, OptionsTab::General);
    dlg(&mut h).tab = OptionsTab::Map;
    h.run();
    h.get_by_label("Airports").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let st = h.state();
    assert!(st.options_dlg.is_none(), "OK closes the dialog");
    assert_eq!(st.settings.gui.map_show_airports, !airports);
    assert!(st.settings.dx_lat.is_nan());
    let db = db.borrow();
    assert!(db.locations.is_empty() && db.writes.is_empty());
}

#[test]
fn qrv_edits_follow_call_and_locator_keys() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    edit_qrv(&mut h, Who::Dx, ["10", "12", "50"]);

    dlg(&mut h).draft.dx_call = "TE1ST".into();
    h.run();
    assert_eq!(
        qrv_field(&h, Who::Dx, "Power:").value().as_deref(),
        Some("")
    );
    let d = dlg(&mut h);
    d.draft.dx_call = "TE2ST".into();
    h.run();
    for (loc, expected) in [("JO50AA", ""), ("jo60aa", "50"), ("JO50AA", "")] {
        let d = dlg(&mut h);
        d.stations
            .dx
            .set_loc(loc, &mut d.draft.dx_lat, &mut d.draft.dx_lon);
        h.run();
        assert_eq!(
            qrv_field(&h, Who::Dx, "Power:").value().as_deref(),
            Some(expected)
        );
    }
    h.get_by_label("OK").click();
    h.run();
    let db = db.borrow();
    assert_eq!(db.locations.len(), 1);
    assert_eq!(db.locations[0].loc, "JO50AA");
    assert_eq!(db.writes.len(), 1);
    assert_eq!(db.writes[0].call, "TE2ST");
    assert_eq!(db.writes[0].loc, "JO60AA");
    assert_eq!(db.writes[0].power, 50.0);
}

fn position_edit_harness() -> Harness<'static, AppState> {
    let s = synthetic_stations();
    let db = SharedDb::default();
    db.borrow_mut()
        .stored
        .push(Qrv::new("TE2ST", "JO50AA", s.band, 10.0, 12.0, 50.0, Utc::now()).unwrap());
    let mut h = harness(s, db);
    open_stations(&mut h);
    assert_eq!(
        qrv_field(&h, Who::Dx, "Power:").value().as_deref(),
        Some("")
    );
    h
}

fn dx_locator<'a>(h: &'a Harness<'static, AppState>) -> egui_kittest::Node<'a> {
    h.query_all_by_label("Loc:")
        .filter(|node| node.accesskit_node().role() == eframe::egui::accesskit::Role::TextInput)
        .nth(1)
        .unwrap()
}

fn assert_dx_qrv_repaint(h: &mut Harness<'static, AppState>) {
    assert_eq!(dlg(h).stations.dx.qrv_key.as_ref().unwrap().1, "JO50AA");
    h.step();
    assert!(
        h.ctx
            .repaint_causes()
            .iter()
            .any(|cause| cause.file.ends_with("ui/dialogs/mod.rs")),
        "Options should request a repaint after refreshing the QRV key"
    );
    assert_eq!(
        qrv_field(h, Who::Dx, "Power:").value().as_deref(),
        Some("50")
    );
}

#[test]
fn locator_edit_repaints_the_loaded_qrv_row() {
    let mut h = position_edit_harness();
    dx_locator(&h).focus();
    h.run();
    dx_locator(&h).set_text_value_accesskit("JO50AA");
    h.step();
    assert_dx_qrv_repaint(&mut h);
}

#[test]
fn map_confirmation_repaints_the_loaded_qrv_row() {
    let mut h = position_edit_harness();
    h.query_all_by_label("\nMap").nth(1).unwrap().click();
    h.run();
    h.get_by_label("Locator").focus();
    h.run();
    h.get_by_label("Locator").set_text_value_accesskit("JO50AA");
    h.run();
    h.query_all_by_label("OK").last().unwrap().click_accesskit();
    h.step();
    assert!(dlg(&mut h).map_dialog().is_none());
    assert_dx_qrv_repaint(&mut h);
}

#[test]
fn correcting_qrv_fields_clears_band_step_error() {
    let mut h = harness(synthetic_stations(), SharedDb::default());
    open_stations(&mut h);
    edit_qrv(&mut h, Who::My, ["10", "", "50"]);
    h.get_by_label("▲").click();
    h.run();
    assert!(dlg(&mut h).status_texts().1.contains("finite numbers"));

    edit_qrv(&mut h, Who::My, ["10", "12", "50"]);
    assert!(dlg(&mut h).status_texts().1.is_empty());
    dx_locator(&h).focus();
    h.run();
    dx_locator(&h).set_text_value_accesskit("JO6");
    h.run();
    h.get_by_label("▲").click();
    h.run();
    assert!(dlg(&mut h).status_texts().1.contains("valid DX station"));
    dx_locator(&h).focus();
    h.run();
    dx_locator(&h).set_text_value_accesskit("JO60AA");
    h.run();
    h.get_by_label("Map").click();
    h.run();
    assert_eq!(dlg(&mut h).tab, OptionsTab::Map);
    assert!(dlg(&mut h).status_texts().1.is_empty());
}

#[test]
fn cached_partial_qrv_blocks_tab_change_and_ok() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    dlg(&mut h).draft.band = Band::B432M;
    h.run();
    edit_qrv(&mut h, Who::Dx, ["", "", "50"]);
    dx_locator(&h).focus();
    h.run();
    dx_locator(&h).set_text_value_accesskit("JO50AA");
    h.run();
    assert_eq!(
        qrv_field(&h, Who::Dx, "Power:").value().as_deref(),
        Some("")
    );

    let assert_error = |d: &airscout::ui::options::OptionsDlg| {
        let status = d.status_texts().1;
        for text in ["DX", Band::B432M.name(), "TE2ST", "JO60AA"] {
            assert!(status.contains(text), "missing {text} in {status:?}");
        }
    };
    assert!(!dlg(&mut h).try_select_tab(OptionsTab::Map));
    assert_error(dlg(&mut h));
    h.get_by_label("OK").click();
    h.run();
    assert_error(dlg(&mut h));
    assert!(db.borrow().writes.is_empty() && db.borrow().locations.is_empty());

    dx_locator(&h).focus();
    h.run();
    dx_locator(&h).set_text_value_accesskit("JO60AA");
    h.run();
    edit_qrv(&mut h, Who::Dx, ["10", "12", "50"]);
    assert!(dlg(&mut h).try_select_tab(OptionsTab::Map));
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(db.borrow().writes.len(), 1);
}

#[test]
fn empty_qrv_deletes_only_loaded_row() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let s = synthetic_stations();
    let band = s.band;
    let loaded_at: DateTime<Utc> = "2026-11-27T12:34:56Z".parse().unwrap();
    let my = Qrv::new("TE1ST", "JO50AA", band, 10.0, 12.0, 50.0, loaded_at).unwrap();
    let dx = Qrv::new("TE2ST", "JO60AA", band, 0.0, 0.0, 0.0, loaded_at).unwrap();
    paths
        .stations
        .qrv_bulk_insert_or_update_if_newer(&[my.clone(), dx.clone()])
        .unwrap();
    let st = &paths.stations;
    let mut h = service_harness(s, Box::new(RealServices::new(paths.clone(), tx)));
    let find_my = || st.qrv_find("TE1ST", "JO50AA", band).unwrap();
    for action in ["Cancel", "OK"] {
        open_stations(&mut h);
        assert_eq!(h.query_all_by_label("2026-11-27 12:34:56").count(), 2);
        edit_qrv(&mut h, Who::My, ["", "", ""]);
        assert_eq!(find_my(), Some(my.clone()));
        h.get_by_label(action).click();
        h.run();
        assert!(h.state().options_dlg.is_none());
        assert_eq!(find_my(), (action == "Cancel").then(|| my.clone()));
        assert_eq!(
            st.qrv_find("TE2ST", "JO60AA", band).unwrap(),
            Some(dx.clone())
        );
    }
}

#[test]
fn options_qrv_user_edit_marker_tracks_actual_writes() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let st = &paths.stations;
    let marker = || {
        st.table_timestamp(as_stations::QRV_USER_EDIT_MARKER)
            .unwrap()
    };
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut h = service_harness(
        synthetic_stations(),
        Box::new(RealServices::new(paths.clone(), tx)),
    );
    let mut last = None;
    for (values, changed) in [
        (["", "", ""], false),
        (["10", "12", "50"], true),
        (["10", "12", "50"], false),
        (["", "", ""], true),
    ] {
        open_stations(&mut h);
        edit_qrv(&mut h, Who::My, values);
        h.get_by_label("OK").click();
        h.run();
        assert!(h.state().options_dlg.is_none());
        let current = marker();
        if changed {
            assert!(current > last);
        } else {
            assert_eq!(current, last);
        }
        last = current;
    }
    assert_eq!(st.table_timestamp("QRV").unwrap(), None);
}

#[test]
fn qrv_not_reloaded_every_frame() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    let n = db.borrow().loads;
    h.run();
    h.run();
    assert_eq!(db.borrow().loads, n);
    // Both roles share a cached station when the normalized key matches.
    dlg(&mut h).draft.dx_call = "TE1ST".into();
    h.run();
    assert_eq!(db.borrow().loads, n);
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
    h.query_all_by_label("\nMap").next().unwrap().click();
    h.run();
    assert!(h.query_by_label("Station DL2ALF").is_some());
}

fn synthetic_stations() -> Settings {
    Settings {
        my_call: "TE1ST".into(),
        dx_call: "TE2ST".into(),
        my_lat: 50.01,
        my_lon: 10.03,
        dx_lat: 50.01,
        dx_lon: 12.03,
        ..Settings::default()
    }
}

#[test]
fn options_lookup_hit_then_miss() {
    let db = SharedDb::default();
    let mut s = synthetic_stations();
    s.my_call = "TE2ST".into();
    s.my_lon = 12.03;
    let mut h = harness(s, db.clone());
    open_stations(&mut h);
    dlg(&mut h).draft.my_call = "TE1ST".into();
    h.run();
    let (id, _, pos) = *db
        .borrow()
        .details
        .iter()
        .rev()
        .find(|(_, who, _)| *who == Who::My)
        .unwrap();
    assert_eq!(pos, GPoint::new(50.01, 10.03));
    h.state_mut().on_event(ServiceEvent::OptionsStationDetails {
        id,
        who: Who::My,
        elevation_m: Some(123),
        obstruction: Some(false),
    });
    assert_eq!(dlg(&mut h).stations.my.elevation_m, Some(123));
    assert_eq!(
        dlg(&mut h).stations.my.qrv_key.as_ref().unwrap().1,
        "JO50AA"
    );
    dlg(&mut h).draft.my_call = "TE2ST".into();
    h.run();
    let d = dlg(&mut h);
    assert!(d.stations.my.loc.is_empty());
    assert!(d.stations.my.elevation_m.is_none());
    assert!(!maidenhead::check_lat_lon(d.draft.my_lat, d.draft.my_lon));
    assert!(d.stations.my.qrv_key.is_none());
}

#[test]
fn stale_station_details_are_ignored() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    let my_id = || {
        db.borrow()
            .details
            .iter()
            .rev()
            .find(|(_, who, _)| *who == Who::My)
            .unwrap()
            .0
    };
    let first = my_id();
    dlg(&mut h).draft.my_lat += 0.001;
    h.run();
    let second = my_id();
    assert_ne!(first, second);
    for (id, elevation_m) in [(second, 123), (first, 42)] {
        h.state_mut().on_event(ServiceEvent::OptionsStationDetails {
            id,
            who: Who::My,
            elevation_m: Some(elevation_m),
            obstruction: None,
        });
    }
    assert_eq!(dlg(&mut h).stations.my.elevation_m, Some(123));
}

#[test]
fn locator_settings_keep_storage_key() {
    let db = SharedDb::default();
    let mut s = synthetic_stations();
    s.gui.locator_max_length = 6;
    let mut h = harness(s, db);
    open_stations(&mut h);
    let d = dlg(&mut h);
    assert_eq!(d.stations.my.loc.len(), 6);
    let key = d.stations.my.qrv_key.clone().unwrap();
    d.draft.gui.locator_max_length = 14;
    d.draft.gui.locator_auto_length = false;
    h.run();
    let d = dlg(&mut h);
    assert_eq!(d.stations.my.loc.len(), 14);
    assert_eq!(d.stations.my.qrv_key.as_ref(), Some(&key));
}

fn move_dx_on_map(h: &mut Harness<'static, AppState>, confirm: bool) {
    h.query_all_by_label("\nMap").nth(1).unwrap().click();
    h.run();
    dlg(h)
        .map_dialog()
        .unwrap()
        .state
        .drag_to(GPoint::new(50.02, 12.04));
    h.query_all_by_label(if confirm { "OK" } else { "Cancel" })
        .last()
        .unwrap()
        .click();
    h.run();
    assert!(dlg(h).map_dialog().is_none());
}

#[test]
fn station_map_stages_local_write() {
    let db = SharedDb::default();
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    move_dx_on_map(&mut h, false);
    assert!(dlg(&mut h).pending_locations(Utc::now()).is_empty());

    move_dx_on_map(&mut h, true);
    let rows = dlg(&mut h).pending_locations(Utc::now());
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].call.as_str(), rows[0].loc.as_str()),
        ("TE2ST", "JO60AA")
    );
    assert_eq!(rows[0].source, GeoSource::FromUser);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(db.borrow().locations.is_empty());

    open_stations(&mut h);
    move_dx_on_map(&mut h, true);
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(db.borrow().locations.len(), 1);
}

#[test]
fn station_write_error_preserves_draft_for_retry() {
    let db = SharedDb::default();
    db.borrow_mut().fail_location_write = true;
    let mut h = harness(synthetic_stations(), db.clone());
    open_stations(&mut h);
    move_dx_on_map(&mut h, true);
    edit_qrv(&mut h, Who::My, ["10", "12", "50"]);
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().settings.dx_lat, 50.01);
    let d = dlg(&mut h);
    assert_eq!(d.draft.dx_lat, 50.02);
    assert_eq!(d.status_texts().1, "Cannot save station edits.");

    assert!(db.borrow().locations.is_empty() && db.borrow().writes.is_empty());
    assert_eq!(
        qrv_field(&h, Who::My, "Power:").value().as_deref(),
        Some("50")
    );
    db.borrow_mut().fail_location_write = false;
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(h.state().settings.dx_lat, 50.02);
    assert_eq!(db.borrow().locations.len(), 1);
    assert_eq!(db.borrow().writes.len(), 1);
}

fn qrz_round_trip(
    pos: GPoint,
    edit: impl FnOnce(&mut Harness<'static, AppState>),
) -> Harness<'static, AppState> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut s = synthetic_stations();
    s.gui.qrz_url_database = format!("http://{}/db/", listener.local_addr().unwrap());
    s.my_lat = 50.02;
    s.my_lon = 10.04;
    let (requested, request) = channel();
    let (respond, response) = channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut lines = BufReader::new(&socket).lines();
        assert_eq!(lines.next().unwrap().unwrap(), "GET /db/TE1ST HTTP/1.1");
        let headers: Vec<_> = lines
            .map(Result::unwrap)
            .take_while(|line| !line.is_empty())
            .collect();
        assert!(headers.iter().any(|line| {
            line.split_once(": ").is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("user-agent") && value == as_net::user_agent()
            })
        }));
        requested.send(()).unwrap();
        response.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = format!("cs_lat = \"{}\";\r\ncs_lon = \"{}\";\r\n", pos.lat, pos.lon);
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = channel();
    let mut h = service_harness(
        s,
        Box::new(RealServices::new(open_data(dir.path()).unwrap(), tx)),
    );
    open_stations(&mut h);
    {
        let d = dlg(&mut h);
        d.stations
            .my
            .set_loc("JO50AA", &mut d.draft.my_lat, &mut d.draft.my_lon);
    }
    h.run();
    assert!(
        request.try_recv().is_err(),
        "opening and editing made no QRZ request"
    );
    h.query_all_by_label("QRZ lookup").next().unwrap().click();
    h.run();
    request.recv_timeout(Duration::from_secs(5)).unwrap();
    edit(&mut h);
    respond.send(()).unwrap();
    loop {
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let complete = matches!(event, ServiceEvent::QrzLookup { .. });
        h.state_mut().on_event(event);
        if complete {
            break;
        }
    }
    h.run();
    server.join().unwrap();
    h
}

#[test]
fn qrz_lookup_hit_updates_position() {
    let pos = GPoint::new(50.01, 10.03);
    let mut h = qrz_round_trip(pos, |_| {});
    let d = dlg(&mut h);
    assert_eq!((d.draft.my_lat, d.draft.my_lon), (pos.lat, pos.lon));
    assert_eq!(
        d.status_texts().1,
        "Position update from QRZ.com was performed succesfully."
    );
}

#[test]
fn qrz_lookup_mismatch_keeps_position() {
    let mut h = qrz_round_trip(GPoint::new(50.01, 12.03), |_| {});
    let d = dlg(&mut h);
    assert_eq!(
        (d.draft.my_lat, d.draft.my_lon),
        maidenhead::lat_lon_from_loc("JO50AA", PositionInRectangle::MiddleMiddle).unwrap()
    );
    assert_eq!(d.status_texts().1,
        "Position query at QRZ.com failed or does not match with current locator. \nNo update was performed.");
}

#[test]
fn stale_qrz_reply_is_ignored() {
    let mut h = qrz_round_trip(GPoint::new(50.01, 10.03), |h| {
        dlg(h).draft.my_call = "TE2ST".into();
        h.run();
        let d = dlg(h);
        d.stations
            .my
            .set_loc("JO60AA", &mut d.draft.my_lat, &mut d.draft.my_lon);
        h.run();
    });
    let d = dlg(&mut h);
    assert_eq!(d.draft.my_call, "TE2ST");
    assert_eq!(
        (d.draft.my_lat, d.draft.my_lon),
        maidenhead::lat_lon_from_loc("JO60AA", PositionInRectangle::MiddleMiddle).unwrap()
    );
    assert!(d.status_texts().1.is_empty());
}
