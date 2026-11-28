//! OptionsDlg fidelity: the Designer frame, tab order and the scoped Phase 3A
//! comparer that reuses the Phase 2 strict matcher against `OptionsDlg.json`.

mod gui_ref;

use airscout::ui::forms::trace::{self, Entry, Kind};
use airscout::ui::options::{OptionsDlg, OptionsTab};
use as_settings::{Settings, TrackUdp};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use gui_ref::fidelity::{assert_subtree, compare, load_allow, load_deviations};
use gui_ref::model::Window;
use std::cell::RefCell;
use std::rc::Rc;

/// The six deferred Designer headers, omitted from the Phase 3A reference projection.
const DEFERRED_HEADERS: [&str; 6] = [
    "tab_Options_GLOBE",
    "tab_Options_SRTM3",
    "tab_Options_SRTM1",
    "tab_Options_ASTER3",
    "tba_Option_ASTER1",
    "tab_Options_Misc",
];

/// The four pages whose bodies Phase 3A owns (`include_bodies = true`).
const PHASE_A_BODIES: [&str; 4] = [
    "tab_Options_General",
    "tab_Options_Stations",
    "tab_Options_Planes",
    "tab_Options_Map",
];

/// The Rust-only interim combined elevation page (Decision 1), dropped before `compare`.
const INTERIM_ELEVATION: &str = "tab_Options_Elevation";

/// Every tab page name the Rust dialog draws (the interim page included).
const RUST_TAB_NAMES: [&str; 14] = [
    "tab_Options_General",
    "tab_Options_Database",
    "tab_Options_Stations",
    "tab_Options_Map",
    "tab_Options_Elevation",
    "tab_Options_Path",
    "tab_Options_Planes",
    "tab_Options_Alarm",
    "tab_Options_Network",
    "tab_Options_SpecLab",
    "tab_Options_Track",
    "tab_Options_CAT",
    "tab_Options_Watchlist",
    "tab_Options_Info",
];

/// The nearest `TabPage` that is `name` itself or one of its ancestors, if any.
fn tab_page_of(reference: &Window, name: &str) -> Option<String> {
    let mut cur = name.to_string();
    loop {
        let control = reference.controls.iter().find(|c| c.name == cur)?;
        if control.kind == "TabPage" {
            return Some(control.name.clone());
        }
        if control.parent == "$this" || control.parent == cur {
            return None;
        }
        cur = control.parent.clone();
    }
}

/// Project the reference to the Phase 3A scope: the six deferred headers and their
/// subtrees are dropped, and for `include_bodies = false` every tab body is dropped too
/// (for `true` only the four Phase 3A bodies are kept, the rest keep just their header).
fn project_reference(reference: &Window, include_bodies: bool) -> Window {
    let controls = reference
        .controls
        .iter()
        .filter(|c| match tab_page_of(reference, &c.name) {
            None => true,
            Some(page) => {
                if DEFERRED_HEADERS.contains(&page.as_str()) {
                    return false;
                }
                if include_bodies {
                    PHASE_A_BODIES.contains(&page.as_str()) || c.name == page
                } else {
                    c.name == page
                }
            }
        })
        .cloned()
        .collect();
    Window {
        controls,
        ..reference.clone()
    }
}

