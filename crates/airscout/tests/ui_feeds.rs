//! Feed selection, feed settings and disclaimer in the Options dialog (egui_kittest).
use airscout::app::{AppState, Services};
use airscout::feed_files::import_feed;
use airscout::services::PathRequest;
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use eframe::egui::{accesskit::Role, Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::path::PathBuf;
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
    harness_in(settings, None)
}

fn harness_in(settings: Settings, data_dir: Option<PathBuf>) -> Harness<'static, AppState> {
    let mut st = AppState::new(
        settings,
        None,
        Box::new(Fake),
        Arc::new(PlaneInfoCache::new()),
    );
    st.data_dir = data_dir;
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(900.0, 600.0))
        .build_ui_state(
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

/// A feed with a disclaimer (ADSBSupport) is picked in the slot 1 combo.
fn pick(h: &mut Harness<'static, AppState>, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Plane Feed 1:")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, name).click();
    h.run();
}

/// Pick `name` (a feed, or `[none]`) in the combo of slot 2 or 3. The dropdown scrolls, so the
/// item is scrolled into view before it is clicked (a click off the visible area hits nothing).
fn pick_slot(h: &mut Harness<'static, AppState>, slot: usize, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, &format!("Plane Feed {slot}:"))
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, name).scroll_to_me();
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
    feed_button(&mut h, "Settings", 1);
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
    feed_button(&mut h, "Settings", 1);
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
    feed_button(&mut h, "Settings", 1);
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
    h.get_all_by_role_and_label(Role::Button, "Default")
        .last()
        .unwrap()
        .click();
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
    feed_button(&mut h, "Settings", 1);
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

#[test]
fn feed2_selection_sets_the_second_slot() {
    let mut h = harness(Settings::default());
    pick_slot(&mut h, 2, "Readsb");
    assert_eq!(draft(&mut h).feed, "OpenSky");
    assert_eq!(draft(&mut h).feed2, "Readsb");
    assert_eq!(draft(&mut h).feed3, "");
}

#[test]
fn feed3_none_clears_the_third_slot() {
    let s = Settings {
        feed3: "Readsb".into(),
        ..Settings::default()
    };
    let mut h = harness(s);
    pick_slot(&mut h, 3, "[none]");
    assert_eq!(draft(&mut h).feed3, "");
    assert_eq!(draft(&mut h).feed, "OpenSky");
}

#[test]
fn feed2_disclaimer_decline_keeps_the_slot_empty() {
    let mut h = harness(Settings::default());
    pick_slot(&mut h, 2, "ADSBSupport");
    assert!(h
        .state()
        .options_dlg
        .as_ref()
        .unwrap()
        .disclaimer_dlg
        .is_some());
    assert_eq!(draft(&mut h).feed2, "");
    h.get_by_label("Decline").click();
    h.run();
    assert_eq!(draft(&mut h).feed2, "");
    assert!(draft(&mut h).feeds_disclaimer_accepted.is_empty());
}

#[test]
fn feed3_disclaimer_accept_sets_the_third_slot() {
    let mut h = harness(Settings::default());
    pick_slot(&mut h, 3, "ADSBSupport");
    h.get_by_label("Accept").click();
    h.run();
    assert_eq!(draft(&mut h).feed, "OpenSky");
    assert_eq!(draft(&mut h).feed3, "ADSBSupport");
    assert_eq!(draft(&mut h).feeds_disclaimer_accepted, ["ADSBSupport"]);
}

#[test]
fn finding_340_editor_stores_the_canonical_endpoint() {
    use airscout::ui::dialogs::feed_settings::FeedSettingsDlg;
    let mut dlg = FeedSettingsDlg::new("OpenSky", None).unwrap();
    dlg.table.insert(
        "url".into(),
        "https://api.opensky-network.org/api/%2F/states?lat=%MINLAT%".into(),
    );
    let stored = dlg.try_ok().unwrap();
    assert_eq!(
        stored["url"].as_str(),
        Some("https://opensky-network.org/api/%2F/states?lat=%MINLAT%")
    );
}

#[test]
fn finding_329_editor_restores_the_desktop_default() {
    use airscout::ui::dialogs::feed_settings::FeedSettingsDlg;
    let mut dlg = FeedSettingsDlg::new("AirScoutServer", None).unwrap();
    dlg.table.insert("url".into(), "".into());
    dlg.restore_defaults();
    assert_eq!(
        dlg.table["url"].as_str(),
        Some("http://localhost:9872/planes.json")
    );
}

fn feed_button(h: &mut Harness<'static, AppState>, label: &str, slot: usize) {
    h.get_all_by_role_and_label(Role::Button, label)
        .nth(slot - 1)
        .unwrap()
        .click();
}

fn submit_feed_file(h: &mut Harness<'static, AppState>, label: &str) {
    h.get_all_by_role_and_label(Role::Button, label)
        .last()
        .unwrap()
        .click();
    h.run();
}

fn import_from_tmp(h: &mut Harness<'static, AppState>, filename: &str) {
    feed_button(h, "Import", 1);
    h.run();
    h.get_by_label(filename).click();
    h.run();
    submit_feed_file(h, "Import");
}

fn custom_feed() -> (Settings, toml::Table) {
    let mut settings = Settings::default();
    let mut table = airscout::ui::dialogs::feed_settings::FeedSettingsDlg::new("OpenSky", None)
        .unwrap()
        .table;
    table.insert("url".into(), "http://localhost:9070/states".into());
    table.insert("oauth_secret".into(), "synthetic-current-secret".into());
    settings
        .feed_settings
        .insert("OpenSky".into(), table.clone().into());
    (settings, table)
}

#[test]
fn feed_export_import_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, original) = custom_feed();
    let mut h = harness_in(settings, Some(dir.path().into()));

    feed_button(&mut h, "Export", 1);
    h.run();
    submit_feed_file(&mut h, "Export");
    let path = dir.path().join("Tmp/OpenSky.feed");
    assert!(path.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }

    pick_slot(&mut h, 2, "OpenSky");
    assert_eq!(draft(&mut h).feed2, "OpenSky");
    draft(&mut h).feed_settings.remove("OpenSky");
    feed_button(&mut h, "Import", 2);
    h.run();
    h.get_by_label("OpenSky.feed").click();
    h.run();
    submit_feed_file(&mut h, "Import");
    assert!(draft(&mut h).feed_settings["OpenSky"].as_table() == Some(&original));
    assert_eq!(
        draft(&mut h).feed_settings["OpenSky"]["oauth_secret"].as_str(),
        Some("synthetic-current-secret")
    );
}

