//! First-run wizard: page rules, finish/abort, and that no service starts before Finish.
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::dialogs::wizard::{terms_text, WizardPage, WizardState};
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Log {
    bg_starts: usize,
    bg_stops: usize,
    feed_starts: usize,
    nfs: Vec<f64>,
    my: Vec<(String, f64, f64)>,
    speclab: usize,
    stations: usize,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, _req: PathRequest) {}
    fn start_feed(&mut self, _: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        self.0.borrow_mut().feed_starts += 1;
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn start_background(&mut self, _: &Settings, _: u64) {
        self.0.borrow_mut().bg_starts += 1;
    }
    fn stop_background(&mut self) {
        self.0.borrow_mut().bg_stops += 1;
    }
    fn set_near_field_suppression(&mut self, m: f64) {
        self.0.borrow_mut().nfs.push(m);
    }
    fn set_my(&mut self, call: &str, lat: f64, lon: f64) {
        self.0.borrow_mut().my.push((call.into(), lat, lon));
    }
    fn start_speclab(&mut self, _: &Settings, _: Arc<std::sync::atomic::AtomicBool>) {
        self.0.borrow_mut().speclab += 1;
    }
    fn load_stations(&mut self, _: Option<as_geo::latlon::GRect>) {
        self.0.borrow_mut().stations += 1;
    }
}

fn app(settings: Settings, rec: &Rec, path: &std::path::Path) -> AppState {
    let mut s = AppState::new(
        settings,
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    );
    s.settings_path = Some(path.to_path_buf());
    s
}

fn fresh() -> WizardState {
    WizardState::new(&Settings::default())
}

#[test]
fn terms_must_be_accepted() {
    let mut w = fresh();
    assert_eq!(w.page, WizardPage::Terms);
    assert!(w.next().is_err());
    assert_eq!(w.page, WizardPage::Terms);
    w.terms_accepted = true;
    assert!(w.next().is_ok());
    assert_eq!(w.page, WizardPage::Coverage);
    w.back();
    assert_eq!(w.page, WizardPage::Terms);
    w.back();
    assert_eq!(w.page, WizardPage::Terms);
}

#[test]
fn terms_text_has_no_bom() {
    let t = terms_text();
    assert!(!t.starts_with('\u{feff}'));
    assert!(!t.trim().is_empty());
}

#[test]
fn invalid_area_blocks_next() {
    let mut w = fresh();
    w.terms_accepted = true;
    w.next().unwrap();
    w.draft.area.min_lat = 60.0;
    w.draft.area.max_lat = 50.0;
    assert!(w.next().is_err());
    assert_eq!(w.page, WizardPage::Coverage);
}

#[test]
fn no_model_blocks_next() {
    let mut w = fresh();
    w.terms_accepted = true;
    w.next().unwrap();
    w.next().unwrap();
    assert_eq!(w.page, WizardPage::Elevation);
    w.draft.elevation_enabled = Default::default();
    w.draft.elevation_enabled.globe = false;
    assert!(w.next().is_err());
    assert_eq!(w.page, WizardPage::Elevation);
    w.draft.elevation_enabled.srtm3 = true;
    assert!(w.next().is_ok());
    assert_eq!(w.page, WizardPage::User);
}

#[test]
fn invalid_call_blocks_next() {
    let mut w = fresh();
    w.terms_accepted = true;
    w.next().unwrap();
    w.next().unwrap();
    w.next().unwrap();
    assert_eq!(w.page, WizardPage::User);
    w.draft.my_call = "!!".into();
    assert!(w.next().is_err());
    assert_eq!(w.page, WizardPage::User);
    w.draft.my_call = " dl2alf ".into();
    assert!(w.next().is_ok());
    assert_eq!(w.draft.my_call, "DL2ALF");
    assert_eq!(w.page, WizardPage::Feed);
}

#[test]
fn locator_and_position_follow_each_other() {
    let mut w = fresh();
    w.set_loc_text("JO50AA");
    assert!((w.draft.my_lat - 50.02).abs() < 0.05);
    w.set_position(48.0, 11.0);
    assert!(w.loc_text.starts_with("JN58"));
    // an invalid locator leaves the position alone
    w.set_loc_text("zz");
    assert_eq!(w.draft.my_lat, 48.0);
}