/// Filter the drawn trace to the same scope: drop the interim `Elevation` header and every
/// tab body that `include_bodies` does not keep.
fn project_trace(trace: &[Entry], include_bodies: bool) -> Vec<Entry> {
    trace
        .iter()
        .filter(|e| {
            if e.name == INTERIM_ELEVATION {
                return false;
            }
            for segment in &e.path {
                if RUST_TAB_NAMES.contains(&segment.as_str())
                    && !(include_bodies && PHASE_A_BODIES.contains(&segment.as_str()))
                {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Compare the projected frame (and, for `include_bodies`, the four Phase 3A page
/// subtrees) against the reference with the strict matcher and the Options allowlist.
fn assert_options_phase_a(trace: &[Entry], include_bodies: bool) {
    let reference = gui_ref::load("OptionsDlg");
    let allow = load_allow("OptionsDlg");
    let deviations = load_deviations();
    let mismatches = compare(
        &project_reference(&reference, include_bodies),
        &project_trace(trace, include_bodies),
        &allow,
        &deviations,
    );
    assert!(
        mismatches.is_empty(),
        "OptionsDlg Phase 3A fidelity mismatches:\n{}",
        mismatches
            .iter()
            .map(|m| format!("- {m:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The drawn rect of `name` (a `Container` or `Text` entry).
fn rect_of(t: &[Entry], name: &str) -> egui::Rect {
    t.iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} not in trace"))
        .rect
}

/// Merge the traces of several frames into one, keeping a single entry per
/// `(path, name, kind)`: a `TabPage` recorded as its `Kind::Text` header and as
/// its `Kind::Container` body are distinct, while the same header drawn on two
/// frames is not duplicated. First occurrence wins.
fn merge_traces(frames: impl IntoIterator<Item = Vec<Entry>>) -> Vec<Entry> {
    let mut merged: Vec<Entry> = Vec::new();
    for frame in frames {
        for entry in frame {
            if !merged
                .iter()
                .any(|m| m.path == entry.path && m.name == entry.name && m.kind == entry.kind)
            {
                merged.push(entry);
            }
        }
    }
    merged
}

/// Run the real dialog at `size` / `ppp` for one settled frame with tracing on.
fn trace_options_with_ppp(
    tab: OptionsTab,
    size: egui::Vec2,
    ppp: f32,
    setup: impl FnOnce(&mut OptionsDlg),
) -> Vec<Entry> {
    let mut dlg = OptionsDlg::new(&Settings::default());
    setup(&mut dlg);
    dlg.tab = tab;
    let mut h = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(ppp)
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                trace::enable(ui.ctx());
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );
    trace::take(&h.ctx);
    h.step();
    trace::take(&h.ctx)
}

/// Run the real dialog at `size` for one settled frame with tracing on.
fn trace_options(
    tab: OptionsTab,
    size: egui::Vec2,
    setup: impl FnOnce(&mut OptionsDlg),
) -> Vec<Entry> {
    trace_options_with_ppp(tab, size, 1.0, setup)
}

#[test]
fn frame_and_tab_order() {
    let trace = trace_options(OptionsTab::General, egui::vec2(900.0, 600.0), |_| {});
    assert_options_phase_a(&trace, false);

    let frame = rect_of(&trace, "$this");
    assert_eq!(frame.size(), egui::vec2(784.0, 561.0));

    let status = rect_of(&trace, "ss_Options");
    assert!(
        (status.bottom() - frame.bottom()).abs() <= 0.001,
        "status strip {:?} does not touch the frame bottom {:?}",
        status,
        frame
    );

    let ok = rect_of(&trace, "btn_Options_OK");
    let cancel = rect_of(&trace, "btn_Options_Cancel");
    assert!(
        ok.bottom() < cancel.top(),
        "OK {:?} overlaps Cancel {:?}",
        ok,
        cancel
    );

    // The interim combined Elevation page sits between Map and Path (Decision 1).
    let map = rect_of(&trace, "tab_Options_Map");
    let elev = rect_of(&trace, INTERIM_ELEVATION);
    let path = rect_of(&trace, "tab_Options_Path");
    assert!(
        map.min.x < elev.min.x && elev.min.x < path.min.x,
        "Elevation header {:?} not between Map {:?} and Path {:?}",
        elev,
        map,
        path
    );
}

#[test]
fn small_frame_footer_reachable() {
    for ppp in [1.0, 2.0] {
        let dlg = OptionsDlg::new(&Settings::default());
        let h = Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .with_pixels_per_point(ppp)
            .build_ui_state(
                |ui, dlg: &mut OptionsDlg| {
                    let _ = dlg.show(ui.ctx());
                },
                dlg,
            );
        let viewport = h.ctx.content_rect();
        for name in ["OK", "Cancel"] {
            let rect = h.get_by_label(name).rect();
            assert!(
                viewport.contains_rect(rect),
                "{name} {:?} outside the clipped viewport {:?} at ppp {ppp}",
                rect,
                viewport
            );
        }
    }
}

/// Drain the frames accumulated while `h` settled, then run one more settled
/// frame and return its trace. The dialog must be traced via
/// [`trace::enable`] in the harness closure for this to return entries.
fn settled_trace(h: &mut Harness<'static, OptionsDlg>) -> Vec<Entry> {
    h.run();
    trace::take(&h.ctx);
    h.step();
    trace::take(&h.ctx)
}

/// Assert that the footer and status strip are fully (not partially) inside the
/// viewport in the given trace.
fn assert_footer_inside_viewport(h: &Harness<'static, OptionsDlg>, trace: &[Entry], when: &str) {
    let viewport = h.ctx.content_rect();
    for name in ["btn_Options_OK", "btn_Options_Cancel", "ss_Options"] {
        let rect = rect_of(trace, name);
        assert!(
            viewport.contains_rect(rect),
            "{name} {rect:?} is not fully inside the viewport {viewport:?} {when}"
        );
    }
}

#[test]
fn dialog_tracks_live_viewport() {
    let dlg = OptionsDlg::new(&Settings::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1366.0, 768.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                trace::enable(ui.ctx());
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );

    // Open large and settle, then shrink below the 784×561 Designer client: each
    // footer control must scroll into reach and be fully (not partially) inside
    // the viewport. `Status` is the ss_Options status text; the strip itself is
    // the full 784-wide client, which cannot fit a 400-wide viewport whole.
    h.run();
    h.set_size(egui::vec2(900.0, 600.0));
    h.run();
    h.set_size(egui::vec2(400.0, 300.0));
    h.run();
    let viewport = h.ctx.content_rect();
    for name in ["OK", "Cancel", "Status"] {
        h.get_by_label(name).scroll_to_me();
        h.run();
        let rect = h.get_by_label(name).rect();
        assert!(
            viewport.contains_rect(rect),
            "{name} {rect:?} is not fully reachable in the viewport {viewport:?} after shrinking"
        );
    }

    // Grow back: the client is the full 784×561 again and no scrollbars are
    // needed, so the whole client and footer are visible without scrolling.
    h.set_size(egui::vec2(1366.0, 768.0));
    let trace = settled_trace(&mut h);
    let client = rect_of(&trace, "$this");
    assert_eq!(client.size(), egui::vec2(784.0, 561.0));
    let viewport = h.ctx.content_rect();
    assert!(
        viewport.contains_rect(client),
        "client {client:?} is not fully inside the viewport {viewport:?} after regrowing"
    );
    assert_footer_inside_viewport(&h, &trace, "after regrowing");
}

#[test]
fn dialog_reopens_at_default_size() {
    let open = Rc::new(RefCell::new(true));
    let flag = open.clone();
    let dlg = OptionsDlg::new(&Settings::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 300.0))
        .build_ui_state(
            move |ui, dlg: &mut OptionsDlg| {
                trace::enable(ui.ctx());
                if *flag.borrow() {
                    let _ = dlg.show(ui.ctx());
                }
            },
            dlg,
        );

    // A small session settles the area at the small size.
    let trace = settled_trace(&mut h);
    let client = rect_of(&trace, "$this");
    assert_eq!(client.size(), egui::vec2(784.0, 561.0));

    // Close it, then reopen it in a large viewport: it must be the full
    // Designer client again, not the stale small scrolled box.
    *open.borrow_mut() = false;
    settled_trace(&mut h);
    *open.borrow_mut() = true;
    h.set_size(egui::vec2(1366.0, 768.0));
    let trace = settled_trace(&mut h);
    let client = rect_of(&trace, "$this");
    assert_eq!(client.size(), egui::vec2(784.0, 561.0));
    let viewport = h.ctx.content_rect();
    assert!(
        viewport.contains_rect(client),
        "reopened client {client:?} is not fully inside the viewport {viewport:?}"
    );
    assert_footer_inside_viewport(&h, &trace, "after reopening at the default size");
}

#[test]
fn maintenance_still_blocks_exit() {
    let mut dlg = OptionsDlg::new(&Settings::default());
    dlg.maint_running = true;
    // Latch every outcome so a transient `Some` on the click frame cannot be
    // overwritten by a later settle frame returning `None`.
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let slot = outcomes.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            move |ui, dlg: &mut OptionsDlg| {
                if let Some(outcome) = dlg.show(ui.ctx()) {
                    slot.borrow_mut().push(outcome);
                }
            },
            dlg,
        );
    for button in ["OK", "Cancel"] {
        assert!(
            h.get_by_label(button).accesskit_node().is_disabled(),
            "{button} is not disabled while a destructive job runs"
        );
    }
    h.get_by_label("OK").click();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(
        outcomes.borrow().is_empty(),
        "a destructive job still allowed an exit: {:?}",
        *outcomes.borrow()
    );
}

#[test]
fn title_is_rendered() {
    let dlg = OptionsDlg::new(&Settings::default());
    let h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );
    h.get_by_label("Air Scout Options");
}

#[test]
fn refusing_guard_keeps_departing_tab() {
    let mut dlg = OptionsDlg::new(&Settings::default());
    dlg.test_invalid_departure = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );
    // Each refusal replaces the departing page's message instead of appending it.
    for _ in 0..2 {
        h.get_by_label("Database").click();
        h.run();
        let dlg = h.state();
        assert_eq!(
            dlg.tab,
            OptionsTab::General,
            "refused switch must keep General"
        );
        assert_eq!(dlg.status_texts().1, "The current page has invalid values.");
    }
    assert!(h.state().errors.is_empty(), "no OK errors were recorded");

    // Fixed in place: the message goes away before the user leaves the page.
    h.state_mut().test_invalid_departure = false;
    h.run();
    assert_eq!(h.state().status_texts().0, "Status");

    // Refused again, then fixed and left: the switch clears the message.
    h.state_mut().test_invalid_departure = true;
    h.get_by_label("Database").click();
    h.run();
    assert_eq!(h.state().tab, OptionsTab::General);
    h.state_mut().test_invalid_departure = false;
    h.get_by_label("Database").click();
    h.run();
    assert_eq!(h.state().tab, OptionsTab::Database);
    assert_eq!(h.state().status_texts().0, "Status");
}

#[test]
fn refused_switch_then_ok_shows_one_message() {
    let mut s = Settings::default();
    s.planes_max_alt_m = s.planes_min_alt_m - 1;
    let mut h = dialog_harness(OptionsTab::Planes, &s);
    h.get_by_label("Map").click();
    h.run();
    assert_eq!(h.state().tab, OptionsTab::Planes);
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(
        h.state().status_texts().1,
        "MaxAlt must be greater than MinAlt."
    );
}

#[test]
fn status_strip_summary_and_hover() {
    let mut dlg = OptionsDlg::new(&Settings::default());
    // Idle.
    assert_eq!(dlg.status_texts(), ("Status".to_string(), String::new()));

    // Errors only: single-line summary, one message per hover line.
    dlg.errors = vec!["E1".to_string(), "E2".to_string()];
    assert_eq!(
        dlg.status_texts(),
        ("E1; E2".to_string(), "E1\nE2".to_string())
    );

    // Errors and a port warning: the warning is not dropped and there is no
    // trailing newline.
    dlg.draft.server.activate = true;
    dlg.draft.track.activate = true;
    dlg.draft.track.udp = TrackUdp::WinTest;
    dlg.draft.track.udp_wintest_port = dlg.draft.server.port;
    let warning =
        "Network: port 9872 is used by the UDP server and by the tracking output (Win-Test).";
    assert_eq!(
        dlg.status_texts(),
        (format!("E1; E2; {warning}"), format!("E1\nE2\n{warning}"))
    );

    // Warnings only.
    dlg.errors.clear();
    assert_eq!(
        dlg.status_texts(),
        (warning.to_string(), warning.to_string())
    );
}

#[test]
fn merge_traces_keeps_one_entry_per_path_name_kind() {
    let entry = |name: &str, kind: trace::Kind| Entry {
        path: vec!["tc_Options".to_string(), "tab_Options_General".to_string()],
        name: name.to_string(),
        kind,
        text: name.to_string(),
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0)),
    };
    let header = entry("tab_Options_General", trace::Kind::Text);
    let body = entry("tab_Options_General", trace::Kind::Container);
    let merged = merge_traces([vec![header.clone(), header.clone()], vec![body.clone()]]);
    assert_eq!(merged.len(), 2, "header and body are distinct by kind");
    assert!(merged.iter().any(|e| e.kind == trace::Kind::Text));
    assert!(merged.iter().any(|e| e.kind == trace::Kind::Container));
    assert_eq!(
        merged
            .iter()
            .filter(|e| e.kind == trace::Kind::Text)
            .count(),
        1,
        "repeated headers collapse to one"
    );
}

#[test]
fn general_matches_designer() {
    let trace = trace_options(OptionsTab::General, egui::vec2(900.0, 600.0), |_| {});
    assert_subtree("OptionsDlg", "tab_Options_General", None, &trace);
    let map = rect_of(&trace, "gm_Options_Coverage");
    assert!(
        map.is_positive(),
        "coverage map rect {map:?} is not positive"
    );
}

#[test]
fn reentering_general_refits_the_preview_camera() {
    let dlg = OptionsDlg::new(&Settings::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );

    // Entering General (the default tab) fits the camera to the area.
    h.run();
    let fitted = h.state().preview_camera();
    assert!(fitted.0.is_some(), "the camera is fitted on entry");

    // Pan and zoom the preview away from the fitted view.
    h.state_mut().move_preview_camera(0.0, 0.0, 5.0);
    h.run();
    assert_ne!(
        h.state().preview_camera(),
        fitted,
        "the camera moved away from the fitted view"
    );

    // An unchanged frame on General leaves a user-moved camera alone.
    let moved = h.state().preview_camera();
    h.run();
    assert_eq!(
        h.state().preview_camera(),
        moved,
        "unchanged frames must not move a user-moved camera"
    );

    // Switch away and back: re-entry refits the camera.
    assert!(h.state_mut().try_select_tab(OptionsTab::Database));
    h.run();
    assert!(h.state_mut().try_select_tab(OptionsTab::General));
    h.run();
    assert_eq!(
        h.state().preview_camera(),
        fitted,
        "re-entering General must refit the camera"
    );
}

#[test]
fn tab_clicks_back_to_general_refit_the_preview_camera() {
    let dlg = OptionsDlg::new(&Settings::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );

    h.run();
    let fitted = h.state().preview_camera();
    assert!(fitted.0.is_some(), "the camera is fitted on entry");

    // Pan and zoom away, then leave and return to General through the tab
    // control's guarded clicks (the reviewer's scenario).
    h.state_mut().move_preview_camera(0.0, 0.0, 5.0);
    h.run();
    h.get_by_label("Database").click();
    h.run();
    h.get_by_label("General").click();
    h.run();
    assert_eq!(
        h.state().preview_camera(),
        fitted,
        "clicking back to General must refit the camera"
    );
}

#[test]
fn coverage_preview_is_local() {
    // No tile store: entering General and editing a bound must queue no view
    // requests (no coverage/grid job) and download nothing (Decision 2).
    let requests = Rc::new(RefCell::new(Vec::new()));
    let slot = requests.clone();
    let dlg = OptionsDlg::new(&Settings::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            move |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
                slot.borrow_mut().extend(dlg.take_view_requests());
            },
            dlg,
        );
    h.run(); // entry: General is the default tab
    h.state_mut().draft.area.max_lat = 61.0; // change a bound
    h.run();
    assert!(
        requests.borrow().is_empty(),
        "the local preview queued background work: {:?}",
        *requests.borrow()
    );
}

#[test]
fn stations_groups_match_designer() {
    let trace = trace_options(OptionsTab::Stations, egui::vec2(1366.0, 768.0), |_| {});
    for group in [
        "groupBox4",
        "groupBox5",
        "groupBox14",
        "groupBox16",
        "groupBox18",
        "groupBox44",
        "groupBox45",
        "groupBox46",
    ] {
        assert_subtree("OptionsDlg", group, None, &trace);
    }
    let reference = gui_ref::load("OptionsDlg");
    for name in [
        "tb_Options_MyCall",
        "tb_Options_MyLoc",
        "tb_Options_MyLat",
        "tb_Options_MyLon",
        "tb_Options_MyElevation",
        "tb_Options_DXCall",
        "tb_Options_DXLoc",
        "tb_Options_DXLat",
        "tb_Options_DXLon",
        "tb_Options_DXElevation",
        "ud_Options_Locator_MaxLength",
        "tb_Options_Band",
        "tb_Options_MyAntennaHeight",
        "tb_Options_MyAntennaGain",
        "tb_Options_MyPower",
        "tb_Options_DXAntennaHeight",
        "tb_Options_DXAntennaGain",
        "tb_Options_DXPower",
    ] {
        let control = reference.controls.iter().find(|c| c.name == name).unwrap();
        let field = trace.iter().find(|e| e.name == name).unwrap();
        let parent = trace.iter().find(|e| e.name == control.parent).unwrap();
        let [x, y] = control.location.unwrap();
        let [w, h] = control.size.unwrap();
        assert_eq!(
            field.rect.min - parent.rect.min,
            egui::vec2(x as f32, y as f32),
            "{name}"
        );
        assert!(
            field.rect.width() >= w as f32 && field.rect.height() >= h as f32,
            "{name}"
        );
    }
}

#[test]
fn plane_feeds_group_matches_designer() {
    for (size, ppp) in [
        (egui::vec2(1366.0, 768.0), 1.0),
        (egui::vec2(900.0, 600.0), 1.0),
        (egui::vec2(900.0, 600.0), 2.0),
    ] {
        let trace = trace_options_with_ppp(OptionsTab::Planes, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox6", None, &trace);
    }
}

#[test]
fn debug_group_matches_designer() {
    let reference = gui_ref::load("OptionsDlg");
    for (size, ppp) in [
        (egui::vec2(1366.0, 768.0), 1.0),
        (egui::vec2(900.0, 600.0), 1.0),
        (egui::vec2(900.0, 600.0), 2.0),
    ] {
        let trace = trace_options_with_ppp(OptionsTab::Planes, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox36", None, &trace);
        assert_subtree("OptionsDlg", "groupBox48", None, &trace);
        for name in [
            "groupBox36",
            "groupBox48",
            "cb_Options_TracePositions",
            "label33",
            "label22",
            "int32TextBox1",
            "cb_Options_Planes_ExtendedPlausibilityCheck_Enabled",
            "tb_Options_Planes_Interval",
            "cb_Options_Planes_LogErrors",
        ] {
            let control = reference.controls.iter().find(|c| c.name == name).unwrap();
            let field = trace
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("missing {name}"));
            let parent = trace
                .iter()
                .find(|e| e.name == control.parent && e.kind == Kind::Container)
                .unwrap();
            assert_eq!(field.path.last(), Some(&control.parent), "{name}");
            let [x, y] = control.location.unwrap();
            let [w, h] = control.size.unwrap();
            assert_eq!(
                field.rect.min - parent.rect.min,
                egui::vec2(x as f32, y as f32),
                "{name}"
            );
            assert!(
                field.rect.width() >= w as f32 && field.rect.height() >= h as f32,
                "{name}"
            );
        }
    }
}

#[test]
fn database_maintenance_matches_designer() {
    let reference = gui_ref::load("OptionsDlg");
    for (size, ppp) in [
        (egui::vec2(1366.0, 768.0), 1.0),
        (egui::vec2(900.0, 600.0), 1.0),
        (egui::vec2(900.0, 600.0), 2.0),
    ] {
        let trace = trace_options_with_ppp(OptionsTab::Planes, size, ppp, |_| {});
        for group in ["groupBox40", "groupBox26"] {
            assert_subtree("OptionsDlg", group, None, &trace);
            for control in reference
                .controls
                .iter()
                .filter(|c| c.name == group || c.parent == group)
            {
                let rect = rect_of(&trace, &control.name);
                let [x, y] = control.location.unwrap();
                let [w, h] = control.size.unwrap();
                let parent = trace
                    .iter()
                    .find(|e| e.name == control.parent && e.kind == Kind::Container)
                    .unwrap_or_else(|| panic!("{} container not in trace", control.parent))
                    .rect;
                assert_eq!(
                    rect.min - parent.min,
                    egui::vec2(x as f32, y as f32),
                    "{}",
                    control.name
                );
                assert!(
                    rect.width() >= w as f32 && rect.height() >= h as f32,
                    "{}",
                    control.name
                );
            }
        }
    }
}

/// The three render settings the Designer-rect checks run at.
const RENDER_SETTINGS: [(egui::Vec2, f32); 3] = [
    (egui::vec2(1366.0, 768.0), 1.0),
    (egui::vec2(900.0, 600.0), 1.0),
    (egui::vec2(900.0, 600.0), 2.0),
];

/// Each of `names` is drawn once, inside its Designer parent, at its Designer location and at
/// least at its Designer size.
fn assert_designer_rects(trace: &[Entry], names: &[&str]) {
    let reference = gui_ref::load("OptionsDlg");
    for name in names {
        let control = reference
            .controls
            .iter()
            .find(|c| c.name == *name)
            .unwrap_or_else(|| panic!("{name} not in the reference"));
        let entries: Vec<&Entry> = trace.iter().filter(|e| e.name == *name).collect();
        assert_eq!(entries.len(), 1, "{name} drawn {} times", entries.len());
        let field = entries[0];
        assert_eq!(field.path.last(), Some(&control.parent), "{name}");
        let parent = trace
            .iter()
            .find(|e| e.name == control.parent && e.kind == Kind::Container)
            .unwrap_or_else(|| panic!("{} container not in trace", control.parent))
            .rect;
        let [x, y] = control.location.unwrap();
        let [w, h] = control.size.unwrap();
        assert_eq!(
            field.rect.min - parent.min,
            egui::vec2(x as f32, y as f32),
            "{name}"
        );
        assert!(
            field.rect.width() >= w as f32 && field.rect.height() >= h as f32,
            "{name} {:?} smaller than {w}x{h}",
            field.rect
        );
    }
}

/// A harness on the real dialog, opened on `tab`.
fn dialog_harness(tab: OptionsTab, settings: &Settings) -> Harness<'static, OptionsDlg> {
    let mut dlg = OptionsDlg::new(settings);
    dlg.tab = tab;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1366.0, 768.0))
        .build_ui_state(
            |ui, dlg: &mut OptionsDlg| {
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        );
    h.run();
    h
}

