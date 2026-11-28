//! MapDlg main-window skeleton, driven through egui_kittest with the frame
//! trace on: the frame (`sc_Main` / `sc_Map` / `ss_Main`) must match the
//! upstream `MapDlg` reference, and the tab/collapse state must follow the
//! `AppState` methods.

mod gui_ref;

use airscout::app::{AppState, BottomTab, ControlTab, MapTab, RunMode, Services};
use airscout::services::PathRequest;
use airscout::ui::forms::trace::{self, Entry, Kind};
use airscout::ui::Maps;
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use eframe::egui;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use gui_ref::fidelity::{assert_subtree, assert_window};
use std::sync::Arc;

/// The test window: the reference client is 1008×730, plus the 24 pt menu bar.
const MAPDLG_TEST_SIZE: egui::Vec2 = egui::vec2(1008.0, 754.0);

struct Rec;

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {}
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
        Box::new(Rec),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn mapdlg(setup: impl FnOnce(&mut AppState)) -> Harness<'static, (AppState, Maps)> {
    let mut st = state();
    setup(&mut st);
    Harness::builder()
        .with_size(MAPDLG_TEST_SIZE)
        .build_ui_state(
            |ui, (st, maps): &mut (AppState, Maps)| {
                trace::enable(ui.ctx());
                airscout::ui::show(ui, st, maps);
            },
            (st, Maps::without_tiles()),
        )
}

/// Run `ui::show` in a `size` harness with tracing on and return the trace of
/// one clean frame.
fn trace_mapdlg_sized(size: egui::Vec2, setup: impl FnOnce(&mut AppState)) -> Vec<Entry> {
    let mut st = state();
    setup(&mut st);
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, (st, maps): &mut (AppState, Maps)| {
            trace::enable(ui.ctx());
            airscout::ui::show(ui, st, maps);
        },
        (st, Maps::without_tiles()),
    );
    trace::take(&h.ctx);
    h.step();
    trace::take(&h.ctx)
}

/// Run `ui::show` in a `MAPDLG_TEST_SIZE` harness with tracing on and return
/// the trace of one clean frame.
fn trace_mapdlg(setup: impl FnOnce(&mut AppState)) -> Vec<Entry> {
    let mut h = mapdlg(setup);
    // Drop the harness's own setup frames, then record one more.
    trace::take(&h.ctx);
    h.step();
    trace::take(&h.ctx)
}

#[test]
fn mapdlg_matches_reference() {
    // Start with the default, expanded layout for the Designer geometry check.
    // Later frames supply page bodies and their descendants. Tab headers and
    // bodies share a name, so keep one entry of each kind per path.
    let mut merged = Vec::new();
    for map_tab in [MapTab::Map, MapTab::News] {
        for bottom_tab in [
            BottomTab::Profile,
            BottomTab::Spectrum,
            BottomTab::Analysis,
            BottomTab::Planes,
        ] {
            for control_tab in [ControlTab::Single, ControlTab::Multi, ControlTab::Opt] {
                let entries = trace_mapdlg(|st| {
                    st.map_tab = map_tab;
                    st.bottom_tab = bottom_tab;
                    st.control_tab = control_tab;
                });
                for entry in entries {
                    if !merged.iter().any(|previous: &Entry| {
                        previous.path == entry.path
                            && previous.name == entry.name
                            && previous.kind == entry.kind
                    }) {
                        merged.push(entry);
                    }
                }
            }
        }
    }
    assert_window("MapDlg", &merged);
}

#[test]
fn skeleton_matches_reference() {
    let t = trace_mapdlg(|_| {});
    assert_subtree("MapDlg", "sc_Main", Some(0), &t);
    assert_subtree("MapDlg", "sc_Map", Some(2), &t);
    assert_subtree("MapDlg", "ss_Main", None, &t);
}

