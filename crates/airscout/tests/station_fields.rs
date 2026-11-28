//! MapDlg edits and selections for the four station fields.
use airscout::app::{AppState, RealServices, Services};
use airscout::services::{open_data, PathRequest};
use airscout::ui::control;
use as_aircraft::PlaneInfoCache;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{gpoint_from_loc, PositionInRectangle};
use as_settings::{Settings, WatchlistItem};
use as_stations::{GeoSource, Location};
use chrono::{DateTime, Duration};
use eframe::egui::{accesskit::Role, Color32, Key, Shape};
use egui_kittest::{kittest::Queryable, Harness};
use std::sync::Arc;

struct Stations;

fn point(loc: &str) -> GPoint {
    gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle).unwrap()
}

impl Services for Stations {
    fn request_path(&mut self, _: PathRequest) {}

    fn start_feed(&mut self, _: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        Ok(())
    }

    fn stop_feed(&mut self) {}

    fn location_find(&self, call: &str) -> Option<GPoint> {
        assert!(
            as_stations::callsign::check(call),
            "looked up an invalid call"
        );
        self.location_find_all(call).into_iter().next()
    }

    fn location_find_all(&self, call: &str) -> Vec<GPoint> {
        match call {
            "TE1ST" => vec![point("JO50AA")],
            "TE2ST" => vec![point("JO61AA"), point("JO60AA")],
            _ => Vec::new(),
        }
    }
}

