//! Multi tab and watchlist dialog, driven through egui_kittest (no rendering).
use airscout::app::{AppState, ControlTab, Services};
use airscout::services::{PathRequest, ServiceEvent};
use airscout::ui::dialogs;
use airscout::ui::multi::{self, watch_row_color};
use as_aircraft::PlaneInfoCache;
use as_geo::latlon::GRect;
use as_settings::{Settings, WatchlistItem};
use eframe::egui::accesskit::Role;
use eframe::egui::Color32;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Loads = Rc<RefCell<Vec<Option<GRect>>>>;

struct Fake {
    loads: Loads,
    /// Number of `station_position` queries (station database lookups).
    positions: Rc<std::cell::Cell<usize>>,
}

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
    fn load_stations(&mut self, area: Option<GRect>) {
        self.loads.borrow_mut().push(area);
    }
    fn station_position(&self, _call: &str, _loc: &str) -> Option<as_geo::latlon::GPoint> {
        self.positions.set(self.positions.get() + 1);
        None
    }
}

fn item(call: &str, loc: &str, checked: bool, oor: bool) -> WatchlistItem {
    WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        checked,
        out_of_range: oor,
        ..WatchlistItem::default()
    }
}

fn state(items: Vec<WatchlistItem>) -> (AppState, Loads) {
    let (st, loads, _) = state_counting(items);
    (st, loads)
}

fn state_counting(items: Vec<WatchlistItem>) -> (AppState, Loads, Rc<std::cell::Cell<usize>>) {
    let loads = Loads::default();
    let positions = Rc::new(std::cell::Cell::new(0));
    let mut s = Settings::default();
    s.watchlist.0 = items;
    let mut st = AppState::new(
        s,
        None,
        Box::new(Fake {
            loads: loads.clone(),
            positions: positions.clone(),
        }),
        Arc::new(PlaneInfoCache::new()),
    );
    st.control_tab = ControlTab::Multi;
    (st, loads, positions)
}

fn harness(st: AppState) -> Harness<'static, AppState> {
    Harness::new_ui_state(
        |ui, st: &mut AppState| {
            multi::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    )
}

#[test]
fn watch_row_color_thresholds() {
    assert_eq!(watch_row_color(0), None);
    assert_eq!(watch_row_color(1), Some(Color32::from_rgb(255, 165, 0)));
    assert_eq!(watch_row_color(51), Some(Color32::from_rgb(255, 0, 0)));
    assert_eq!(watch_row_color(76), Some(Color32::from_rgb(255, 0, 255)));
}

#[test]
fn oor_row_checkbox_disabled() {
    let (st, _) = state(vec![
        item("DL0AA", "JO50", false, false),
        item("OE1XX", "JN88", false, true),
    ]);
    let mut h = harness(st);
    h.run();
    // display order: DL0AA, OE1XX
    let disabled: Vec<bool> = h
        .get_all_by_role(Role::CheckBox)
        .map(|n| n.accesskit_node().is_disabled())
        .collect();
    assert_eq!(disabled, vec![false, true]);
}

#[test]
fn header_click_checks_all_in_range() {
    let (st, _) = state(vec![
        item("DL0AA", "JO50", false, false),
        item("OE1XX", "JN88", false, true),
        item("PA0ZZ", "JO22", false, false),
    ]);
    let mut h = harness(st);
    h.run();
    h.get_by_label("Call").click();
    h.run();
    let checked: Vec<bool> = h
        .state()
        .settings
        .watchlist
        .0
        .iter()
        .map(|i| i.checked)
        .collect();
    assert_eq!(checked, vec![true, false, true]);
}

#[test]
fn row_click_sets_dx_when_paused() {
    let (st, _) = state(vec![item("DL0AA", "JO50AA", false, false)]);
    let mut h = harness(st);
    h.run();
    h.get_by_label("DL0AA").click();
    h.run();
    assert_eq!(h.state().settings.dx_call, "DL0AA");
    assert_eq!(h.state().dx_loc_text, "JO50AA");
}

#[test]
fn watchlist_dialog_add_and_ok() {
    let (st, loads) = state(vec![item("DL0AA", "JO50AA", true, false)]);
    let mut h = harness(st);
    h.run();
    h.get_by_label("Manage").click();
    h.run();
    // the station list loads once per open and shows "loading…" until it arrives
    assert_eq!(loads.borrow().len(), 1);
    assert!(loads.borrow()[0].is_some());
    h.get_by_label("loading…");
    h.state_mut().on_event(ServiceEvent::Stations(vec![
        ("DL0AA".into(), "JO50AA".into()),
        ("OE1XX".into(), "JN88AA".into()),
    ]));
    h.run();
    h.get_by_label("OE1XX JN88AA").click();
    h.run();
    h.get_by_label("Add").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().watchlist_dlg.is_none());
    let w = &h.state().settings.watchlist.0;
    assert_eq!(w.len(), 2);
    assert_eq!((w[0].call.as_str(), w[0].checked), ("DL0AA", true));
    assert_eq!((w[1].call.as_str(), w[1].checked), ("OE1XX", false));
}

#[test]
fn watchlist_dialog_cancel_keeps_watchlist() {
    let (st, _) = state(vec![item("DL0AA", "JO50AA", true, false)]);
    let mut h = harness(st);
    h.run();
    h.get_by_label("Manage").click();
    h.run();
    h.get_by_label("Remove all").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(h.state().settings.watchlist.0.len(), 1);
}

#[test]
fn no_station_query_per_row_per_frame() {
    let items = (0..50)
        .map(|i| item(&format!("DL{i}AA"), "JO50AA", true, false))
        .collect();
    let (st, _, positions) = state_counting(items);
    let mut h = harness(st);
    for _ in 0..5 {
        h.run();
    }
    // no row hovered: no station database query at all
    assert_eq!(positions.get(), 0);
    // hovering one row queries for that row only: one query per frame
    h.get_by_label("DL0AA").hover();
    h.run();
    positions.set(0);
    h.step();
    assert_eq!(positions.get(), 1);
}