#[test]
fn map_overlay_toggles_match_designer() {
    for (size, ppp) in RENDER_SETTINGS {
        let trace = trace_options_with_ppp(OptionsTab::Map, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox49", None, &trace);
        assert_subtree("OptionsDlg", "groupBox22", None, &trace);
        assert_designer_rects(
            &trace,
            &[
                "groupBox23",
                "cb_Options_Airports_Activate",
                "cb_Options_Locators_Activate",
                "cb_Options_Distances_Activated",
                "cb_Options_Watchlist_Activate",
                "cb_Options_Beamwidths_Activate",
                "groupBox49",
                "cb_Options_Map_SmallMarkers",
                "cb_Options_Map_LabelCalls",
                "groupBox22",
                "cb_Options_Gauges_Show",
                "label188",
                "ud_Options_Gauges_Width",
                "label198",
                "cb_Options_Gauges_ForeColor",
            ],
        );
        // Decision 10: the empty placeholder group is not drawn.
        assert!(!trace.iter().any(|e| e.name == "groupBox39"));
        for (name, text) in [
            ("groupBox23", "General"),
            ("cb_Options_Airports_Activate", "Airports"),
            ("cb_Options_Locators_Activate", "Locator Grid"),
            ("cb_Options_Distances_Activated", "Distance Circles"),
            ("cb_Options_Watchlist_Activate", "Watchlist Locations"),
            ("cb_Options_Beamwidths_Activate", "Beamwidths"),
            ("groupBox49", "Multi-Path"),
            (
                "cb_Options_Map_SmallMarkers",
                "Use Small Markers for all Path Marks",
            ),
            (
                "cb_Options_Map_LabelCalls",
                "Show Labels with Callsign at Locations",
            ),
            ("groupBox22", "Tracking Gauges"),
            (
                "cb_Options_Gauges_Show",
                "Show large Tracking Display on map",
            ),
            ("label188", "Azimuth/Elevation Gauge Width:"),
            ("label198", "Gauge ForeColor:"),
        ] {
            let entry = trace.iter().find(|e| e.name == name).unwrap();
            assert_eq!(entry.text.trim(), text, "{name}");
        }
    }

    // The drawn widgets stay inside their groups, and the controls of the left column end
    // before the Designer groups.
    for (size, ppp) in RENDER_SETTINGS {
        let mut dlg = OptionsDlg::new(&Settings::default());
        dlg.tab = OptionsTab::Map;
        let mut h = Harness::builder()
            .with_size(size)
            .with_pixels_per_point(ppp)
            .build_ui_state(
                |ui, dlg: &mut OptionsDlg| {
                    trace::enable(ui.ctx());
                    let _ = dlg.show(ui.ctx());
                },
                dlg,
            );
        let trace = settled_trace(&mut h);
        for (group, labels) in [
            (
                "groupBox23",
                &[
                    "Airports",
                    "Locator Grid",
                    "Distance Circles",
                    "Watchlist Locations",
                    "Beamwidths",
                ][..],
            ),
            (
                "groupBox22",
                &[
                    "Show large Tracking Display on map",
                    "Azimuth/Elevation Gauge Width:",
                    "Gauge ForeColor:",
                ][..],
            ),
            (
                "groupBox49",
                &[
                    "Use Small Markers for all Path Marks",
                    "Show Labels with Callsign at Locations",
                ][..],
            ),
        ] {
            let g = rect_of(&trace, group);
            for label in labels {
                let r = h.get_by_label(label).rect();
                assert!(
                    g.expand(0.5).contains_rect(r),
                    "{label} {r:?} outside {group} {g:?} at {size:?} ppp {ppp}"
                );
            }
        }
        let text_of = |label: &str| {
            h.get_by_role_and_label(egui::accesskit::Role::Label, label)
                .rect()
        };
        let label188 = text_of("Azimuth/Elevation Gauge Width:");
        assert!(label188.right() <= rect_of(&trace, "ud_Options_Gauges_Width").left());
        let label198 = text_of("Gauge ForeColor:");
        let gauges = rect_of(&trace, "groupBox22").expand(0.5);
        assert!(gauges.contains_rect(label188) && gauges.contains_rect(label198));
        assert!(label198.right() <= rect_of(&trace, "cb_Options_Gauges_ForeColor").left());
        let right_column = rect_of(&trace, "groupBox23").left();
        for label in ["Enable Map Preloading", "Thunderforest API key", "show"] {
            let r = h.get_by_label(label).rect();
            assert!(
                r.right() < right_column,
                "{label} {r:?} reaches x {right_column}"
            );
        }
    }

    // The free-standing Map check boxes and the Track tab's gauge check box are gone.
    let h = dialog_harness(OptionsTab::Map, &Settings::default());
    assert!(h.query_by_label("Show beam width").is_none());
    assert!(h.query_by_label("Label calls").is_none());
    let h = dialog_harness(OptionsTab::Track, &Settings::default());
    assert!(h
        .query_by_label("Show tracking values on the map")
        .is_none());
    assert!(h
        .query_by_label("Show large Tracking Display on map")
        .is_none());
}

/// Type `text` into the gauge width field and confirm it with Enter.
fn type_gauge_width(h: &mut Harness<'static, OptionsDlg>, text: &str) {
    use egui::Key;
    h.get_by_role_and_label(
        egui::accesskit::Role::SpinButton,
        "Azimuth/Elevation Gauge Width:",
    )
    .click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run();
    h.get_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.accesskit_node().is_focused())
        .expect("the gauge width field has focus")
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

#[test]
fn gauge_controls_match_designer() {
    use airscout::ui::map_track::{gauge_color, KNOWN_COLORS};
    use egui::accesskit::Role;

    let mut h = dialog_harness(OptionsTab::Map, &Settings::default());
    let width = h.get_by_role_and_label(Role::SpinButton, "Azimuth/Elevation Gauge Width:");
    let node = width.accesskit_node();
    assert_eq!(node.numeric_value(), Some(175.0));
    assert_eq!(node.min_numeric_value(), Some(175.0));
    assert_eq!(node.max_numeric_value(), Some(1000.0));
    for (typed, stored) in [("5000", 1000), ("10", 175), ("300.6", 301), ("420", 420)] {
        type_gauge_width(&mut h, typed);
        assert_eq!(h.state().draft.gui.gauge_width, stored, "typed {typed}");
    }

    // A stored width outside the range is shown as is, not rewritten.
    let mut s = Settings::default();
    s.gui.gauge_width = 2000;
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    assert_eq!(h.state().draft.gui.gauge_width, 2000);

    // `cb_Options_Gauges_ForeColor`: a drop-down list of every `KnownColor`, shown as
    // `Color.ToString()`, storing `ToKnownColor().ToString()`.
    assert_eq!(KNOWN_COLORS.len(), 174);
    assert_eq!(KNOWN_COLORS[0].0, "ActiveBorder");
    assert_eq!(KNOWN_COLORS[173].0, "MenuHighlight");
    let mut h = dialog_harness(OptionsTab::Map, &Settings::default());
    let combo = h.get_by_role_and_label(Role::ComboBox, "Gauge ForeColor:");
    assert_eq!(combo.value().as_deref(), Some("Color [Black]"));
    combo.click();
    h.run();
    h.get_by_label("Color [ActiveCaption]").click();
    h.run();
    assert_eq!(h.state().draft.gui.gauge_forecolor, "ActiveCaption");

    // An unknown stored name is kept and painted black (the C# fallback).
    let mut s = Settings::default();
    s.gui.gauge_forecolor = "NoSuchColor".into();
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    assert_eq!(h.state().draft.gui.gauge_forecolor, "NoSuchColor");
    assert_eq!(gauge_color("NoSuchColor"), egui::Color32::BLACK);
    assert_eq!(gauge_color("Red"), egui::Color32::from_rgb(255, 0, 0));
    assert_eq!(
        gauge_color("ActiveCaption"),
        egui::Color32::from_rgb(153, 180, 209)
    );
    assert_eq!(
        gauge_color("Transparent"),
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 0)
    );

    // The gauge painter follows the width (`gm_Main_SizeChanged`, `MapDlg.cs:5664-5694`).
    use airscout::ui::map_track::gauge_rects;
    let map = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let (frame, rig) = gauge_rects(map, 175);
    assert_eq!(
        frame,
        egui::Rect::from_min_size(egui::pos2(610.0, 505.0), egui::vec2(370.0, 175.0))
    );
    assert_eq!(
        rig,
        egui::Rect::from_min_size(egui::pos2(610.0, 408.0), egui::vec2(370.0, 87.0))
    );
    assert_eq!(gauge_rects(map, 100), (frame, rig), "at least 175");
    assert_eq!(gauge_rects(map, 300).0.width(), 620.0);
    assert_eq!(
        gauge_rects(map, 1000).0.width(),
        1020.0,
        "at most half the map"
    );
}

