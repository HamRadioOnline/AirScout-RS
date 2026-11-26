//! Options dialog, driven through egui_kittest (no rendering).
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::{Secret, Settings};
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Restarts = Rc<RefCell<Vec<Settings>>>;

struct Fake(Restarts);

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
    fn restart_background(&mut self, settings: &Settings, _generation: u64) {
        self.0.borrow_mut().push(settings.clone());
    }
}

fn harness(settings: Settings) -> (Harness<'static, AppState>, Restarts) {
    let r = Restarts::default();
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake(r.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    let h = Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    );
    (h, r)
}

fn open(h: &mut Harness<'static, AppState>) {
    h.get_by_label("Options").click();
    h.run();
    assert!(h.state().options_dlg.is_some());
}

fn draft<'a>(h: &'a mut Harness<'static, AppState>) -> &'a mut Settings {
    &mut h.state_mut().options_dlg.as_mut().unwrap().draft
}

#[test]
fn cancel_discards_edits() {
    let (mut h, restarts) = harness(Settings::default());
    let before = h.state().settings.clone();
    open(&mut h);
    draft(&mut h).area.min_lat = 40.0;
    draft(&mut h).gui.alarm_distance_km = 5.0;
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(h.state().settings, before);
    assert!(restarts.borrow().is_empty());
}

#[test]
fn ok_applies_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let (mut h, restarts) = harness(Settings::default());
    h.state_mut().settings_path = Some(path.clone());
    h.state_mut().settings.gui.side_panel_width = Some(333.0);
    open(&mut h);
    draft(&mut h).area.min_lat = 40.0;
    draft(&mut h).gui.alarm_distance_km = 5.0;
    draft(&mut h).gui.map_update_s = 3;
    h.get_by_label("OK").click();
    h.run();
    let st = h.state();
    assert!(st.options_dlg.is_none());
    assert_eq!(st.settings.area.min_lat, 40.0);
    assert_eq!(st.settings.gui.alarm_distance_km, 5.0);
    // live layout survives the apply
    assert_eq!(st.settings.gui.side_panel_width, Some(333.0));
    let r = restarts.borrow();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].area.min_lat, 40.0);
    let saved = Settings::load_or_default(&path).unwrap();
    assert_eq!(saved.area.min_lat, 40.0);
    assert_eq!(saved.gui.map_update_s, 3);
}

#[test]
fn invalid_area_blocks_ok() {
    let (mut h, restarts) = harness(Settings::default());
    open(&mut h);
    draft(&mut h).area.min_lat = 70.0; // above max_lat 60
    h.get_by_label("OK").click();
    h.run();
    let dlg = h.state().options_dlg.as_ref().expect("dialog stays open");
    assert!(dlg.errors.iter().any(|e| e.contains("latitude")));
    assert_eq!(h.state().settings.area.min_lat, 35.0);
    assert!(restarts.borrow().is_empty());
    h.get_by_label_contains("min latitude must be below");
}

#[test]
fn options_disabled_while_playing() {
    let (mut h, _) = harness(Settings::default());
    h.state_mut().play().unwrap();
    h.run();
    let b = h.get_by_role_and_label(Role::Button, "Options");
    assert!(b.accesskit_node().is_disabled());
    assert!(!h.state_mut().open_options_dlg());
    assert!(h.state().options_dlg.is_none());
}

#[test]
fn status_never_contains_key() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    draft(&mut h).gui.thunderforest_api_key = Secret::from("k3y-secret");
    draft(&mut h).gui.map_provider = "OCM Landscape".into();
    h.get_by_label("OK").click();
    h.run();
    let st = h.state();
    assert!(!st.status_line().contains("k3y-secret"));
    assert!(!format!("{:?}", st.settings).contains("k3y-secret"));
}

#[test]
fn every_tab_renders() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    for tab in [
        "Database",
        "Map",
        "Elevation",
        "Path",
        "Planes",
        "Alarm",
        "Network",
        "Track",
        "CAT",
        "SpecLab",
        "Watchlist",
        "Info",
        "General",
    ] {
        h.get_by_role_and_label(Role::Button, tab).click();
        h.run();
    }
    assert_eq!(
        h.state().options_dlg.as_ref().unwrap().tab,
        OptionsTab::General
    );
    h.get_by_role_and_label(Role::SpinButton, "Min Lat");
}

/// Options › Map: the preloader checkbox, the zoom spinner (enabled only with the checkbox) and
/// the tile estimate for the area (default area, zoom 5..=11: 74,916 tiles, the Task 1 golden).
#[test]
fn map_tab_preloader_controls() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Map;
    h.run();
    let estimate =
        "Preloads about 74916 tiles from www.airscout.eu (OpenStreetMap only); off by default.";
    h.get_by_label(estimate);
    assert!(h
        .get_by_role_and_label(Role::SpinButton, "down to Zoom Level:")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("Enable Map Preloading").click();
    h.run();
    assert!(draft(&mut h).gui.map_preloader_enabled);
    assert!(!h
        .get_by_role_and_label(Role::SpinButton, "down to Zoom Level:")
        .accesskit_node()
        .is_disabled());
    draft(&mut h).gui.map_preloader_max_zoom = 5;
    h.run();
    let n5 = {
        let a = Settings::default().area;
        as_maps::preload_tile_count(
            &as_geo::latlon::GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon),
            5,
            5,
        )
    };
    h.get_by_label(&format!(
        "Preloads about {n5} tiles from www.airscout.eu (OpenStreetMap only); off by default."
    ));
}