#[test]
fn feed_import_error_keeps_current_feed() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    std::fs::create_dir(&tmp).unwrap();
    let (settings, original) = custom_feed();
    let mut h = harness_in(settings, Some(dir.path().into()));
    for (filename, contents) in [
        (
            "malformed.feed",
            "[OpenSky]\noauth_secret = \"synthetic-import-secret\" extra",
        ),
        (
            "invalid.feed",
            "[OpenSky]\noauth_secret = \"synthetic-import-secret\"\nurl = 5",
        ),
        ("empty.feed", "[OpenSky]\n"),
    ] {
        std::fs::write(tmp.join(filename), contents).unwrap();
        import_from_tmp(&mut h, filename);
        let (summary, hover) = h.state().options_dlg.as_ref().unwrap().status_texts();
        assert!(summary.contains("Cannot import"));
        h.get_by_label(&summary);
        for text in [&summary, &hover] {
            assert!(!text.contains("synthetic-import-secret"));
            assert!(!text.contains("synthetic-current-secret"));
        }
        assert!(draft(&mut h).feed_settings["OpenSky"].as_table() == Some(&original));
    }
}

fn check_incompatible_import(contents: &str) {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    std::fs::create_dir(&tmp).unwrap();
    std::fs::write(tmp.join("incompatible.feed"), contents).unwrap();
    let (settings, original) = custom_feed();
    let mut h = harness_in(settings, Some(dir.path().into()));

    import_from_tmp(&mut h, "incompatible.feed");

    let dlg = h.state().options_dlg.as_ref().unwrap();
    let (summary, hover) = dlg.status_texts();
    let expected = "Cannot import plane feed settings: invalid settings for the selected feed.";
    assert!(summary.contains(expected) && hover.contains(expected));
    for text in [&summary, &hover] {
        assert!(!text.contains("synthetic-import-secret"));
        assert!(!text.contains("synthetic-current-secret"));
    }
    h.get_by_label(&summary);
    assert!(dlg.draft.feed_settings["OpenSky"].as_table() == Some(&original));
}

#[test]
fn wrong_feed_import_keeps_current_feed() {
    check_incompatible_import("[Readsb]\nurl = \"http://localhost:8080/data/aircraft.json\"");
}

#[test]
fn unknown_feed_keys_keep_current_feed() {
    check_incompatible_import("[OpenSky]\nunknown = \"synthetic-import-secret\"");
    check_incompatible_import(
        "[OpenSky]\nunknown = \"synthetic-import-secret\"\nurl = \"http://localhost:8080/states\"",
    );
}

