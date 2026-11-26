//! The planeserver window (egui_kittest): tabs, feed slots with disclaimer and password fields,
//! Apply, and the status tab. A fake service stands in for the real one: no network, no tray.
use airscout_planeserver::config::PlaneServerConfig;
use airscout_planeserver::gui::{PlaneServerApp, ServiceHandle, PORT_LABEL};
use airscout_planeserver::service::{FeedStatus, Status};
use airscout_planeserver::tray::{TrayCommand, TrayHandle};
use chrono::Utc;
use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers, ViewportCommand, ViewportEvent, ViewportId};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct FakeService {
    status: Status,
}

impl ServiceHandle for FakeService {
    fn status(&self) -> Status {
        self.status.clone()
    }
    fn stop(self: Box<Self>) {}
}

fn status(feeds: Vec<FeedStatus>) -> Status {
    Status {
        started: Utc::now(),
        planes: 7,
        feeds,
        server: "listening on 127.0.0.1:9872".into(),
        db_error: None,
    }
}

/// The app with a fake service that returns `st`; `calls` counts the services made.
fn app(dir: &Path, st: Status, calls: Arc<AtomicUsize>) -> (PlaneServerApp, PathBuf) {
    let path = dir.join("ps.toml");
    let cfg = PlaneServerConfig {
        data_dir: Some(dir.to_path_buf()),
        ..PlaneServerConfig::default()
    };
    let factory = Box::new(move |_: &PlaneServerConfig| -> Box<dyn ServiceHandle> {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::new(FakeService { status: st.clone() })
    });
    (PlaneServerApp::new(cfg, path.clone(), factory), path)
}

fn harness(app: PlaneServerApp) -> Harness<'static, PlaneServerApp> {
    Harness::builder()
        .with_size(eframe::egui::vec2(634.0, 452.0))
        .build_ui_state(
            |ui, app: &mut PlaneServerApp| {
                app.tick(ui.ctx());
                app.draw(ui);
            },
            app,
        )
}

fn default_harness(dir: &Path) -> Harness<'static, PlaneServerApp> {
    harness(app(dir, status(Vec::new()), Arc::default()).0)
}

fn has(h: &Harness<'static, PlaneServerApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

fn tab(h: &mut Harness<'static, PlaneServerApp>, name: &str) {
    h.get_by_role_and_label(Role::Button, name).click();
    h.run();
}

/// Picks feed `name` in the combo of slot 1.
fn pick(h: &mut Harness<'static, PlaneServerApp>, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Feed 1").click();
    h.run();
    h.get_by_role_and_label(Role::Button, name).click();
    h.run();
}

#[test]
fn tabs_render() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = default_harness(dir.path());
    h.run();
    // Each tab shows a label of its own; the label of the tab before it is gone, so a lost
    // click cannot pass.
    let tabs = [
        ("Status", "Uptime:"),
        ("Feeds", "Feed 1"),
        ("Area", "Covered Area"),
        ("Server", "Activate Server"),
        ("General", "Start in tray"),
        ("Status", "Uptime:"),
    ];
    assert!(has(&h, "Uptime:"));
    for w in tabs.windows(2) {
        let (prev, (name, label)) = (w[0].1, w[1]);
        tab(&mut h, name);
        assert!(has(&h, label), "tab {name} shows {label}");
        assert!(!has(&h, prev), "tab {name} no longer shows {prev}");
    }
}

#[test]
fn restricted_feed_shows_disclaimer() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = default_harness(dir.path());
    tab(&mut h, "Feeds");
    pick(&mut h, "ADSBSupport");
    h.get_by_label_contains("This feed needs credentials");
    // Cancel: the slot goes back to [none], nothing is accepted.
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run();
    assert!(h
        .query_by_label_contains("This feed needs credentials")
        .is_none());
    assert_eq!(h.state().draft().feeds[0], "[none]");
    assert!(h.state().draft().feeds_disclaimer_accepted.is_empty());
    // Accept: the slot keeps the feed and the name is remembered.
    pick(&mut h, "ADSBSupport");
    h.get_by_role_and_label(Role::Button, "Accept").click();
    h.run();
    assert!(h
        .query_by_label_contains("This feed needs credentials")
        .is_none());
    assert_eq!(h.state().draft().feeds[0], "ADSBSupport");
    assert_eq!(
        h.state().draft().feeds_disclaimer_accepted,
        vec!["ADSBSupport".to_owned()]
    );
    // Once accepted, picking it again asks no more.
    pick(&mut h, "[none]");
    pick(&mut h, "ADSBSupport");
    assert!(h
        .query_by_label_contains("This feed needs credentials")
        .is_none());
    assert_eq!(h.state().draft().feeds[0], "ADSBSupport");
}

#[test]
fn bincraft_shows_fallback_disclaimer() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = default_harness(dir.path());
    tab(&mut h, "Feeds");
    pick(&mut h, "BinCraft");
    h.get_by_label_contains("undocumented or restricted data source");
    h.get_by_role_and_label(Role::Button, "Accept").click();
    h.run();
    assert_eq!(h.state().draft().feeds[0], "BinCraft");
    assert_eq!(
        h.state().draft().feeds_disclaimer_accepted,
        vec!["BinCraft".to_owned()]
    );
}

#[test]
fn secret_field_is_password() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = default_harness(dir.path());
    tab(&mut h, "Feeds");
    pick(&mut h, "ADSBSupport");
    h.get_by_role_and_label(Role::Button, "Accept").click();
    h.run();
    h.get_by_role_and_label(Role::PasswordInput, "password");
    assert!(h
        .query_by_role_and_label(Role::TextInput, "password")
        .is_none());
    h.get_by_role_and_label(Role::TextInput, "url");
    // non-string values have their own widgets
    h.get_by_role_and_label(Role::SpinButton, "timeout_s");
    h.get_by_role_and_label(Role::CheckBox, "use_geo_alt");
}