#[test]
fn default_split_matches_designer() {
    let t = trace_mapdlg(|_| {});
    let panel2 = t
        .iter()
        .find(|e| e.name == "sc_Main.Panel2")
        .expect("sc_Main.Panel2 recorded");
    assert!(
        (panel2.rect.width() - 152.0).abs() <= 1.0,
        "sc_Main.Panel2 width {} (expected 152)",
        panel2.rect.width()
    );
    let panel1 = t
        .iter()
        .find(|e| e.name == "sc_Map.Panel1")
        .expect("sc_Map.Panel1 recorded");
    assert!(
        (panel1.rect.height() - 342.0).abs() <= 1.0,
        "sc_Map.Panel1 height {} (expected 342)",
        panel1.rect.height()
    );
}

#[test]
fn tab_change_refused_while_playing() {
    let mut st = state();
    st.mode = RunMode::Playing;
    assert!(!st.set_map_tab(MapTab::News));
    assert_eq!(st.map_tab, MapTab::Map);
    assert!(!st.set_bottom_tab(BottomTab::Planes));
    assert_eq!(st.bottom_tab, BottomTab::Profile);
    assert!(!st.set_control_tab(ControlTab::Multi));
    assert_eq!(st.control_tab, ControlTab::Single);

    // Paused: every tab change is accepted.
    st.mode = RunMode::Paused;
    assert!(st.set_map_tab(MapTab::News));
    assert_eq!(st.map_tab, MapTab::News);
    assert!(st.set_bottom_tab(BottomTab::Planes));
    assert_eq!(st.bottom_tab, BottomTab::Planes);
    assert!(st.set_control_tab(ControlTab::Multi));
    assert_eq!(st.control_tab, ControlTab::Multi);
}

#[test]
fn lower_panel_collapsed_in_multi_and_news() {
    let mut st = state();
    assert!(!st.lower_panel_collapsed());

    st.control_tab = ControlTab::Multi;
    assert!(st.lower_panel_collapsed());
    st.control_tab = ControlTab::Single;

    st.map_tab = MapTab::News;
    assert!(st.lower_panel_collapsed());
    st.map_tab = MapTab::Map;
    assert!(!st.lower_panel_collapsed());
}

#[test]
fn playing_refuses_news_tab_click_before_rendering() {
    check_tab_click("Latest News", "tp_News", MapTab::News, BottomTab::Profile);
}

#[test]
fn playing_refuses_planes_tab_click_before_rendering() {
    check_tab_click("Planes", "tp_Planes", MapTab::Map, BottomTab::Planes);
}

fn check_tab_click(label: &str, page: &str, map: MapTab, bottom: BottomTab) {
    let mut h = mapdlg(|st| st.mode = RunMode::Playing);
    trace::take(&h.ctx);

    h.get_by_label(label).click();
    h.run();

    let entries = trace::take(&h.ctx);
    assert!(
        !entries
            .iter()
            .any(|e| e.name == page && e.kind == Kind::Container),
        "clicking {label} while playing rendered {page}"
    );
    assert_eq!(h.state().0.map_tab, MapTab::Map);
    assert_eq!(h.state().0.bottom_tab, BottomTab::Profile);

    h.state_mut().0.mode = RunMode::Paused;
    h.step();
    h.get_by_label(label).click();
    h.run();
    assert!(trace::take(&h.ctx)
        .iter()
        .any(|e| e.name == page && e.kind == Kind::Container));
    assert_eq!(h.state().0.map_tab, map);
    assert_eq!(h.state().0.bottom_tab, bottom);
}

#[test]
fn splitters_leave_defaults_unset_until_dragged() {
    let mut h = mapdlg(|_| {});
    let gui = &h.state().0.settings.gui;
    assert_eq!(
        (gui.side_panel_width, gui.bottom_panel_height),
        (None, None)
    );

    for (start, end) in [
        (egui::pos2(854.0, 150.0), egui::pos2(790.0, 150.0)),
        (egui::pos2(200.0, 368.0), egui::pos2(200.0, 408.0)),
    ] {
        h.drag_at(start);
        h.run();
        h.hover_at(end);
        h.run();
        h.drop_at(end);
        h.run();
    }

    assert_eq!(h.state().0.settings.gui.side_panel_width, Some(216.0));
    assert_eq!(h.state().0.settings.gui.bottom_panel_height, Some(320.0));
}