fn state() -> AppState {
    AppState::new(
        Settings::default(),
        None,
        Box::new(Stations),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn my(st: &AppState) -> GPoint {
    GPoint::new(st.settings.my_lat, st.settings.my_lon)
}

fn dx(st: &AppState) -> GPoint {
    GPoint::new(st.settings.dx_lat, st.settings.dx_lon)
}

#[test]
fn call_text_hit_sets_position() {
    let mut st = state();
    let height = st.settings.my_height_m;
    st.my_call_changed("TE1ST");
    st.dx_call_changed("TE2ST");
    assert_eq!(st.settings.my_call, "TE1ST");
    assert_eq!(st.settings.dx_call, "TE2ST");
    assert_eq!(my(&st), point("JO50AA"));
    assert_eq!(dx(&st), point("JO61AA"));
    assert_eq!(st.settings.my_height_m, height);
    assert_eq!(st.my_loc_text, "JO50AA");
    assert_eq!(st.dx_loc_text, "JO61AA");
}

#[test]
fn call_text_miss_clears_position_and_locator() {
    let mut st = state();
    st.my_call_changed("TE1ST");
    st.dx_call_changed("TE2ST");
    st.my_call_changed("TE3ST");
    st.dx_call_changed("TE3ST");
    for p in [my(&st), dx(&st)] {
        assert!(p.lat.is_nan() && p.lon.is_nan());
    }
    assert!(st.my_loc_text.is_empty());
    assert!(st.dx_loc_text.is_empty());
    assert_eq!(st.settings.my_call, "TE3ST");
    assert_eq!(st.settings.dx_call, "TE3ST");
    assert_eq!(st.qtf_qrb(), (0, 0));
}

#[test]
fn invalid_call_never_looks_up() {
    let mut st = state();
    st.my_call_changed("TE");
    st.dx_call_changed("TE");
    assert!(my(&st).lat.is_nan());
    assert!(dx(&st).lat.is_nan());
}

#[test]
fn call_selected_sets_locator_and_height_10() {
    let mut st = state();
    st.settings.gui.locator_small_letters_for_subsquares = true;
    st.my_call_selected("TE1ST");
    st.dx_call_selected("TE2ST");
    assert_eq!(st.my_loc_text, "JO50aa");
    assert_eq!(st.dx_loc_text, "JO61aa");
    assert_eq!(my(&st), point("JO50AA"));
    assert_eq!(dx(&st), point("JO61AA"));
    assert_eq!(st.settings.my_height_m, 10.0);
    assert_eq!(st.settings.dx_height_m, 10.0);

    st.my_call_selected("TE3ST");
    st.dx_call_selected("TE3ST");
    assert!(my(&st).lat.is_nan());
    assert!(dx(&st).lat.is_nan());
    assert_eq!(st.settings.my_height_m, 0.0);
    assert_eq!(st.settings.dx_height_m, 0.0);
}

#[test]
fn dx_items_are_sorted_watchlist_calls() {
    let mut st = state();
    st.settings.gui.my_calls = vec!["TE2ST".into(), "TE1ST".into()];
    st.settings.watchlist.0 = [
        ("TE2ST", "JO61AA"),
        ("TE1ST", "JO50AA"),
        ("TE2ST", "JO60AA"),
    ]
    .into_iter()
    .map(|(call, loc)| WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        ..Default::default()
    })
    .collect();
    assert_eq!(st.my_call_items(), ["TE2ST", "TE1ST"]);
    assert_eq!(st.dx_call_items(), ["TE1ST", "TE2ST", "TE2ST"]);
}

#[test]
fn loc_items_list_all_locations_most_recent_first() {
    let mut st = state();
    st.my_call_changed("TE1ST");
    st.dx_call_changed("TE2ST");
    st.settings.gui.locator_small_letters_for_subsquares = true;
    assert_eq!(st.my_loc_items(), [("JO50aa".into(), point("JO50AA"))]);
    assert_eq!(
        st.dx_loc_items(),
        [
            ("JO61aa".into(), point("JO61AA")),
            ("JO60aa".into(), point("JO60AA")),
        ]
    );
}

#[test]
fn real_services_find_station_locations_most_recent_first() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let older = point("JO50AA");
    let newer = point("JO60AA");
    let old_time = DateTime::UNIX_EPOCH + Duration::days(1);
    let new_time = old_time + Duration::seconds(1);
    for (loc, p, timestamp) in [("JO60AA", newer, new_time), ("JO50AA", older, old_time)] {
        let location = Location::new(
            "TE1ST",
            loc,
            p.lat,
            p.lon,
            GeoSource::FromUser,
            0,
            timestamp,
        );
        paths
            .stations
            .location_insert_or_update_if_newer(&location)
            .unwrap();
    }

    let (tx, _rx) = std::sync::mpsc::channel();
    let services = RealServices::new(paths, tx);
    assert_eq!(services.location_find("TE1ST"), Some(newer));
    assert_eq!(services.location_find_all("TE1ST"), [newer, older]);
    assert_eq!(services.location_find("TE2ST"), None);
    assert!(services.location_find_all("TE2ST").is_empty());
}

#[test]
fn loc_selected_sets_position() {
    let mut st = state();
    st.my_call_changed("TE1ST");
    st.dx_call_changed("TE2ST");
    let (loc, mut p) = st.dx_loc_items().pop().unwrap();
    st.dx_loc_selected(p);
    assert_eq!(dx(&st), p);
    assert_eq!(st.dx_loc_text, loc);

    p = point("JO50AA");
    p.lat += 0.001;
    st.my_loc_selected(p);
    assert_eq!(my(&st), p);
    assert!(st.my_loc_text.starts_with("JO50AA"));
    assert!(st.my_loc_text.len() > 6);
}

#[test]
fn short_locator_keeps_position() {
    let mut st = state();
    st.settings.my_height_m = 0.0;
    st.settings.dx_height_m = 0.0;
    st.set_my_loc_text("JO50AA");
    st.set_dx_loc_text("JO60AA");
    assert_eq!(st.settings.my_height_m, 10.0);
    assert_eq!(st.settings.dx_height_m, 10.0);
    assert!(st.locators_valid());
    assert_ne!(st.qtf_qrb(), (0, 0));

    st.set_my_loc_text("JO60");
    assert_eq!(my(&st), point("JO50AA"));
    assert_eq!(st.my_loc_text, "JO60");
    assert!(!st.locators_valid());
    assert_eq!(st.qtf_qrb(), (0, 0));
    assert!(st.play().is_err());

    st.set_my_loc_text("JO50AA");
    st.set_dx_loc_text("JO50");
    assert_eq!(dx(&st), point("JO60AA"));
    assert_eq!(st.dx_loc_text, "JO50");
    assert!(!st.locators_valid());
    assert_eq!(st.qtf_qrb(), (0, 0));

    st.set_dx_loc_text("JO60AA");
    st.my_call_changed("TE");
    st.set_my_loc_text("JO60");
    assert!(my(&st).lat.is_nan());
    assert!(!st.locators_valid());
}