#[test]
fn apply_saves_and_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (app, path) = app(dir.path(), status(Vec::new()), calls.clone());
    let mut h = harness(app);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tab(&mut h, "Server");
    h.get_by_role_and_label(Role::SpinButton, PORT_LABEL)
        .click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_all_by_role(Role::SpinButton)
        .find(|n| n.accesskit_node().is_focused())
        .expect("the port field has focus")
        .type_text("9999");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().draft().port, 9999);
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    let saved = PlaneServerConfig::load_or_default(&path).unwrap();
    assert_eq!(saved.port, 9999);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn revert_reloads_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app(dir.path(), status(Vec::new()), Arc::default());
    PlaneServerConfig {
        port: 9990,
        ..PlaneServerConfig::default()
    }
    .save(&path)
    .unwrap();
    let mut h = harness(app);
    h.state_mut().draft_mut().port = 1234;
    tab(&mut h, "Server");
    h.get_by_role_and_label(Role::Button, "Revert").click();
    h.run();
    assert_eq!(h.state().draft().port, 9990);
}

#[test]
fn status_shows_feed_error() {
    let dir = tempfile::tempdir().unwrap();
    let feed = FeedStatus {
        name: "OpenSky".into(),
        state: "running".into(),
        last_ok: None,
        last_error: Some("timeout".into()),
        last_error_at: Some(Utc::now()),
    };
    let (app, _) = app(dir.path(), status(vec![feed]), Arc::default());
    let mut h = harness(app);
    h.run();
    h.get_by_label("OpenSky");
    h.get_by_label("running");
    h.get_by_label("timeout");
    h.get_by_label_contains("listening on 127.0.0.1:9872");
    h.get_by_label("7");
}

#[test]
fn status_hides_error_older_than_last_ok() {
    let dir = tempfile::tempdir().unwrap();
    let feed = FeedStatus {
        name: "OpenSky".into(),
        state: "running".into(),
        // a cycle went well after the error
        last_ok: Some(Utc::now() - chrono::Duration::minutes(1)),
        last_error: Some("timeout".into()),
        last_error_at: Some(Utc::now() - chrono::Duration::minutes(2)),
    };
    let (app, _) = app(dir.path(), status(vec![feed]), Arc::default());
    let mut h = harness(app);
    h.run();
    h.get_by_label("OpenSky");
    assert!(h.query_by_label("timeout").is_none());
}

#[test]
fn status_shows_tray_error() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app(dir.path(), status(Vec::new()), Arc::default());
    app.set_tray_error("no display".into());
    let mut h = harness(app);
    h.run();
    h.get_by_label("tray: no display");
}

/// One feed row with an error at `err` and the last good cycle at `ok`.
fn feed_at(ok: chrono::DateTime<Utc>, err: chrono::DateTime<Utc>) -> FeedStatus {
    FeedStatus {
        name: "OpenSky".into(),
        state: "running".into(),
        last_ok: Some(ok),
        last_error: Some("timeout".into()),
        last_error_at: Some(err),
    }
}

#[test]
fn repeated_error_after_ok_is_shown() {
    let dir = tempfile::tempdir().unwrap();
    let t0 = Utc::now();
    // The same text as before, but it came after the last good cycle.
    let first = feed_at(
        t0 - chrono::Duration::seconds(30),
        t0 - chrono::Duration::seconds(60),
    );
    let st = status(vec![first]);
    let (app, _) = app(dir.path(), st, Arc::default());
    let mut h = harness(app);
    h.run();
    assert!(h.query_by_label("timeout").is_none());
    // Now the service reports the same text again, newer than last_ok.
    let again = status(vec![feed_at(t0 - chrono::Duration::seconds(30), t0)]);
    h.state_mut()
        .replace_service_for_test(Box::new(FakeService { status: again }));
    h.run();
    h.get_by_label("timeout");
}

#[test]
fn error_then_ok_inside_one_refresh_is_hidden() {
    let dir = tempfile::tempdir().unwrap();
    // Both happened before the window looked: the error 1 s ago, a good cycle 0.5 s ago.
    let t0 = Utc::now();
    let feed = feed_at(
        t0 - chrono::Duration::milliseconds(500),
        t0 - chrono::Duration::seconds(1),
    );
    let (app, _) = app(dir.path(), status(vec![feed]), Arc::default());
    let mut h = harness(app);
    h.run();
    h.get_by_label("OpenSky");
    assert!(h.query_by_label("timeout").is_none());
}

struct FakeTray;

impl TrayHandle for FakeTray {
    fn poll(&self) -> Vec<TrayCommand> {
        Vec::new()
    }
    fn set_planes(&mut self, _: usize) {}
}

/// Sends a close request to the window and returns whether the app cancelled it.
fn close_cancelled(wayland: bool) -> bool {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app(dir.path(), status(Vec::new()), Arc::default());
    app.set_tray(Box::new(FakeTray));
    app.set_wayland(wayland);
    let mut h = harness(app);
    h.run();
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.step();
    h.output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&ViewportCommand::CancelClose))
}

#[test]
fn close_hides_with_tray() {
    assert!(close_cancelled(false));
}

#[test]
fn close_quits_on_wayland() {
    assert!(!close_cancelled(true));
}

#[test]
fn general_tab_shows_wayland_note() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app(dir.path(), status(Vec::new()), Arc::default());
    app.set_wayland(true);
    let mut h = harness(app);
    tab(&mut h, "General");
    h.get_by_label_contains("Wayland");
}