/// The `groupBox58` children and the `groupBox23` toggle of the user-supplied overlay.
const OVERLAY_CONTROLS: [&str; 15] = [
    "groupBox58",
    "label209",
    "tb_Options_UserSupplied_Filename",
    "btn_Options_UserSupplied_Filename",
    "label210",
    "pc_Options_UserSupplied_Opacity",
    "label200",
    "tb_Options_UserSupplied_MinLat",
    "label208",
    "cb_Options_UserSupplied_MinLon",
    "label34",
    "cb_Options_UserSupplied_MaxLat",
    "label204",
    "cb_Options_UserSupplied_MaxLon",
    "cb_Options_ShowUserSuppliedOverlay",
];

/// Type `text` into the spin button labelled `label` and confirm it with Enter.
fn type_spin(h: &mut Harness<'static, OptionsDlg>, label: &str, text: &str) {
    use egui::Key;
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label)
        .click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run();
    h.get_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.accesskit_node().is_focused())
        .unwrap_or_else(|| panic!("{label} has focus"))
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

#[test]
fn overlay_group_matches_designer() {
    use egui::accesskit::Role;

    for (size, ppp) in RENDER_SETTINGS {
        let trace = trace_options_with_ppp(OptionsTab::Map, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox58", None, &trace);
        assert_subtree("OptionsDlg", "groupBox23", None, &trace);
        assert_designer_rects(&trace, &OVERLAY_CONTROLS);
        for (name, text) in [
            ("groupBox58", "User supplied Overlay"),
            ("label209", "Filename:"),
            ("btn_Options_UserSupplied_Filename", "Select"),
            ("label210", "Opacity:"),
            ("label200", "Min. Lat:"),
            ("label208", "Min. Lon:"),
            ("label34", "Max. Lat:"),
            ("label204", "Max. Lon:"),
            ("cb_Options_ShowUserSuppliedOverlay", "User Supplied"),
        ] {
            let entry = trace.iter().find(|e| e.name == name).unwrap();
            assert_eq!(entry.text.trim(), text, "{name}");
        }
    }

    // The drawn widgets stay inside the group, each label ends before its field, and the
    // Select button ends inside the group.
    for (size, ppp) in RENDER_SETTINGS {
        let mut dlg = OptionsDlg::new(&Settings::default());
        dlg.tab = OptionsTab::Map;
        let mut h = Harness::builder()
            .with_size(size)
            .with_pixels_per_point(ppp)
            .build_ui_state(
                |ui, dlg: &mut OptionsDlg| {
                    trace::enable(ui.ctx());
                    let _ = dlg.show(ui.ctx());
                },
                dlg,
            );
        let trace = settled_trace(&mut h);
        let group = rect_of(&trace, "groupBox58").expand(0.5);
        for (label, field) in [
            ("Filename:", "tb_Options_UserSupplied_Filename"),
            ("Opacity:", "pc_Options_UserSupplied_Opacity"),
            ("Min. Lat:", "tb_Options_UserSupplied_MinLat"),
            ("Min. Lon:", "cb_Options_UserSupplied_MinLon"),
            ("Max. Lat:", "cb_Options_UserSupplied_MaxLat"),
            ("Max. Lon:", "cb_Options_UserSupplied_MaxLon"),
        ] {
            let r = h.get_by_role_and_label(Role::Label, label).rect();
            assert!(
                group.contains_rect(r),
                "{label} {r:?} outside groupBox58 {group:?} at {size:?} ppp {ppp}"
            );
            assert!(
                r.right() <= rect_of(&trace, field).left(),
                "{label} {r:?} reaches {field} at {size:?} ppp {ppp}"
            );
        }
        let select = h.get_by_role_and_label(Role::Button, "Select").rect();
        assert!(
            group.contains_rect(select),
            "Select {select:?} at ppp {ppp}"
        );
        let toggle = h.get_by_label("User Supplied").rect();
        let general = rect_of(&trace, "groupBox23").expand(0.5);
        assert!(general.contains_rect(toggle), "User Supplied {toggle:?}");
    }

    // The fields show the C# defaults.
    let mut h = dialog_harness(OptionsTab::Map, &Settings::default());
    for (label, value, min, max) in [
        ("Min. Lat:", 49.0, -90.0, 90.0),
        ("Max. Lat:", 59.0, -90.0, 90.0),
        ("Min. Lon:", -14.5, -180.0, 180.0),
        ("Max. Lon:", 6.0, -180.0, 180.0),
        ("Opacity:", 0.5, 0.0, 1.0),
    ] {
        let node = h.get_by_role_and_label(Role::SpinButton, label);
        let node = node.accesskit_node();
        assert_eq!(node.numeric_value(), Some(value), "{label}");
        assert_eq!(node.min_numeric_value(), Some(min), "{label}");
        assert_eq!(node.max_numeric_value(), Some(max), "{label}");
    }
    // `PercentageControl` shows the value as a whole percentage.
    assert_eq!(
        h.get_by_role_and_label(Role::SpinButton, "Opacity:")
            .value()
            .as_deref(),
        Some("50%")
    );
    let filename = h.get_by_role_and_label(Role::TextInput, "Filename:");
    assert_eq!(filename.value().as_deref(), Some(""));
    assert!(!h.state().draft.gui.overlay_enabled);
    assert_eq!(h.state().draft.gui.overlay_filename, None);

    // Edits reach the draft: a percentage, a fraction, and the clamped bounds.
    for (typed, stored) in [("30%", 0.3), ("0.7", 0.7), ("250%", 1.0), ("-5", 0.0)] {
        type_spin(&mut h, "Opacity:", typed);
        let got = h.state().draft.gui.overlay_opacity;
        assert!((got - stored).abs() < 1e-6, "typed {typed}: {got}");
    }
    for (label, typed, stored) in [
        ("Min. Lat:", "47.25", 47.25),
        ("Max. Lat:", "95", 90.0),
        ("Min. Lon:", "-200", -180.0),
        ("Max. Lon:", "12.5", 12.5),
    ] {
        type_spin(&mut h, label, typed);
        let g = &h.state().draft.gui;
        let got = match label {
            "Min. Lat:" => g.overlay_min_lat,
            "Max. Lat:" => g.overlay_max_lat,
            "Min. Lon:" => g.overlay_min_lon,
            _ => g.overlay_max_lon,
        };
        assert_eq!(got, stored, "{label} typed {typed}");
    }
    h.get_by_label("User Supplied").click();
    h.run();
    assert!(h.state().draft.gui.overlay_enabled);
    h.get_by_role_and_label(Role::TextInput, "Filename:")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Filename:")
        .type_text("overlay.png");
    h.run();
    assert_eq!(
        h.state().draft.gui.overlay_filename.as_deref(),
        Some(std::path::Path::new("overlay.png"))
    );

    // Stored values outside the field ranges are shown as they are, not rewritten.
    let mut s = Settings::default();
    s.gui.overlay_min_lat = -120.0;
    s.gui.overlay_max_lon = 200.0;
    s.gui.overlay_opacity = 1.5;
    s.gui.overlay_filename = Some("/data/TE1ST overlay.png".into());
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    let g = &h.state().draft.gui;
    assert_eq!(
        (g.overlay_min_lat, g.overlay_max_lon, g.overlay_opacity),
        (-120.0, 200.0, 1.5)
    );
    assert_eq!(g.overlay_filename, s.gui.overlay_filename);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Filename:")
            .value()
            .as_deref(),
        Some("/data/TE1ST overlay.png")
    );
}