#[test]
fn feed_import_canonicalizes_opensky_url() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    std::fs::create_dir(&tmp).unwrap();
    std::fs::write(
        tmp.join("legacy.feed"),
        "[OpenSky]\nurl = \"https://api.opensky-network.org/api/%2F/states?lat=%MINLAT%\"",
    )
    .unwrap();
    let mut h = harness_in(Settings::default(), Some(dir.path().into()));

    import_from_tmp(&mut h, "legacy.feed");
    click_dialog_ok(&mut h);
    h.run();

    assert!(h.state().options_dlg.is_none());
    assert_eq!(
        h.state().settings.feed_settings["OpenSky"]["url"].as_str(),
        Some("https://opensky-network.org/api/%2F/states?lat=%MINLAT%")
    );
}

#[test]
fn feed_file_status_clears_after_leaving_planes_or_changing_feed() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("Tmp");
    std::fs::create_dir(&tmp).unwrap();
    std::fs::write(tmp.join("invalid.feed"), "[OpenSky]\n").unwrap();
    for action in [
        "tab click",
        "try_select_tab",
        "Default",
        "Settings",
        "Select",
    ] {
        let mut h = harness_in(Settings::default(), Some(dir.path().into()));
        import_from_tmp(&mut h, "invalid.feed");
        let status = |h: &Harness<'static, AppState>| {
            h.state().options_dlg.as_ref().unwrap().status_texts().0
        };
        assert!(status(&h).contains("Cannot import"));

        match action {
            "tab click" => {
                h.get_by_label("General").click();
                h.run();
                assert_eq!(
                    h.state().options_dlg.as_ref().unwrap().tab,
                    OptionsTab::General
                );
            }
            "try_select_tab" => {
                assert!(h
                    .state_mut()
                    .options_dlg
                    .as_mut()
                    .unwrap()
                    .try_select_tab(OptionsTab::General));
            }
            "Select" => {
                pick(&mut h, "Readsb");
                assert_eq!(draft(&mut h).feed, "Readsb");
            }
            _ => {
                feed_button(&mut h, action, 1);
                h.run();
            }
        }
        assert!(!status(&h).contains("Cannot import"), "{action}");
    }
}

#[test]
fn feed_buttons_enabled_per_feed() {
    let mut h = harness(Settings::default());
    let check = |h: &Harness<'static, AppState>, disabled: [bool; 3]| {
        for caption in ["Import", "Export", "Default"] {
            let actual: Vec<_> = h
                .get_all_by_role_and_label(Role::Button, caption)
                .map(|n| n.accesskit_node().is_disabled())
                .collect();
            assert_eq!(actual, disabled, "{caption}");
        }
    };
    check(&h, [false, true, true]);
    pick_slot(&mut h, 2, "Readsb");
    check(&h, [false, false, true]);
    pick_slot(&mut h, 2, "[none]");
    check(&h, [false, true, true]);
}

#[test]
fn feed_default_restores_defaults() {
    let (settings, _) = custom_feed();
    let mut h = harness(settings);
    feed_button(&mut h, "Default", 1);
    h.run();
    let default = airscout::ui::dialogs::feed_settings::FeedSettingsDlg::new("OpenSky", None)
        .unwrap()
        .table;
    assert!(draft(&mut h).feed_settings["OpenSky"].as_table() == Some(&default));
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(
        h.state().settings.feed_settings["OpenSky"]["oauth_secret"].as_str(),
        Some("synthetic-current-secret")
    );
}

#[test]
fn feed_export_uses_defaults_at_typed_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("localhost.feed");
    std::fs::write(&path, "previous file").unwrap();
    let mut h = harness_in(Settings::default(), Some(dir.path().into()));

    feed_button(&mut h, "Export", 1);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "File path")
        .click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "File path")
        .type_text(&format!(" {} ", path.display()));
    h.run();
    submit_feed_file(&mut h, "Export");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous file");
    h.get_by_label("Replace the existing feed file?");
    submit_feed_file(&mut h, "Replace");

    let (name, table) = import_feed(&path).unwrap();
    assert_eq!(name, "OpenSky");
    let default = airscout::ui::dialogs::feed_settings::FeedSettingsDlg::new("OpenSky", None)
        .unwrap()
        .table;
    assert!(table == default);
    assert!(draft(&mut h).feed_settings.get("OpenSky").is_none());
}

#[test]
fn settings_defaults() {
    for settings in [Settings::default(), toml::from_str::<Settings>("").unwrap()] {
        assert!(!settings.planes_log_errors);
        assert!(!settings.planes_trace_positions);
        assert_eq!(settings.planes_interval_s, 60);
    }
}