const LOCATOR_LABELS: [&str; 2] = ["My Loc", "DX Loc"];
const FLORAL_WHITE: Color32 = Color32::from_rgb(255, 250, 240);
const PALE_GREEN: Color32 = Color32::from_rgb(152, 251, 152);

fn locator_harness() -> Harness<'static, AppState> {
    let mut st = state();
    for select in [AppState::my_loc_selected, AppState::dx_loc_selected] {
        let mut p = point("JO50AA");
        p.lat += 0.001;
        select(&mut st, p);
    }
    Harness::new_ui_state(control::show, st)
}

fn edit_locator(h: &mut Harness<'_, AppState>, label: &str, text: &str) {
    let field = h.get_by_role_and_label(Role::TextInput, label);
    field.focus();
    h.run();
    assert!(h.get_by_role_and_label(Role::TextInput, label).is_focused());

    h.get_by_role_and_label(Role::TextInput, label)
        .set_text_value_accesskit(text);
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, label)
            .value()
            .as_deref(),
        Some(text)
    );
}

fn assert_locator_colors(
    h: &Harness<'_, AppState>,
    label: &str,
    fill: Color32,
    foreground: Color32,
) {
    let rect = h.get_by_role_and_label(Role::TextInput, label).rect();
    let shapes = &h.output().shapes;
    assert!(
        shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Rect(shape) if shape.fill == fill && shape.rect.contains(rect.center())
        )),
        "{label} should have background {fill:?}"
    );
    assert!(
        shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Text(shape) if rect.contains(shape.pos)
                && shape.galley.job.sections.iter().all(|section| {
                    let color = section.format.color;
                    if color == Color32::PLACEHOLDER {
                        shape.fallback_color == foreground
                    } else {
                        color == foreground
                    }
                })
        )),
        "{label} should have foreground {foreground:?}"
    );
}

#[test]
fn focused_short_locator_is_floral_white_and_keeps_position() {
    for label in LOCATOR_LABELS {
        let mut h = locator_harness();
        let before = [my(h.state()), dx(h.state())];
        assert_locator_colors(&h, label, PALE_GREEN, Color32::BLACK);
        edit_locator(&mut h, label, "JO60");
        assert_eq!([my(h.state()), dx(h.state())], before);
        assert!(h.get_by_role_and_label(Role::TextInput, label).is_focused());
        assert_locator_colors(&h, label, FLORAL_WHITE, Color32::BLACK);
    }
}

#[test]
fn leaving_invalid_locator_sets_error_colors_and_keeps_focus() {
    for label in LOCATOR_LABELS {
        for click in [None, Some("My Call"), Some("DX Call")] {
            let mut h = locator_harness();
            edit_locator(&mut h, label, "XX99");
            assert_locator_colors(&h, label, FLORAL_WHITE, Color32::BLACK);
            if let Some(target) = click {
                h.get_by_role_and_label(Role::TextInput, target).click();
            } else {
                h.key_press(Key::Tab);
            }
            h.run();
            assert!(
                h.get_by_role_and_label(Role::TextInput, label).is_focused(),
                "{label} should keep focus after {click:?} click or Tab"
            );
            assert_locator_colors(&h, label, Color32::RED, Color32::WHITE);
        }
    }
}