/// The four Phase 3A pages, each traced in its own settled frame at `size` / `ppp`, merged
/// into one trace (frame plus the General, Stations, Planes and Map bodies).
fn trace_options_merged(size: egui::Vec2, ppp: f32) -> Vec<Entry> {
    merge_traces(
        [
            OptionsTab::General,
            OptionsTab::Stations,
            OptionsTab::Planes,
            OptionsTab::Map,
        ]
        .map(|tab| trace_options_with_ppp(tab, size, ppp, |_| {})),
    )
}

/// The `groupBox7` controls drawn at their Designer rects (`cb_Options_InfoWin_Speed` is
/// checked separately: it moves into `groupBox33`).
const INFO_WINDOW_CONTROLS: [&str; 27] = [
    "groupBox7",
    "label72",
    "rb_Options_InfoWin_Metric",
    "rb_Options_InfoWin_Imperial",
    "label62",
    "tb_Options_Map_ToolTipFont",
    "groupBox29",
    "cb_Options_InfoWin_Position",
    "cb_Options_InfoWin_Alt",
    "cb_Options_InfoWin_Track",
    "cb_Options_InfoWin_Type",
    "label74",
    "groupBox33",
    "cb_Options_InfoWin_Dist",
    "cb_Options_InfoWin_Time",
    "cb_Options_InfoWin_Angle",
    "cb_Options_InfoWin_Epsilon",
    "cb_Options_InfoWin_Squint",
    "label75",
    "groupBox34",
    "cb_Options_InfoWin_MyAzimuth",
    "cb_Options_InfoWin_MyElevation",
    "cb_Options_InfoWin_MyDoppler",
    "cb_Options_InfoWin_DXAzimuth",
    "cb_Options_InfoWin_DXElevation",
    "cb_Options_InfoWin_DXDoppler",
    "label76",
];