#[test]
fn window_shrink_preserves_saved_split_sizes() {
    let mut h = mapdlg(|st| {
        st.settings.gui.side_panel_width = Some(216.0);
        st.settings.gui.bottom_panel_height = Some(360.0);
    });
    h.set_size(egui::vec2(200.0, 320.0)).run();
    let gui = &h.state().0.settings.gui;
    assert_eq!(
        (gui.side_panel_width, gui.bottom_panel_height),
        (Some(216.0), Some(360.0))
    );

    h.set_size(MAPDLG_TEST_SIZE);
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);
    let bottom = entries.iter().find(|e| e.name == "sc_Map.Panel2").unwrap();
    assert_eq!(bottom.rect.height(), 360.0);
}

#[test]
fn collapsed_map_keeps_panel_one_trace_path() {
    for (control_tab, map_tab) in [
        (ControlTab::Multi, MapTab::Map),
        (ControlTab::Single, MapTab::News),
    ] {
        let entries = trace_mapdlg(|st| {
            st.control_tab = control_tab;
            st.map_tab = map_tab;
        });
        let map = entries.iter().find(|e| e.name == "tc_Map").unwrap();
        assert_eq!(
            map.path,
            ["sc_Main", "sc_Main.Panel1", "sc_Map", "sc_Map.Panel1"]
        );
        assert_eq!(
            rect_of(&entries, "sc_Map.Panel1"),
            rect_of(&entries, "sc_Map")
        );
        assert!(!entries.iter().any(|e| e.name == "tc_Main"));
    }
}

#[test]
fn status_spring_keeps_long_messages_clear_of_other_items() {
    let message = "TE1ST JO50AA ".repeat(100);
    let mut h = mapdlg(|st| st.status.clone_from(&message));
    for width in [1008.0, 900.0] {
        h.set_size(egui::vec2(width, 600.0));
        trace::take(&h.ctx);
        h.step();
        let entries = trace::take(&h.ctx);
        let status = entries.iter().find(|e| e.name == "tsl_Status").unwrap();
        let calculations = entries
            .iter()
            .find(|e| e.name == "tsl_Calculations")
            .unwrap();
        assert_eq!(status.text, message);
        assert!(status.rect.width() > 0.0);
        assert!(
            status.rect.max.x <= calculations.rect.min.x,
            "status {:?} overlaps calculations {:?} at width {width}",
            status.rect,
            calculations.rect
        );
    }
}

#[test]
fn control_column_matches_reference() {
    // The comparer expects every visible Designer descendant. Collect the
    // three pages from separate frames, keeping each page's own geometry.
    let entries: Vec<_> = [ControlTab::Single, ControlTab::Multi, ControlTab::Opt]
        .into_iter()
        .flat_map(|tab| trace_mapdlg(|st| st.control_tab = tab))
        .collect();
    for root in [
        "pa_CommonInfo",
        "tc_Control",
        "gb_Map_Zoom",
        "gb_Map_Buttons",
    ] {
        assert_subtree("MapDlg", root, None, &entries);
    }
}

#[test]
fn zoom_readout_shows_map_zoom() {
    let mut h = mapdlg(|_| {});
    let readout = |h: &mut Harness<'_, (AppState, Maps)>| {
        trace::take(&h.ctx);
        h.step();
        let entries = trace::take(&h.ctx);
        let text = &entries
            .iter()
            .find(|entry| entry.name == "tb_Zoom")
            .unwrap()
            .text;
        assert_eq!(*text, format!("{:.0}", h.state().0.map_zoom));
        text.parse::<i32>().expect("integer zoom")
    };

    let initial = readout(&mut h);
    h.get_by_label("+").click();
    h.run();
    assert_eq!(readout(&mut h), initial + 1);
    h.get_by_label("-").click();
    h.run();
    assert_eq!(readout(&mut h), initial);
}

