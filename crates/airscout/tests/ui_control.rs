//! Control panel and status bar, driven through egui_kittest (no rendering).
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::{control, status};
use as_aircraft::PlaneInfoCache;
use as_geo::latlon;
use as_settings::Settings;
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::sync::Arc;

struct Null;

impl Services for Null {
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
}

fn state() -> AppState {
    AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn harness(st: AppState) -> Harness<'static, AppState> {
    Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            status::show(ui, st);
        },
        st,
    )
}

#[test]
fn play_button_disables_inputs() {
    let mut h = harness(state());
    h.run();
    let loc = h.get_by_role_and_label(Role::TextInput, "My loc");
    assert!(!loc.accesskit_node().is_disabled());

    h.get_by_label("Play").click();
    h.run();

    assert!(h.state().is_playing());
    assert!(h
        .get_by_role_and_label(Role::TextInput, "My loc")
        .accesskit_node()
        .is_disabled());
    assert!(h
        .get_by_role_and_label(Role::TextInput, "DX call")
        .accesskit_node()
        .is_disabled());

    h.get_by_label("Pause").click();
    h.run();
    assert!(!h.state().is_playing());
    assert!(!h
        .get_by_role_and_label(Role::TextInput, "My loc")
        .accesskit_node()
        .is_disabled());
}

#[test]
fn qtf_qrb_shown() {
    let mut st = state();
    st.set_my_loc_text("JO50AA");
    st.set_dx_loc_text("JO61UF");
    let qtf = latlon::bearing_loc("JO50AA", "JO61UF")
        .unwrap()
        .round_ties_even();
    let qrb = latlon::distance_loc("JO50AA", "JO61UF")
        .unwrap()
        .round_ties_even();
    let mut h = harness(st);
    h.run();
    h.get_by_label(&format!("QTF {qtf}°"));
    h.get_by_label(&format!("QRB {qrb} km"));
}

#[test]
fn invalid_locator_shows_zero_and_status_reason() {
    let mut st = state();
    st.set_my_loc_text("nonsense");
    let mut h = harness(st);
    h.run();
    h.get_by_label("QTF 0°");
    h.get_by_label("Play").click();
    h.run();
    assert!(!h.state().is_playing());
    h.get_by_label("invalid locator for My or DX");
}

#[test]
fn settings_error_is_shown_in_status_bar() {
    let st = AppState::new(
        Settings::default(),
        Some("cannot parse settings.toml".into()),
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    let mut h = harness(st);
    h.run();
    let _ = h.get_by_label_contains("cannot parse settings.toml");
}

#[test]
fn alarm_label_is_shown_in_status_bar() {
    let mut st = state();
    let mut h = harness(st_with_alarm(&mut st));
    h.run();
    h.get_by_label("ALARM: DLH123 13 km");
}

fn st_with_alarm(st: &mut AppState) -> AppState {
    let mut p = as_aircraft::PlaneInfo::new(
        chrono::Utc::now(),
        "DLH123",
        "",
        "abc123",
        51.3,
        1.6,
        135.0,
        30000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        as_aircraft::PlaneCategory::Medium,
    );
    p.int_qrb = 12.6;
    st.tick.alarm = Some(p);
    std::mem::replace(st, state())
}