#[test]
fn info_window_matches_designer() {
    use egui::accesskit::Role;

    for (size, ppp) in RENDER_SETTINGS {
        let trace = trace_options_with_ppp(OptionsTab::Map, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox7", None, &trace);
        assert_designer_rects(&trace, &INFO_WINDOW_CONTROLS);
        // The visibility captions sit inside their column box, at its bottom.
        for (name, column, text) in [
            ("label74", "groupBox29", "Always visible"),
            (
                "label75",
                "groupBox33",
                "Visible only in detailed window when crossing",
            ),
            (
                "label76",
                "groupBox34",
                "Visible only in detailed window and when tracking",
            ),
        ] {
            let entry = trace.iter().find(|e| e.name == name).unwrap();
            assert_eq!(entry.text, text, "{name}");
            assert_eq!(entry.path.last().map(String::as_str), Some(column));
            let column = rect_of(&trace, column);
            assert_eq!(entry.rect.min.y - column.min.y, 136.0, "{name}");
        }
        // `cb_Options_InfoWin_Speed` lies outside `groupBox7`'s Designer rect; it moves into
        // the detailed-mode column, below Squint.
        let speed = trace
            .iter()
            .find(|e| e.name == "cb_Options_InfoWin_Speed")
            .expect("Plane Speed in trace");
        assert_eq!(speed.text, "Plane Speed");
        assert_eq!(speed.path.last().map(String::as_str), Some("groupBox33"));
        let column = trace
            .iter()
            .find(|e| e.name == "groupBox33" && e.kind == Kind::Container)
            .unwrap()
            .rect;
        assert_eq!(speed.rect.min - column.min, egui::vec2(11.0, 115.0));
        assert!(
            column.contains_rect(speed.rect),
            "{:?} {column:?}",
            speed.rect
        );
        // The font row: the caption and the read-only field; no font picker (Decision 9).
        assert_eq!(rect_text(&trace, "label62"), "Font:");
        assert!(!trace.iter().any(|e| e.name == "btn_Options_SelectFont"));
        // Every Info Window widget stays inside groupBox7 and its column box.
        let mut dlg = OptionsDlg::new(&Settings::default());
        dlg.tab = OptionsTab::Map;
        let mut h = Harness::builder()
            .with_size(size)
            .with_pixels_per_point(ppp)
            .build_ui_state(
                |ui, dlg: &mut OptionsDlg| {
                    trace::enable(ui.ctx());
                    let _ = dlg.show(ui.ctx());
                },
                dlg,
            );
        let trace = settled_trace(&mut h);
        for (column, labels) in [
            (
                "groupBox29",
                &[
                    "Plane Position",
                    "Plane Altitude",
                    "Plane Track",
                    "Plane Type",
                    "Always visible",
                ][..],
            ),
            (
                "groupBox33",
                &[
                    "Crossing Distance",
                    "Crossing Time",
                    "Crossing Angle",
                    "Elevation Angles",
                    "Squint Angle",
                    "Plane Speed",
                    "Visible only in detailed window when crossing",
                ][..],
            ),
            (
                "groupBox34",
                &[
                    "My Azimuth",
                    "My Elevation",
                    "My Doppler",
                    "DX Azimuth",
                    "DX Elevation",
                    "DX Doppler",
                    "Visible only in detailed window and when tracking",
                ][..],
            ),
        ] {
            let g = rect_of(&trace, column).expand(0.5);
            for label in labels {
                let r = h.get_by_label(label).rect();
                assert!(
                    g.contains_rect(r),
                    "{label} {r:?} outside {column} {g:?} at {size:?} ppp {ppp}"
                );
            }
        }
        let info = rect_of(&trace, "groupBox7").expand(0.5);
        for label in ["Units:", "Metric", "Imperial", "Font:"] {
            let r = h.get_by_label(label).rect();
            assert!(info.contains_rect(r), "{label} {r:?} at ppp {ppp}");
        }
    }

    // Units are mutually-exclusive radio buttons; the font field is read-only.
    let mut h = dialog_harness(OptionsTab::Map, &Settings::default());
    assert_eq!(
        h.get_by_role_and_label(Role::RadioButton, "Metric")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    h.get_by_role_and_label(Role::RadioButton, "Imperial")
        .click();
    h.run();
    let w = &h.state().draft.gui.info_win;
    assert!(w.imperial && !w.metric);
    h.get_by_role_and_label(Role::RadioButton, "Metric").click();
    h.run();
    let w = &h.state().draft.gui.info_win;
    assert!(w.metric && !w.imperial);
    let font = h.get_by_role_and_label(Role::TextInput, "Font:");
    assert_eq!(font.value().as_deref(), Some("egui default"));
    font.click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Font:")
        .type_text("Arial");
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Font:")
            .value()
            .as_deref(),
        Some("egui default")
    );
    // The check boxes edit their InfoWin fields.
    h.get_by_label("Plane Speed").click();
    h.run();
    assert!(h.state().draft.gui.info_win.speed);
    h.get_by_label("DX Doppler").click();
    h.run();
    assert!(h.state().draft.gui.info_win.dx_doppler);

    // Stored units with both (or neither) flag set are shown, not rewritten.
    let mut s = Settings::default();
    s.gui.info_win.metric = true;
    s.gui.info_win.imperial = true;
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    assert_eq!(h.state().draft.gui.info_win, s.gui.info_win);

    // The Info Window controls are gone from the Planes tab.
    let trace = trace_options(OptionsTab::Planes, egui::vec2(1366.0, 768.0), |_| {});
    assert!(!trace
        .iter()
        .any(|e| e.name.starts_with("cb_Options_InfoWin") || e.name.starts_with("rb_Options")));
    let h = dialog_harness(OptionsTab::Planes, &Settings::default());
    assert!(h.query_by_label("Plane info window").is_none());
    assert!(h.query_by_label("My azimuth").is_none());
}

/// The recorded text of `name`.
fn rect_text(t: &[Entry], name: &str) -> String {
    t.iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} not in trace"))
        .text
        .clone()
}