#[test]
fn group_title_click_toggles_box() {
    let click = |h: &mut Harness<'_, (AppState, Maps)>, pos| {
        h.hover_at(pos);
        h.run();
        h.drag_at(pos);
        h.run();
        h.drop_at(pos);
        h.run();
    };
    for (tab, group, child, setting) in [
        (ControlTab::Single, "gb_Map_Info", "cb_MyCall", 0),
        (ControlTab::Single, "gb_Map_Zoom", "btn_Zoom_In", 1),
        (ControlTab::Opt, "gb_Map_Filter", "pa_Planes_Filter", 2),
        (ControlTab::Opt, "gb_Map_Alarms", "cb_Alarms_Activate", 3),
    ] {
        let mut h = mapdlg(|st| st.control_tab = tab);
        trace::take(&h.ctx);
        h.step();
        let entries = trace::take(&h.ctx);
        let rect = entries.iter().find(|e| e.name == group).unwrap().rect;
        assert!(entries.iter().any(|e| e.name == child));

        // A click in the group body leaves the setting alone.
        click(&mut h, rect.right_bottom() - egui::vec2(4.0, 4.0));
        let mut title = rect.min;
        for (expanded, click_y) in [(false, 15.0), (true, 18.0)] {
            trace::take(&h.ctx);
            click(&mut h, title + egui::vec2(22.0, click_y));
            trace::take(&h.ctx);
            h.step();
            let entries = trace::take(&h.ctx);
            let gui = &h.state().0.settings.gui;
            assert_eq!(
                [
                    gui.show_info_box,
                    gui.show_zoom_box,
                    gui.show_filter_box,
                    gui.show_alarm_box
                ][setting],
                expanded,
                "{group}"
            );
            assert_eq!(entries.iter().any(|e| e.name == child), expanded, "{group}");
            let actual = entries.iter().find(|e| e.name == group).unwrap().rect;
            assert_eq!(actual.height(), if expanded { rect.height() } else { 19.0 });
            title = actual.min;
        }
    }
}

#[test]
fn control_column_reachable_at_small_sizes() {
    for size in [egui::vec2(1366.0, 768.0), egui::vec2(900.0, 600.0)] {
        for tab in [ControlTab::Single, ControlTab::Multi, ControlTab::Opt] {
            let mut h = mapdlg(|st| st.control_tab = tab);
            h.set_size(size).run();
            trace::take(&h.ctx);
            h.step();
            let entries = trace::take(&h.ctx);
            let window = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut names = vec![
                "tp_Control_Single",
                "tp_Control_Multi",
                "tp_Control_Options",
                "btn_Options",
                "btn_Map_Save",
                "btn_Map_PlayPause",
                "btn_Zoom_In",
                "tb_Zoom",
                "btn_Zoom_Out",
                "cb_AutoCenter",
            ];
            match tab {
                ControlTab::Single => {
                    names.extend(["cb_MyCall", "cb_MyLoc", "cb_DXCall", "cb_DXLoc", "tb_QRB"]);
                    if size.y > 600.0 {
                        names.push("tb_QTF");
                    }
                }
                ControlTab::Multi => names.extend([
                    "ch_Control_Watchlist_Call",
                    "ch_Control_Watchlist_Loc",
                    "btn_Control_Manage_Watchlist",
                ]),
                ControlTab::Opt => names.extend([
                    "tb_Planes_Filter_Max_Circumcircle",
                    "tb_Planes_Filter_Min_Alt",
                    "cb_Planes_Filter_Min_Cat",
                    "cb_Alarms_Activate",
                ]),
            }

            let mut rects: Vec<(&str, egui::Rect)> = Vec::new();
            for name in names {
                let rect = entries.iter().find(|e| e.name == name).unwrap().rect;
                assert!(
                    rect.is_positive() && window.contains_rect(rect),
                    "{name} at {size:?}"
                );
                for &(other, previous) in &rects {
                    assert!(
                        !rect.intersect(previous).is_positive(),
                        "{name} overlaps {other} at {size:?}"
                    );
                }
                rects.push((name, rect));
            }
            if tab == ControlTab::Single && size.y == 600.0 {
                let page = entries
                    .iter()
                    .find(|e| e.name == "tp_Control_Single" && e.kind == Kind::Container)
                    .unwrap()
                    .rect;
                let qtf = entries.iter().find(|e| e.name == "tb_QTF").unwrap();
                assert!(qtf.rect.max.y > page.max.y);
                h.hover_at(page.center());
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -160.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
                h.run();
                trace::take(&h.ctx);
                h.step();
                let scrolled = trace::take(&h.ctx);
                let qtf = scrolled.iter().find(|e| e.name == "tb_QTF").unwrap();
                assert!(qtf.rect.is_positive() && page.contains_rect(qtf.rect));
            }

            // Save must be actionable even while the tab itself is scrollable.
            h.get_by_label("Save").click();
            h.run();
            assert!(h.state().0.map_save_pending);
        }
    }
}