#[test]
fn general_feed_controls_apply() {
    let mut h = harness(Settings::default());
    for caption in [
        "Trace aircraft positions to files (Caution: this will cause a lot of files in <AirScout's Temp directory>/PlanePositions!)",
        "Enable extended plausibility check of aircraft info",
        "Log errors as warnings to AirScout's main logfile",
    ] {
        h.get_by_role_and_label(Role::CheckBox, caption).click();
        h.run();
    }

    let distance = "Max. allowed distance between \nreported and estimated position [km]:";
    for (label, value) in [("Interval [sec]:", 5.0), (distance, 601.0)] {
        let field = h.get_by_role_and_label(Role::SpinButton, label);
        let label_rect = h.get_by_role_and_label(Role::Label, label).rect();
        assert!(label_rect.right() <= field.rect().left(), "{label}");
        assert_eq!(field.accesskit_node().min_numeric_value(), Some(10.0));
        assert_eq!(field.accesskit_node().max_numeric_value(), Some(600.0));
        field.set_value_accesskit(value);
        h.run();
    }

    let dlg = draft(&mut h);
    assert!(dlg.planes_trace_positions);
    assert!(dlg.planes_log_errors);
    assert!(!dlg.planes_extended_plausibility);
    assert_eq!(dlg.planes_interval_s, 10);
    assert_eq!(dlg.planes_max_error_dist, 600.0);

    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    let settings = &h.state().settings;
    assert!(settings.planes_trace_positions);
    assert!(settings.planes_log_errors);
    assert!(!settings.planes_extended_plausibility);
    assert_eq!(settings.planes_interval_s, 10);
    assert_eq!(settings.planes_max_error_dist, 600.0);
}

#[test]
fn live_filters_share_main_window_settings() {
    let mut h = harness(Settings::default());
    for (label, value) in [
        ("Minimal Altitude \n(Filter OFF = 0:", 1250.0),
        (
            "Maximal Path Midpoint Circumcircle\n(Filter AUTO = 0)\n(Filter OFF = -1):",
            -1.0,
        ),
    ] {
        let field = h.get_by_role_and_label(Role::SpinButton, label);
        field.scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::SpinButton, label)
            .set_value_accesskit(value);
        h.run();
    }
    assert_eq!(draft(&mut h).gui.planes_filter_min_alt_m, 1250);
    assert_eq!(draft(&mut h).gui.planes_filter_max_circumcircle_km, -1.0);
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    assert_eq!(h.state().settings.gui.planes_filter_min_alt_m, 1250);
    assert_eq!(
        h.state().settings.gui.planes_filter_max_circumcircle_km,
        -1.0
    );
}

#[test]
fn lifetime_default_matches_csharp() {
    for settings in [Settings::default(), toml::from_str::<Settings>("").unwrap()] {
        let serialized = toml::Value::try_from(&settings).unwrap();
        assert_eq!(
            serialized
                .get("aircraft_database_max_days_lifetime")
                .and_then(toml::Value::as_integer),
            Some(7)
        );
    }
    let mut h = harness(Settings::default());
    let label = "Keep aircraft positions for:";
    let field = h.get_by_role_and_label(Role::SpinButton, label);
    assert_eq!(field.accesskit_node().min_numeric_value(), Some(1.0));
    assert_eq!(field.accesskit_node().max_numeric_value(), Some(365.0));
    field.scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::SpinButton, label)
        .set_value_accesskit(30.0);
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Keep Plane Position History")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Keep Plane Position History")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    assert!(h.state().settings.planes_keep_history);
    let serialized = toml::Value::try_from(&h.state().settings).unwrap();
    assert_eq!(
        serialized["aircraft_database_max_days_lifetime"].as_integer(),
        Some(30)
    );
}

#[test]
fn lifetime_zero_skips_age_delete() {
    use as_aircraft::positions::{maintain, AircraftPosition, MaintainerConfig, PositionDb};
    use chrono::{Duration, Utc};

    let db = PositionDb::open_in_memory().unwrap();
    db.insert_or_update_if_newer(&AircraftPosition {
        hex: "000001".into(),
        call: "TE1ST".into(),
        lat: 50.0,
        lon: 10.0,
        alt_ft: 30000.0,
        track: 90.0,
        speed_kts: 300.0,
        last_updated: Utc::now() - Duration::days(30),
    })
    .unwrap();
    let cfg = MaintainerConfig {
        max_days: 0,
        ..MaintainerConfig::default()
    };
    let mut last_max_size = 0.0;
    let stats = maintain(&db, &cfg, &mut last_max_size).unwrap();
    assert_eq!(stats.deleted_old, 0);
    assert_eq!(db.count().unwrap(), 1);
}