#[test]
fn whole_options_phase_a_matches_reference() {
    for (size, ppp) in RENDER_SETTINGS {
        let trace = trace_options_merged(size, ppp);
        assert_options_phase_a(&trace, true);
    }
}

#[test]
fn allowlist_only_deviations() {
    let allow = load_allow("OptionsDlg");
    let deviations = load_deviations();
    assert!(!allow.is_empty());
    for a in &allow {
        let id = a.deviation.as_str();
        assert!(
            id.len() == 5 && id.starts_with("D-") && id[2..].bytes().all(|b| b.is_ascii_digit()),
            "{} cites {id:?}, not a D-NNN id",
            a.name
        );
        assert!(
            deviations.lines().any(|l| l
                .strip_prefix("## ")
                .is_some_and(|h| h.split_whitespace().next() == Some(id))),
            "{} cites {id}, which has no heading in docs/gui-deviations.md",
            a.name
        );
        assert!(
            ["missing", "extra", "text", "moved", "geometry"].contains(&a.what.as_str()),
            "{} has unknown kind {:?}",
            a.name,
            a.what
        );
        assert!(!a.reason.trim().is_empty(), "{} has no reason", a.name);
    }
}

#[test]
fn map_icon_and_position_filter_groups_match_designer() {
    use egui::accesskit::Role;

    for (size, ppp) in RENDER_SETTINGS {
        let trace = trace_options_with_ppp(OptionsTab::Map, size, ppp, |_| {});
        for group in ["groupBox2", "groupBox30"] {
            assert_subtree("OptionsDlg", group, None, &trace);
        }
        assert_designer_rects(
            &trace,
            &[
                "groupBox2",
                "label61",
                "cb_Options_Map_Provider",
                "label148",
                "ud_Options_Map_Opacity",
                "cb_Options_Map_Preloader_Enabled",
                "label144",
                "ud_Options_Map_Preloader_MaxZoom",
                "label29",
                "tb_Options_Map_Update_Interval",
                "label97",
                "groupBox30",
                "label80",
                "tb_Options_Planes_IconSize_L",
                "label83",
                "tb_Options_Planes_IconSize_M",
                "label82",
                "tb_Options_Planes_IconSize_H",
                "label81",
                "tb_Options_Planes_IconSize_S",
                "label84",
            ],
        );
        let trace = trace_options_with_ppp(OptionsTab::Planes, size, ppp, |_| {});
        assert_subtree("OptionsDlg", "groupBox38", None, &trace);
        assert_designer_rects(
            &trace,
            &[
                "groupBox38",
                "label5",
                "tb_Options_Planes_MinAlt",
                "label9",
                "label7",
                "tb_Options_Planes_MaxAlt",
                "label8",
                "label1",
                "tb_Options_Planes_Positions_TTL",
                "label2",
            ],
        );

        // The drawn texts stay inside their groups and clear of the fields below/right.
        for (tab, group, labels) in [
            (
                OptionsTab::Map,
                "groupBox2",
                &[
                    "Map Opacity:",
                    "Enable Map Preloading",
                    "Update screen every ",
                    "second(s).",
                ][..],
            ),
            (
                OptionsTab::Map,
                "groupBox30",
                &[
                    "Light:",
                    "Medium:",
                    "Heavy:",
                    "Super:",
                    "Restart of AirScout required!",
                ][..],
            ),
            (
                OptionsTab::Planes,
                "groupBox38",
                &[
                    "Minimal Altitude:",
                    "Maximal Altitude:",
                    "Discard plane when last position report is older than:",
                    "mins.",
                ][..],
            ),
        ] {
            let mut dlg = OptionsDlg::new(&Settings::default());
            dlg.tab = tab;
            let mut h = Harness::builder()
                .with_size(size)
                .with_pixels_per_point(ppp)
                .build_ui_state(
                    |ui, dlg: &mut OptionsDlg| {
                        trace::enable(ui.ctx());
                        let _ = dlg.show(ui.ctx());
                    },
                    dlg,
                );
            let trace = settled_trace(&mut h);
            let g = rect_of(&trace, group).expand(0.5);
            for label in labels {
                let r = h.get_by_label(label).rect();
                assert!(
                    g.contains_rect(r),
                    "{label} {r:?} outside {group} {g:?} at {size:?} ppp {ppp}"
                );
            }
            if group == "groupBox2" {
                let caption = h
                    .get_by(|n| {
                        n.role() == Role::Label
                            && [n.label(), n.value()]
                                .into_iter()
                                .flatten()
                                .any(|t| t.starts_with("You can select different map provider"))
                    })
                    .rect();
                let combo = rect_of(&trace, "cb_Options_Map_Provider");
                assert!(
                    caption.bottom() <= combo.top() && g.contains_rect(caption),
                    "label61 {caption:?} reaches the provider list {combo:?} at ppp {ppp}"
                );
            }
        }
    }

    // The fields edit their settings; stored values outside a field's range are kept.
    let mut s = Settings::default();
    s.gui.planes_icon_size = [2, 32, 48, 300];
    s.planes_max_alt_m = 150_000;
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    assert_eq!(h.state().draft.gui.planes_icon_size, [2, 32, 48, 300]);
    type_spin(&mut h, "Light:", "20");
    assert_eq!(h.state().draft.gui.planes_icon_size[0], 20);
    type_spin(&mut h, "Update screen every ", "7");
    assert_eq!(h.state().draft.gui.map_update_s, 7);
    type_spin(&mut h, "Map Opacity:", "40%");
    assert!((h.state().draft.gui.map_opacity - 0.4).abs() < 1e-6);
    let mut h = dialog_harness(OptionsTab::Planes, &s);
    h.run();
    assert_eq!(h.state().draft.planes_max_alt_m, 150_000);
    type_spin(&mut h, "Minimal Altitude:", "1500");
    assert_eq!(h.state().draft.planes_min_alt_m, 1500);
    type_spin(
        &mut h,
        "Discard plane when last position report is older than:",
        "9",
    );
    assert_eq!(h.state().draft.planes_position_ttl_min, 9);
}

/// A harness on the real dialog with the frame trace on, opened on `tab` at `size` and `ppp`.
/// The body text is drawn at `body` points.
fn traced_harness(
    tab: OptionsTab,
    settings: &Settings,
    size: egui::Vec2,
    ppp: f32,
    body: f32,
) -> Harness<'static, OptionsDlg> {
    let mut dlg = OptionsDlg::new(settings);
    dlg.tab = tab;
    Harness::builder()
        .with_size(size)
        .with_pixels_per_point(ppp)
        .build_ui_state(
            move |ui, dlg: &mut OptionsDlg| {
                trace::enable(ui.ctx());
                ui.ctx().global_style_mut(|style| {
                    style
                        .text_styles
                        .insert(egui::TextStyle::Body, egui::FontId::proportional(body));
                });
                let _ = dlg.show(ui.ctx());
            },
            dlg,
        )
}