#[test]
fn station_fields_read_only_while_playing() {
    use eframe::egui::accesskit::Role;

    let mut h = mapdlg(|_| {});
    for playing in [false, true, false] {
        h.state_mut().0.mode = if playing {
            RunMode::Playing
        } else {
            RunMode::Paused
        };
        h.run();
        for label in ["My Call", "My Loc", "DX Call", "DX Loc"] {
            let input = h.get_by_role_and_label(Role::TextInput, label);
            assert_eq!(input.accesskit_node().is_disabled(), playing, "{label}");
        }
    }
}

#[test]
fn lower_tabs_match_reference() {
    // Each bottom tab's page only renders when selected, so collect each page
    // from its own frame, keeping the page's own geometry (as in
    // `control_column_matches_reference`).
    let entries: Vec<_> = [BottomTab::Profile, BottomTab::Spectrum, BottomTab::Analysis]
        .into_iter()
        .flat_map(|tab| trace_mapdlg(|st| st.bottom_tab = tab))
        .collect();
    for root in [
        "tp_Elevation",
        "gb_Spectrum_Status",
        "gb_Spectrum",
        "gb_Spectrum_NearestInfo",
        "tp_Analysis",
    ] {
        assert_subtree("MapDlg", root, None, &entries);
    }
}

#[test]
fn spectrum_tab_matches_reference() {
    let entries = trace_mapdlg(|st| st.bottom_tab = BottomTab::Spectrum);
    assert_subtree("MapDlg", "tp_Spectrum", None, &entries);
    let group = rect_of(&entries, "gb_NearestPlaneMap");
    let map = rect_of(&entries, "gm_Nearest");
    let page = entries
        .iter()
        .find(|e| e.name == "tp_Spectrum" && e.kind == Kind::Container)
        .unwrap()
        .rect;
    let content = page.size().max(egui::vec2(844.0, 333.0));
    assert_eq!(group.size(), egui::vec2(168.0, content.y - 60.0));
    assert_eq!(group.min - page.min, egui::vec2(content.x - 174.0, 54.0));
    assert_eq!(map.min - group.min, egui::vec2(3.0, 16.0));
    assert_eq!(map.size(), egui::vec2(162.0, group.height() - 19.0));
}

/// The drawn rect of `name` (a `Container` or `Text` entry).
fn rect_of(t: &[Entry], name: &str) -> egui::Rect {
    t.iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} not in trace"))
        .rect
}

#[test]
fn spectrum_status_and_nearest_stretch_to_group() {
    let t = trace_mapdlg(|st| st.bottom_tab = BottomTab::Spectrum);
    let status_group = rect_of(&t, "gb_Spectrum_Status");
    let status = rect_of(&t, "tb_Spectrum_Status");
    let expected = (status_group.width() - 12.0).max(129.0);
    assert!(
        (status.width() - expected).abs() <= 1.0,
        "tb_Spectrum_Status width {} vs {expected}",
        status.width()
    );

    let nearest = rect_of(&t, "gb_Spectrum_NearestInfo");
    for name in [
        "lbl_Nearest_Call",
        "lbl_Nearest_Type",
        "lbl_Nearest_Cat",
        "lbl_Nearest_Alt",
        "lbl_Nearest_Angle",
        "lbl_Nearest_Dist",
    ] {
        let r = rect_of(&t, name);
        let expected = (nearest.width() - 88.0).max(14.0);
        assert!(
            (r.width() - expected).abs() <= 1.0,
            "{name} width {} vs {expected}",
            r.width()
        );
        assert!(r.height() >= 18.0, "{name} height {} < 18", r.height());
    }
}

