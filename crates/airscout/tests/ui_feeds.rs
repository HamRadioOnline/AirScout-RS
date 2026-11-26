//! Feed selection, feed settings and disclaimer in the Options dialog (egui_kittest).
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
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
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Planes;
    h.run();
    assert_eq!(
        h.state().options_dlg.as_ref().unwrap().tab,
        OptionsTab::Planes
    );
    h
}

fn draft<'a>(h: &'a mut Harness<'static, AppState>) -> &'a mut Settings {
    &mut h.state_mut().options_dlg.as_mut().unwrap().draft
}

/// A feed with a disclaimer (ADSBSupport) is picked in the combo.
fn pick(h: &mut Harness<'static, AppState>, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Feed").click();
    h.run();
    h.get_by_role_and_label(Role::Button, name).click();
    h.run();
}

/// The editor is drawn after Options, so its OK is the last one.
fn click_dialog_ok(h: &mut Harness<'static, AppState>) {
    h.get_all_by_role_and_label(Role::Button, "OK")
        .last()
        .unwrap()
        .click();
}

#[test]
fn secret_fields_are_masked() {
    let mut h = harness(Settings::default());
    h.get_by_label("Feed settings...").click();
    h.run();
    // OpenSky: client id and secret are masked, the URL is plain text
    h.get_by_role_and_label(Role::PasswordInput, "oauth_secret");
    h.get_by_role_and_label(Role::PasswordInput, "oauth_client_id");
    assert!(h
        .query_by_role_and_label(Role::TextInput, "oauth_secret")
        .is_none());
    h.get_by_role_and_label(Role::TextInput, "url");
    // the toggle reveals them
    h.get_by_label("Show credentials").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "oauth_secret");
}

#[test]
fn disclaimer_decline_reverts_selection() {
    let mut h = harness(Settings::default());
    pick(&mut h, "ADSBSupport");
    assert!(h
        .state()
        .options_dlg
        .as_ref()
        .unwrap()
        .disclaimer_dlg
        .is_some());
    assert_eq!(draft(&mut h).feed, "OpenSky");
    h.get_by_label("Decline").click();
    h.run();
    assert!(h
        .state()
        .options_dlg
        .as_ref()
        .unwrap()
        .disclaimer_dlg
        .is_none());
    assert_eq!(draft(&mut h).feed, "OpenSky");
    assert!(draft(&mut h).feeds_disclaimer_accepted.is_empty());
}

#[test]
fn disclaimer_accept_is_remembered() {
    let mut h = harness(Settings::default());
    pick(&mut h, "ADSBSupport");
    h.get_by_label("Accept").click();
    h.run();
    assert_eq!(draft(&mut h).feed, "ADSBSupport");
    assert_eq!(draft(&mut h).feeds_disclaimer_accepted, ["ADSBSupport"]);
    // switching away and back asks no more
    pick(&mut h, "OpenSky");
    assert_eq!(draft(&mut h).feed, "OpenSky");
    pick(&mut h, "ADSBSupport");
    assert!(h
        .state()
        .options_dlg
        .as_ref()
        .unwrap()
        .disclaimer_dlg
        .is_none());
    assert_eq!(draft(&mut h).feed, "ADSBSupport");
}

#[test]
fn invalid_feed_settings_not_stored() {
    let mut h = harness(Settings::default());
    h.get_by_label("Feed settings...").click();
    h.run();
    h.state_mut()
        .options_dlg
        .as_mut()
        .unwrap()
        .feed_dlg
        .as_mut()
        .unwrap()
        .table
        .insert("url".into(), 5.into());
    click_dialog_ok(&mut h);
    h.run();
    let o = h.state().options_dlg.as_ref().unwrap();
    // the editor stays open with the message, nothing is stored
    let dlg = o.feed_dlg.as_ref().expect("editor stays open");
    assert!(dlg.error.as_deref().unwrap().contains("url"));
    assert!(o.draft.feed_settings.get("OpenSky").is_none());
}

#[test]
fn valid_feed_settings_are_stored_and_default_restores() {
    let mut h = harness(Settings::default());
    h.get_by_label("Feed settings...").click();
    h.run();
    let dlg = || h.state().options_dlg.as_ref().unwrap();
    let default_url = dlg().feed_dlg.as_ref().unwrap().table["url"].clone();
    h.state_mut()
        .options_dlg
        .as_mut()
        .unwrap()
        .feed_dlg
        .as_mut()
        .unwrap()
        .table
        .insert("url".into(), "https://example.invalid/x".into());
    h.get_by_label("Default").click();
    h.run();
    assert_eq!(
        h.state()
            .options_dlg
            .as_ref()
            .unwrap()
            .feed_dlg
            .as_ref()
            .unwrap()
            .table["url"],
        default_url
    );
    click_dialog_ok(&mut h);
    h.run();
    let o = h.state().options_dlg.as_ref().unwrap();
    assert!(o.feed_dlg.is_none());
    assert_eq!(o.draft.feed_settings["OpenSky"]["url"], default_url);
}

#[test]
fn invalid_stored_settings_are_reported() {
    let mut s = Settings::default();
    let mut t = toml::Table::new();
    t.insert("url".into(), 5.into());
    t.insert("oauth_secret".into(), "s3cr3t".into());
    s.feed_settings.insert("OpenSky".into(), t.into());
    let mut h = harness(s);
    h.get_by_label("Feed settings...").click();
    h.run();
    let dlg = h
        .state()
        .options_dlg
        .as_ref()
        .unwrap()
        .feed_dlg
        .as_ref()
        .unwrap();
    let e = dlg.error.as_deref().expect("load error shown");
    assert!(e.contains("url") && !e.contains("s3cr3t"));
    h.get_by_label_contains("Stored settings are invalid");
}

#[test]
fn result_debug_hides_credentials() {
    use airscout::ui::dialogs::feed_settings::FeedSettingsResult;
    let mut t = toml::Table::new();
    t.insert("oauth_secret".into(), "s3cr3t".into());
    assert!(!format!("{:?}", FeedSettingsResult::Ok(t)).contains("s3cr3t"));
}
