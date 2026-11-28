//! Options dialog, stage 4d: the HTTP server group and the port check.
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::{validate, OptionsTab};
use airscout::ui::{control, dialogs};
use airscout::web_service::WebState;
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::sync::Arc;

struct Fake(WebState);

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
    fn web_state(&self) -> WebState {
        self.0.clone()
    }
}

fn harness(settings: Settings, web: WebState) -> Harness<'static, AppState> {
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake(web)),
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
fn same_ports_block_ok() {
    let mut s = Settings::default();
    s.server.port = 9880;
    s.web.port = 9880;
    let want = "UDP Server and HTTP Server must have different port numbers.";
    assert_eq!(validate(&s), vec![want.to_string()]);
    s.web.port = 9881;
    assert!(validate(&s).is_empty());
    assert!(validate(&Settings::default()).is_empty());
}

#[test]
fn http_port_field_sets_setting() {
    let mut h = harness(Settings::default(), WebState::Off);
    tab(&mut h, OptionsTab::Network);
    h.get_by_label("AirScout HTTP Server Port:");
    // The UDP port field comes first in the tab; pick the HTTP one by its value.
    let http = h
        .get_all_by_role(Role::SpinButton)
        .find(|n| n.accesskit_node().numeric_value() == Some(9880.0))
        .expect("HTTP port field");
    http.click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_all_by_role(Role::SpinButton)
        .find(|n| n.accesskit_node().is_focused())
        .expect("the port field has focus")
        .type_text("9890");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().settings.web.port, 9890);
}

#[test]
fn lan_warning_shown() {
    let mut h = harness(Settings::default(), WebState::Off);
    tab(&mut h, OptionsTab::Network);
    h.get_by_label_contains("without authentication or encryption");
    h.get_by_label_contains("Open http://localhost:9880/ in a browser");
    h.get_by_label("HTTP Server Settings");
}

#[test]
fn web_state_text_shows() {
    for (state, text) in [
        (WebState::Off, "HTTP server: off"),
        (WebState::Starting, "HTTP server: starting..."),
        (
            WebState::Listening(9880),
            "HTTP server: listening on port 9880",
        ),
        (
            WebState::NotListening("address in use".into()),
            "HTTP server: not listening: address in use",
        ),
        // The server's own prefix is not repeated.
        (
            WebState::NotListening("Web server: cannot listen on 0.0.0.0:9880: in use".into()),
            "HTTP server: not listening: cannot listen on 0.0.0.0:9880: in use",
        ),
    ] {
        let mut h = harness(Settings::default(), state);
        tab(&mut h, OptionsTab::Network);
        h.get_by_label(text);
    }
}