#[test]
fn label_extends_right_from_designer_x() {
    // `label()` renders on one line starting at the Designer x (left-aligned),
    // growing right past the rect instead of centring (WinForms auto-size). A
    // long caption at x = 0 must keep its left edge at 0, not shift left.
    use airscout::ui::forms::widgets::label;
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 80.0))
        .build_ui(|ui| {
            label(
                ui,
                "label13",
                "Maximum Circumcircle:",
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 16.0)),
            );
        });
    h.run();
    let r = h.get_by_label("Maximum Circumcircle:").rect();
    assert!(
        (r.min.x - 0.0).abs() < 1.0,
        "left edge {} shifted from designer x 0",
        r.min.x
    );
    assert!(
        r.width() > 40.0,
        "caption extends right past its 40 pt rect: {}",
        r.width()
    );
    assert!(r.height() < 20.0, "single line, height {}", r.height());
}

#[test]
fn spectrum_text_aligns_left_at_designer_x() {
    let short_status = "standby";
    let long_status = "Spectrum Lab: waiting for data. ".repeat(20);
    let call = "TE1ST";
    let mut h = mapdlg(|st| {
        st.bottom_tab = BottomTab::Spectrum;
        st.spectrum.status = short_status.into();
        st.tick.nearest = Some(as_aircraft::PlaneInfo {
            call: call.into(),
            type_code: "A320".into(),
            ..as_aircraft::PlaneInfo::default()
        });
    });
    trace::take(&h.ctx);
    h.step();
    let entries = trace::take(&h.ctx);

    let status_group = rect_of(&entries, "gb_Spectrum_Status");
    let s = h.get_by_label(short_status).rect();
    assert!(
        (s.left() - (status_group.min.x + 6.0)).abs() <= 1.0,
        "status left {} != designer x {}",
        s.left(),
        status_group.min.x + 6.0
    );
    assert!(
        s.height() < 30.0,
        "status single-line height {}",
        s.height()
    );

    let nearest_group = rect_of(&entries, "gb_Spectrum_NearestInfo");
    let c = h.get_by_label(call).rect();
    assert!(
        (c.left() - (nearest_group.min.x + 82.0)).abs() <= 1.0,
        "call left {} != designer x {}",
        c.left(),
        nearest_group.min.x + 82.0
    );
    assert!(c.height() < 30.0, "call single-line height {}", c.height());

    h.state_mut().0.spectrum.status = long_status.clone();
    h.step();
    let status_group = rect_of(&trace::take(&h.ctx), "gb_Spectrum_Status");
    let s = h.get_by_label(long_status.as_str()).rect();
    let designer_x = status_group.left() + 6.0;
    assert!(
        (s.left() - designer_x).abs() <= 1.0,
        "long status left {} != designer x {designer_x}",
        s.left()
    );
    assert!(s.height() < 30.0, "long status height {}", s.height());
    assert!(
        s.right() <= status_group.right() - 6.0 + 1.0,
        "long status right {} exceeds the status field",
        s.right()
    );
}

#[test]
fn spectrum_nearest_keeps_last_fix_after_contact_loss() {
    use as_aircraft::{PlaneCategory, PlaneInfo};
    use as_geo::latlon::GPoint;

    let mut h = mapdlg(|st| st.bottom_tab = BottomTab::Spectrum);
    let texts = |h: &mut Harness<'_, (AppState, Maps)>| {
        trace::take(&h.ctx);
        h.step();
        let entries = trace::take(&h.ctx);
        ["Call", "Type", "Cat", "Alt", "Angle", "Dist"].map(|suffix| {
            let name = format!("lbl_Nearest_{suffix}");
            entries
                .iter()
                .find(|e| e.name == name)
                .unwrap()
                .text
                .clone()
        })
    };
    assert_eq!(texts(&mut h), ["0"; 6]);

    h.state_mut().0.tick.nearest = Some(PlaneInfo {
        hex: "3C0001".into(),
        call: "TE1ST".into(),
        type_code: "A320".into(),
        category: PlaneCategory::Medium,
        alt_ft: 10668.4 * 3.28084,
        angle: 0.1,
        int_qrb: 123.6,
        int_point: Some(GPoint::new(0.0, 0.0)),
        ..PlaneInfo::default()
    });
    let first = ["TE1ST", "A320", "Medium", "10668m", "6°", "124km"];
    assert_eq!(texts(&mut h), first);
    h.state_mut().0.tick.nearest = None;
    assert_eq!(texts(&mut h), first);

    h.state_mut().0.bottom_tab = BottomTab::Profile;
    h.step();
    h.state_mut().0.bottom_tab = BottomTab::Spectrum;
    assert_eq!(texts(&mut h), first);
}