#[test]
fn map_labels_end_before_the_next_control() {
    use egui::accesskit::Role;

    // The egui default body font (14 pt), and a large one that does not fit the Designer rects:
    // the labels then fall back to the small font instead of running into the next control.
    let large = [(egui::vec2(1366.0, 768.0), 1.0, 20.0)];
    for (size, ppp, body) in RENDER_SETTINGS
        .map(|(size, ppp)| (size, ppp, 14.0))
        .into_iter()
        .chain(large)
    {
        let mut h = traced_harness(OptionsTab::Map, &Settings::default(), size, ppp, body);
        let trace = settled_trace(&mut h);
        for (text, next) in [
            ("Thunderforest API key", "tb_Options_Map_ThunderforestKey"),
            ("second(s).", "label144"),
        ] {
            let label = h.get_by_role_and_label(Role::Label, text).rect();
            let next_rect = rect_of(&trace, next);
            assert!(
                label.right() <= next_rect.left(),
                "{text} {label:?} reaches {next} {next_rect:?} at {size:?} ppp {ppp} body {body}"
            );
        }
    }
}

/// Every numeric field ported in Phase 3A uses the C# Designer range
/// (`OptionsDlg.Designer.cs`): an edit is clamped to it and the C# minimum is accepted.
#[test]
fn options_number_fields_use_designer_ranges() {
    use airscout::ui::options::validate;

    let s = Settings::default();
    let mut h = dialog_harness(OptionsTab::Planes, &s);
    for (label, text, read, want) in [
        // tb_Options_Planes_MinAlt / MaxAlt: 0..20000 (:7431-7434, :7417-7420).
        (
            "Minimal Altitude:",
            "25000",
            (|d: &Settings| f64::from(d.planes_min_alt_m)) as fn(&Settings) -> f64,
            20_000.0,
        ),
        (
            "Maximal Altitude:",
            "25000",
            |d: &Settings| f64::from(d.planes_max_alt_m),
            20_000.0,
        ),
        // tb_Options_Planes_Positions_TTL: 0..30 (:7403-7406).
        (
            "Discard plane when last position report is older than:",
            "0",
            |d| d.planes_position_ttl_min as f64,
            0.0,
        ),
        (
            "Discard plane when last position report is older than:",
            "45",
            |d| d.planes_position_ttl_min as f64,
            30.0,
        ),
        // tb_Options_Planes_MaxDelta: 0..15000 (:1194-1198).
        (
            "Max. el. delta path to plane",
            "20000",
            |d| d.planes_max_delta_m,
            15_000.0,
        ),
    ] {
        type_spin(&mut h, label, text);
        assert_eq!(read(&h.state().draft), want, "{label} typed {text}");
    }

    let mut h = dialog_harness(OptionsTab::Map, &s);
    for (label, text, index, want) in [
        // tb_Options_Planes_IconSize_*: 0..128 (:6732-6777).
        ("Light:", "200", 0, 128),
        ("Medium:", "0", 1, 0),
        ("Heavy:", "100", 2, 100),
        ("Super:", "129", 3, 128),
    ] {
        type_spin(&mut h, label, text);
        assert_eq!(
            h.state().draft.gui.planes_icon_size[index],
            want,
            "{label} typed {text}"
        );
    }
    // tb_Options_Map_Update_Interval: 0..3600 (:7059-7062).
    type_spin(&mut h, "Update screen every ", "0");
    assert_eq!(h.state().draft.gui.map_update_s, 0);
    type_spin(&mut h, "Update screen every ", "4000");
    assert_eq!(h.state().draft.gui.map_update_s, 3600);

    // The validation accepts the C# minimums.
    let mut d = Settings::default();
    d.gui.map_update_s = 0;
    d.planes_position_ttl_min = 0;
    d.gui.planes_icon_size = [0; 4];
    assert_eq!(validate(&d), Vec::<String>::new());
}

/// An `Int32TextBox` takes digits only, so its field stores whole numbers, also when the setting
/// is an `f64`: a typed 12.6 is rounded.
#[test]
fn int32_fields_store_whole_numbers() {
    let s = Settings::default();
    let mut h = dialog_harness(OptionsTab::Planes, &s);
    type_spin(&mut h, "Minimal Altitude:", "12.6");
    assert_eq!(h.state().draft.planes_min_alt_m, 13);
    type_spin(&mut h, "Max. el. delta path to plane", "12.6");
    assert_eq!(h.state().draft.planes_max_delta_m, 13.0);
    type_spin(
        &mut h,
        "Maximal Path Midpoint Circumcircle\n(Filter AUTO = 0)\n(Filter OFF = -1):",
        "12.6",
    );
    assert_eq!(h.state().draft.gui.planes_filter_max_circumcircle_km, 13.0);
    let mut h = dialog_harness(OptionsTab::Map, &s);
    type_spin(&mut h, "Heavy:", "12.6");
    assert_eq!(h.state().draft.gui.planes_icon_size[2], 13);
}

#[test]
fn out_of_range_stored_coverage_and_heights_are_not_rewritten() {
    // `tb_Coverage_*` (±90/±180) and the antenna heights (0..10000) clamp only what the user
    // types; showing the page leaves a stored value outside the range as it is.
    let mut s = Settings::default();
    s.area.max_lat = 95.0;
    s.area.min_lon = -200.0;
    let mut h = dialog_harness(OptionsTab::General, &s);
    h.run();
    assert_eq!(h.state().draft.area, s.area);

    let s = Settings {
        my_height_m: 20_000.0,
        dx_height_m: -5.0,
        ..Settings::default()
    };
    let mut h = dialog_harness(OptionsTab::Stations, &s);
    h.run();
    assert_eq!(h.state().draft.my_height_m, 20_000.0);
    assert_eq!(h.state().draft.dx_height_m, -5.0);
}

#[test]
fn planes_tab_draws_no_feed_info_paragraph() {
    // The feed info is the hover text of each feed in the combo (`cb_Options_Planes_Feed*`);
    // the page draws no extra paragraph under `groupBox36`.
    let s = Settings::default();
    let infos: Vec<String> = (0..3)
        .filter_map(|slot| as_feeds::registry::feed_by_name(s.feed_slot(slot)))
        .map(|f| f.info().to_string())
        .filter(|info| !info.is_empty())
        .collect();
    assert!(!infos.is_empty(), "the default feed has an info text");
    let h = dialog_harness(OptionsTab::Planes, &s);
    for info in infos {
        assert!(h.query_by_label(&info).is_none(), "drawn: {info:?}");
    }
}

#[test]
fn gauge_color_swatch_selects_and_names_ignore_case() {
    use airscout::ui::map_track::gauge_color;
    use egui::accesskit::Role;

    // A click on the swatch selects the colour, like a click on its text.
    let mut h = dialog_harness(OptionsTab::Map, &Settings::default());
    h.get_by_role_and_label(Role::ComboBox, "Gauge ForeColor:")
        .click();
    h.run();
    let text = h.get_by_label("Color [ActiveCaption]").rect();
    // The swatch sits left of the text, one item spacing away, 28 pt wide at the default style.
    let at = egui::pos2(text.left() - 22.0, text.center().y);
    h.event(egui::Event::PointerMoved(at));
    h.run();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
    }
    assert_eq!(h.state().draft.gui.gauge_forecolor, "ActiveCaption");

    // `Color.FromName` ignores case: a stored "red" selects and paints Red, and is kept.
    let mut s = Settings::default();
    s.gui.gauge_forecolor = "red".into();
    let mut h = dialog_harness(OptionsTab::Map, &s);
    h.run();
    let combo = h.get_by_role_and_label(Role::ComboBox, "Gauge ForeColor:");
    assert_eq!(combo.value().as_deref(), Some("Color [Red]"));
    assert_eq!(h.state().draft.gui.gauge_forecolor, "red");
    assert_eq!(gauge_color("red"), egui::Color32::from_rgb(255, 0, 0));
    assert_eq!(gauge_color("ACTIVECAPTION"), gauge_color("ActiveCaption"));
}