#[test]
fn finish_clears_first_run_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let rec = Rec::default();
    let mut st = app(Settings::default(), &rec, &path);
    assert!(st.settings.first_run);
    st.startup();
    let w = st.wizard.as_mut().expect("wizard shows on first run");
    w.draft.my_call = "DK0TEST".into();
    st.finish_wizard();
    assert!(st.wizard.is_none());
    assert!(!st.settings.first_run);
    assert_eq!(st.settings.my_call, "DK0TEST");
    let saved = Settings::load_or_default(&path).unwrap();
    assert!(!saved.first_run);
    assert_eq!(saved.my_call, "DK0TEST");
    assert_eq!(rec.0.borrow().bg_starts, 1);
}

#[test]
fn abort_writes_no_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let rec = Rec::default();
    let mut st = app(Settings::default(), &rec, &path);
    st.startup();
    assert!(st.wizard.is_some());
    st.on_exit();
    assert!(!path.exists());
    assert!(!dir.path().join("settings.toml.tmp").exists());
}

#[test]
fn no_services_before_finish() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let rec = Rec::default();
    let mut st = app(Settings::default(), &rec, &path);
    st.startup();
    st.sync_my();
    {
        let l = rec.0.borrow();
        assert_eq!(l.bg_starts, 0);
        assert_eq!(l.feed_starts, 0);
        assert_eq!(l.speclab, 0);
        assert_eq!(l.stations, 0);
        assert!(l.nfs.is_empty());
        assert!(l.my.is_empty());
    }
    st.finish_wizard();
    assert_eq!(rec.0.borrow().bg_starts, 1);
}

#[test]
fn services_get_nfs_and_my_before_play_after_finish() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Rec::default();
    let mut st = app(Settings::default(), &rec, &dir.path().join("s.toml"));
    st.startup();
    let w = st.wizard.as_mut().unwrap();
    w.draft.my_call = "DK0TEST".into();
    w.draft.path_near_field_suppression_m = 1234.0;
    st.finish_wizard();
    let l = rec.0.borrow();
    assert_eq!(l.nfs, vec![1234.0]);
    assert_eq!(l.my.len(), 1);
    assert_eq!(l.my[0].0, "DK0TEST");
}

#[test]
fn normal_start_syncs_services_without_wizard() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Rec::default();
    let s = Settings {
        first_run: false,
        path_near_field_suppression_m: 777.0,
        ..Settings::default()
    };
    let mut st = app(s, &rec, &dir.path().join("s.toml"));
    st.startup();
    assert!(st.wizard.is_none());
    let l = rec.0.borrow();
    assert_eq!(l.bg_starts, 1);
    assert_eq!(l.nfs, vec![777.0]);
    assert_eq!(l.my.len(), 1);
}

#[test]
fn broken_settings_file_shows_no_wizard() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Rec::default();
    let mut st = AppState::new(
        Settings::default(),
        Some("parse error".into()),
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    );
    st.settings_path = Some(dir.path().join("s.toml"));
    st.startup();
    assert!(st.wizard.is_none());
    assert_eq!(rec.0.borrow().bg_starts, 1);
}

#[test]
fn invalid_locator_blocks_next() {
    let mut w = fresh();
    w.terms_accepted = true;
    w.next().unwrap();
    w.next().unwrap();
    w.next().unwrap();
    assert_eq!(w.page, WizardPage::User);
    // a typo keeps the old (default) position: Next must refuse and say why
    w.set_loc_text("JO5XAA");
    let e = w.next().unwrap_err();
    assert!(e.contains("locator"), "{e}");
    assert_eq!(w.error.as_deref(), Some(e.as_str()));
    assert_eq!(w.page, WizardPage::User);
    w.set_loc_text("JN58TD");
    assert!(w.next().is_ok());
    assert_eq!(w.page, WizardPage::Feed);
}