#[test]
fn spectrum_long_nearest_truncates_single_line() {
    // A long nearest value must stay on one line and truncate at the stretched
    // right edge instead of wrapping.
    let call = "DLH1234567890ABCDEF";
    let mut h = mapdlg(|st| {
        st.bottom_tab = BottomTab::Spectrum;
        st.tick.nearest = Some(as_aircraft::PlaneInfo {
            call: call.into(),
            type_code: "A380-800".into(),
            ..as_aircraft::PlaneInfo::default()
        });
    });
    trace::take(&h.ctx);
    h.step();
    let entries = trace::take(&h.ctx);
    let nearest_group = rect_of(&entries, "gb_Spectrum_NearestInfo");
    let c = h.get_by_label(call).rect();
    assert!(c.height() < 30.0, "call single-line height {}", c.height());
    assert!(
        c.width() <= nearest_group.width() - 88.0 + 1.0,
        "call truncated to the box: width {} > {}",
        c.width(),
        nearest_group.width() - 88.0
    );
}

#[test]
fn lower_tabs_keep_designer_size_when_scrolled() {
    for size in [egui::vec2(1366.0, 768.0), egui::vec2(900.0, 600.0)] {
        // Analysis: below the 844pt designer width the content keeps its full
        // width (scrollable), so the player group keeps its 591pt size instead
        // of clipping; at larger sizes it stretches (Fill).
        let t = trace_mapdlg_sized(size, |st| st.bottom_tab = BottomTab::Analysis);
        let player = rect_of(&t, "gb_Analysis_Player");
        assert!(
            player.width() >= 591.0 - 1.0,
            "gb_Analysis_Player width {} at {size:?}",
            player.width()
        );
        if size.x <= 900.0 {
            assert!(
                (player.width() - 591.0).abs() <= 1.0,
                "gb_Analysis_Player width {} at {size:?}",
                player.width()
            );
        }
        let controls = rect_of(&t, "gb_Analysis_Controls");
        let ff = rect_of(&t, "btn_Analysis_FastForward");
        assert!(
            controls.contains_rect(ff),
            "FastForward not inside Controls at {size:?}"
        );

        // Spectrum: the spectrum group keeps its full designer height.
        let t = trace_mapdlg_sized(size, |st| st.bottom_tab = BottomTab::Spectrum);
        let spec = rect_of(&t, "gb_Spectrum");
        assert!(
            spec.height() >= 273.0 - 1.0,
            "gb_Spectrum height {} at {size:?}",
            spec.height()
        );
        let nearest = rect_of(&t, "gb_Spectrum_NearestInfo");
        assert!(nearest.is_positive(), "NearestInfo positive at {size:?}");
    }
}

#[test]
fn nearest_texts_formats() {
    use airscout::ui::planes::nearest_texts;
    use as_aircraft::{PlaneCategory, PlaneInfo};

    let p = PlaneInfo {
        call: "TE1ST".into(),
        type_code: "A320".into(),
        category: PlaneCategory::Medium,
        alt_ft: 10668.4 * 3.28084,
        angle: 0.1,
        int_qrb: 123.6,
        ..PlaneInfo::default()
    };
    let texts = nearest_texts(&p);
    assert_eq!(
        texts,
        [
            "TE1ST".to_string(),
            "A320".to_string(),
            "Medium".to_string(),
            "10668m".to_string(),
            "6°".to_string(),
            "124km".to_string(),
        ]
    );
}
