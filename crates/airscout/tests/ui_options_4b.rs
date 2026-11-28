//! Options dialog, stage 4b: CAT tab and the Doppler group of the Track tab.
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::{validate, OptionsTab};
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::{CatEngine, Settings};
use eframe::egui::accesskit::Role;
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

fn draft<'a>(h: &'a mut Harness<'static, AppState>) -> &'a mut Settings {
    &mut h.state_mut().options_dlg.as_mut().unwrap().draft
}

fn tab(h: &mut Harness<'static, AppState>, tab: OptionsTab) {
    h.state_mut().options_dlg.as_mut().unwrap().tab = tab;
    h.run();
}

#[test]
fn rigctld_empty_host_blocks_ok() {
    let mut s = Settings::default();
    s.cat.activate = true;
    s.cat.engine = CatEngine::Rigctld;
    s.cat.rigctld_host = "  ".into();
    assert_eq!(
        validate(&s),
        vec!["CAT: the rigctld host must not be empty.".to_string()]
    );
}

#[test]
fn rigctld_port_zero_blocks_ok() {
    let mut s = Settings::default();
    s.cat.activate = true;
    s.cat.engine = CatEngine::Rigctld;
    s.cat.rigctld_port = 0;
    assert_eq!(
        validate(&s),
        vec!["CAT: the rigctld port must be between 1 and 65535.".to_string()]
    );
}

#[test]
fn cat_off_skips_rigctld_checks() {
    let mut s = Settings::default();
    s.cat.engine = CatEngine::Rigctld;
    s.cat.rigctld_port = 0;
    s.cat.rigctld_host.clear();
    assert!(validate(&s).iter().all(|e| !e.starts_with("CAT")));
}

#[test]
fn cat_tab_activate_and_engine() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Cat);
    h.get_by_role_and_label(Role::CheckBox, "Activate CAT Interface")
        .click();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Select CAT-Engine & Rig-Type:")
        .click();
    h.run();
    h.get_by_label("[rigctld] Hamlib rigctld (TCP)").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let c = &h.state().settings.cat;
    assert!(c.activate);
    assert_eq!(c.engine, CatEngine::Rigctld);
}

#[test]
fn rigctld_fields_only_for_rigctld() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Cat);
    assert!(h.query_by_label("Host:").is_none());
    draft(&mut h).cat.engine = CatEngine::Rigctld;
    h.run();
    h.get_by_label("Host:");
    h.get_by_label("Port:");
}

#[cfg(not(windows))]
#[test]
fn no_omnirig_or_serial_entries() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Cat);
    h.get_by_role_and_label(Role::ComboBox, "Select CAT-Engine & Rig-Type:")
        .click();
    h.run();
    assert!(h.query_by_label_contains("[OmniRig]").is_none());
    assert!(h.query_by_label_contains("[Serial]").is_none());
    #[cfg(not(feature = "hamlib"))]
    {
        assert!(h.query_by_label_contains("PortName:").is_none());
        assert!(h
            .query_by_label_contains("[Hamlib] native library")
            .is_none());
    }
}

#[cfg(not(feature = "hamlib"))]
#[test]
fn hamlib_engine_in_default_build_shown_as_unavailable() {
    let mut s = Settings::default();
    s.cat.engine = CatEngine::Hamlib;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Cat);
    let combo = h.get_by_label("Select CAT-Engine & Rig-Type:");
    assert_eq!(
        combo.accesskit_node().value().as_deref(),
        Some("[Hamlib] not available in this build")
    );
}

#[test]
fn doppler_group_needs_cat_and_track() {
    let mut s = Settings::default();
    s.track.activate = true;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Track);
    assert!(h
        .get_by_label("Doppler strategy")
        .accesskit_node()
        .is_disabled());
    draft(&mut h).cat.activate = true;
    h.run();
    assert!(!h
        .get_by_label("Doppler strategy")
        .accesskit_node()
        .is_disabled());
}