#[test]
fn invalid_locator_keeps_focus_when_clicking_another_tab() {
    for label in LOCATOR_LABELS {
        let mut h = locator_harness();
        edit_locator(&mut h, label, "XX99");
        h.get_by_label("Multi").click();
        h.run();
        assert!(h.get_by_role_and_label(Role::TextInput, label).is_focused());
        assert_locator_colors(&h, label, Color32::RED, Color32::WHITE);
    }
}

#[test]
fn explicit_coordinates_allow_focus_to_leave_and_tab_to_change() {
    for label in LOCATOR_LABELS {
        let mut h = locator_harness();
        edit_locator(&mut h, label, "50.5, 10.2");
        assert!(h.state().locators_valid());

        h.key_press(Key::Tab);
        h.run();
        assert!(
            !h.get_by_role_and_label(Role::TextInput, label).is_focused(),
            "{label} must release focus for valid coordinates"
        );
        assert_locator_colors(&h, label, FLORAL_WHITE, Color32::BLACK);

        h.get_by_role_and_label(Role::TextInput, label).focus();
        h.run();
        h.get_by_label("Multi").click();
        h.run();
        assert_eq!(h.state().control_tab, airscout::app::ControlTab::Multi);
    }
}

#[test]
fn leaving_short_locator_keeps_original_colors() {
    for label in LOCATOR_LABELS {
        let mut h = locator_harness();
        edit_locator(&mut h, label, "JO60");
        h.key_press(Key::Tab);
        h.run();
        assert!(!h.get_by_role_and_label(Role::TextInput, label).is_focused());
        assert_locator_colors(&h, label, FLORAL_WHITE, Color32::BLACK);
    }
}

#[test]
fn editing_locator_after_error_restores_original_colors() {
    for label in LOCATOR_LABELS {
        let mut h = locator_harness();
        edit_locator(&mut h, label, "XX99");
        h.key_press(Key::Tab);
        h.run();
        assert_locator_colors(&h, label, Color32::RED, Color32::WHITE);

        // An edit clears the error even while the text is still invalid.
        edit_locator(&mut h, label, "XX9");
        assert_locator_colors(&h, label, FLORAL_WHITE, Color32::BLACK);
    }
}

#[test]
fn station_dropdowns_apply_selected_rows() {
    let mut st = state();
    st.settings.gui.my_calls = vec!["TE1ST".into(), "TE2ST".into()];
    st.settings.watchlist.0 = vec![WatchlistItem {
        call: "TE2ST".into(),
        loc: "JO61AA".into(),
        ..Default::default()
    }];
    let mut h = Harness::new_ui_state(control::show, st);
    h.run();

    for (button, item) in [
        ("My Call choices", "TE2ST"),
        ("My Loc choices", "JO60AA"),
        ("DX Call choices", "TE2ST"),
        ("DX Loc choices", "JO60AA"),
    ] {
        h.get_by_label(button).click();
        h.run();
        h.get_by_label(item).click();
        h.run();
        assert!(h.query_by_label(item).is_none());
    }

    assert_eq!(my(h.state()), point("JO60AA"));
    assert_eq!(dx(h.state()), point("JO60AA"));
    assert_eq!(h.state().settings.my_height_m, 10.0);
    assert_eq!(h.state().settings.dx_height_m, 10.0);
}

#[test]
fn station_locator_display_preferences_keep_fixed_database_key() {
    let mut st = state();
    let position = point("JO50AA");
    st.settings.gui.locator_max_length = 14;
    st.settings.gui.locator_auto_length = false;
    st.settings.gui.locator_small_letters_for_subsquares = true;
    st.my_loc_selected(position);
    st.dx_loc_selected(position);
    assert_eq!(st.my_loc_text.len(), 14);
    assert!(st.my_loc_text.starts_with("JO50aa"));
    assert_eq!(st.my_loc_text, st.dx_loc_text);
    assert_eq!(st.settings.my_loc().as_deref(), Some("JO50AA"));
    assert_eq!(st.settings.dx_loc().as_deref(), Some("JO50AA"));
}
