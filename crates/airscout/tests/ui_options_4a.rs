//! Options dialog, stage 4a: Network tab, full Track tab, KST sync checkbox.
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::options::{network, validate, OptionsTab};
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::{Settings, TrackUdp};
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
fn track_ports_equal_blocks_ok() {
    let mut s = Settings::default();
    s.track.udp_wintest_port = 9872;
    assert_eq!(
        validate(&s),
        vec!["Track: Win-Test port and AirScout port must have different values.".to_string()]
    );
    assert!(validate(&Settings::default()).is_empty());
}

#[test]
fn port_warning_when_server_and_airscout_share_9872() {
    let mut s = Settings::default();
    s.server.activate = true;
    s.track.activate = true;
    s.track.udp = TrackUdp::AirScout;
    assert_eq!(
        network::port_warnings(&s),
        vec![
            "Network: port 9872 is used by the UDP server and by the tracking output (AirScout)."
                .to_string()
        ]
    );
    s.track.udp = TrackUdp::WinTest;
    assert!(network::port_warnings(&s).is_empty());
    s.track.udp = TrackUdp::AirScout;
    s.server.activate = false;
    assert!(network::port_warnings(&s).is_empty());
}

#[test]
fn ok_allowed_with_warning() {
    let mut s = Settings::default();
    s.server.activate = true;
    s.track.activate = true;
    s.track.udp = TrackUdp::AirScout;
    let mut h = harness(s);
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
}

#[test]
fn warning_is_shown() {
    let mut s = Settings::default();
    s.server.activate = true;
    s.track.activate = true;
    s.track.udp = TrackUdp::AirScout;
    let h = harness(s);
    h.get_by_label_contains("is used by the UDP server");
}

#[test]
fn network_tab_toggles_server() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Network);
    h.get_by_label("Activate Network Server").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().settings.server.activate);
}

#[test]
fn network_tab_before_track() {
    let h = harness(Settings::default());
    h.get_by_role_and_label(Role::Button, "Network");
}

#[test]
fn track_output_groups_disabled_until_activate() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Track);
    assert!(h
        .get_by_label("Native Az/El")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("Activate Tracking").click();
    h.run();
    assert!(!h
        .get_by_label("Native Az/El")
        .accesskit_node()
        .is_disabled());
    assert!(draft(&mut h).track.activate);
}

#[test]
fn watchlist_sync_checkbox_disabled_without_server() {
    let mut h = harness(Settings::default());
    tab(&mut h, OptionsTab::Watchlist);
    let l = "Keep in sync with KST user list (needs wtKST > V3.1 and network functions activated):";
    assert!(h.get_by_label(l).accesskit_node().is_disabled());
    draft(&mut h).server.activate = true;
    h.run();
    assert!(!h.get_by_label(l).accesskit_node().is_disabled());
}

#[test]
fn clear_watchlist_disabled_while_synced() {
    let mut s = Settings::default();
    s.server.activate = true;
    s.watchlist_sync_with_kst = true;
    let mut h = harness(s);
    tab(&mut h, OptionsTab::Watchlist);
    assert!(h
        .get_by_label("Clear watchlist")
        .accesskit_node()
        .is_disabled());
    draft(&mut h).watchlist_sync_with_kst = false;
    h.run();
    assert!(!h
        .get_by_label("Clear watchlist")
        .accesskit_node()
        .is_disabled());
}
