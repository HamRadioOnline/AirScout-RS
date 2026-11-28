//! Options dialog, stage 4c: serial and DDE rotator groups, OmniRig CAT entries.
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::{omnirig_rig_text, validate, OptionsTab};
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::{CatEngine, Settings, TrackSerial};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::sync::Arc;

struct Fake;

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
    fn restart_background(&mut self, _settings: &Settings, _generation: u64) {}
}

fn harness(settings: Settings) -> Harness<'static, AppState> {
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake),
        Arc::new(PlaneInfoCache::new()),
    );
    let mut h = Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    );
    h.get_by_label("Options").click();
    h.run();
    assert!(h.state().options_dlg.is_some());
    h
}

fn tab(h: &mut Harness<'static, AppState>, tab: OptionsTab) {
    h.state_mut().options_dlg.as_mut().unwrap().tab = tab;
    h.run();
}

#[test]
fn invalid_baudrate_blocks_ok() {
    let mut s = Settings::default();
    s.track.serial_baudrate = 1234;
    let want = "Track: This is not a valid baudrate: 1234\nValid baudrates are:\n\n50, 100, 110, 150, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200";
    assert!(validate(&s).contains(&want.to_string()));
    s.track.serial_baudrate = 4800;
    assert!(validate(&s).iter().all(|e| !e.starts_with("Track")));
}

#[test]
fn serial_group_sets_settings() {
    let mut s = Settings::default();
    s.track.activate = true;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Track);
    h.get_by_label("Rotator Control Serial Output");
    h.get_by_label("GS-232A Az/El").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().settings.track.serial, TrackSerial::Gs232AzEl);
}

#[test]
fn serial_group_disabled_without_tracking() {
    let mut s = Settings::default();
    s.track.activate = false;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Track);
    assert!(h
        .get_by_label("GS-232A Az/El")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("GS-232A Az/El").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().settings.track.serial, TrackSerial::None);
}

#[test]
fn dde_group_only_on_windows() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Track);
    assert_eq!(
        h.query_by_label("Ham Radio Deluxe (HRDRotator.exe)")
            .is_some(),
        cfg!(windows)
    );
}

#[test]
fn no_not_available_yet_line() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Track);
    assert!(h.query_by_label_contains("not available yet").is_none());
}

#[cfg(not(windows))]
#[test]
fn omnirig_engine_off_windows_shown_as_unavailable() {
    let mut s = Settings::default();
    s.cat.engine = CatEngine::OmniRig;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Cat);
    let combo = h.get_by_label("Select CAT-Engine & Rig-Type:");
    assert_eq!(
        combo.accesskit_node().value().as_deref(),
        Some("[OmniRig] not available on this system")
    );
    assert!(h.query_by_label("[OmniRig] Rig 1").is_none());
}

#[cfg(windows)]
#[test]
fn omnirig_rig_choice() {
    let mut s = Settings::default();
    s.cat.activate = true;
    s.cat.engine = CatEngine::OmniRig;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Cat);
    h.get_by_label("Rig:").click();
    h.run();
    h.get_by_label("[OmniRig] Rig 3").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().settings.cat.omnirig_rig, 3);
}

/// The baudrate field has the C# range 0 to 115200 (`OptionsDlg.Designer.cs:7680-7681`): a
/// hand-edited value above it is shown and kept as 115200, a valid baudrate.
#[test]
fn baudrate_field_has_csharp_range() {
    let mut s = Settings::default();
    s.track.activate = true;
    s.track.serial_baudrate = 230_400;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Track);
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(h.state().settings.track.serial_baudrate, 115_200);
}

/// The OmniRig choice shows the rig the CAT worker uses (`cat_config` clamps to 1..=4).
#[test]
fn omnirig_rig_text_shows_rig_in_use() {
    assert_eq!(omnirig_rig_text(2), "[OmniRig] Rig 2");
    assert_eq!(omnirig_rig_text(9), "[OmniRig] Rig 4");
    assert_eq!(omnirig_rig_text(0), "[OmniRig] Rig 1");
}
