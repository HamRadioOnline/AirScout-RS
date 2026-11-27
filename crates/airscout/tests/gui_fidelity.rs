//! Fidelity comparer: checks a drawn window's trace against a synthetic
//! reference, with the allowlist and deviations log.
//!
//! The `Window`s and traces here are built in code (no upstream text), so the
//! tests exercise the comparer rules in isolation: existence, text, container
//! path, order, extras, split ratios, dock sides, visibility, and text
//! normalisation.

mod gui_ref;

use airscout::ui::forms::trace::{Entry, Kind};
use eframe::egui::{pos2, vec2, Rect};
use gui_ref::fidelity::{assert_window, compare, trace_from_reference, Allow, Mismatch};
use gui_ref::model::{Control, Window};

fn rect(min_x: f32, min_y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(min_x, min_y), vec2(w, h))
}

fn entry(path: &[&str], name: &str, kind: Kind, text: &str, rect: Rect) -> Entry {
    Entry {
        path: path.iter().map(|s| s.to_string()).collect(),
        name: name.to_string(),
        kind,
        text: text.to_string(),
        rect,
    }
}

fn control(
    name: &str,
    kind: &str,
    parent: &str,
    index: usize,
    text: Option<&str>,
    loc: [i32; 2],
    size: [i32; 2],
) -> Control {
    Control {
        name: name.to_string(),
        kind: kind.to_string(),
        parent: parent.to_string(),
        index,
        text: text.map(str::to_string),
        location: Some(loc),
        size: Some(size),
        dock: None,
        anchor: None,
        tab_index: None,
        visible: true,
        enabled: true,
        splitter_distance: None,
        orientation: None,
        items: vec![],
        events: vec![],
    }
}

fn docked(mut c: Control, dock: &str) -> Control {
    c.dock = Some(dock.to_string());
    c
}

/// A `ToolStripStatusLabel`/`ColumnHeader`-style flow item: no designer
/// `Location` (WinForms lays these out left-to-right in `index` order).
fn flow_item(
    name: &str,
    kind: &str,
    parent: &str,
    index: usize,
    text: &str,
    width: i32,
) -> Control {
    let mut c = control(name, kind, parent, index, Some(text), [0, 0], [width, 17]);
    c.location = None;
    c
}

/// The synthetic reference window used across most tests: a group box `gb`
/// (labels `l1` at y 10, `l2` at y 30), a vertical split `sc` (distance 600 in
/// 1000 width), and a top-docked label `ld`.
fn reference() -> Window {
    let mut w = Window::new("Test");
    w.client_size = Some([1000, 730]);
    w.controls = vec![
        control(
            "gb",
            "GroupBox",
            "$this",
            0,
            Some("Group"),
            [0, 30],
            [200, 100],
        ),
        control("l1", "Label", "gb", 0, Some("A"), [10, 10], [50, 20]),
        control("l2", "Label", "gb", 1, Some("B"), [10, 30], [50, 20]),
        {
            let mut c = control(
                "sc",
                "SplitContainer",
                "$this",
                1,
                None,
                [0, 130],
                [1000, 600],
            );
            c.splitter_distance = Some(600);
            c.orientation = Some("Vertical".to_string());
            c
        },
        docked(
            control("ld", "Label", "$this", 2, Some("D"), [0, 0], [100, 20]),
            "Top",
        ),
    ];
    w
}

/// The trace that matches [`reference`] exactly.
fn exact_trace() -> Vec<Entry> {
    vec![
        entry(
            &[],
            "gb",
            Kind::Container,
            "Group",
            rect(0.0, 30.0, 200.0, 100.0),
        ),
        entry(&["gb"], "l1", Kind::Text, "A", rect(10.0, 40.0, 50.0, 20.0)),
        entry(&["gb"], "l2", Kind::Text, "B", rect(10.0, 60.0, 50.0, 20.0)),
        entry(
            &[],
            "sc",
            Kind::Container,
            "",
            rect(0.0, 130.0, 1000.0, 600.0),
        ),
        entry(
            &["sc"],
            "sc.Panel1",
            Kind::Container,
            "",
            rect(0.0, 130.0, 600.0, 600.0),
        ),
        entry(
            &["sc"],
            "sc.Panel2",
            Kind::Container,
            "",
            rect(604.0, 130.0, 396.0, 600.0),
        ),
        entry(&[], "ld", Kind::Text, "D", rect(0.0, 0.0, 100.0, 20.0)),
    ]
}

#[test]
fn exact_trace_has_no_mismatch() {
    assert_eq!(compare(&reference(), &exact_trace(), &[], ""), vec![]);
}

#[test]
fn missing_and_wrong_text() {
    let mut t = exact_trace();
    t.retain(|e| e.name != "l2");
    t.iter_mut().find(|e| e.name == "l1").unwrap().text = "a".to_string();

    let got = compare(&reference(), &t, &[], "");
    assert_eq!(
        got,
        vec![
            Mismatch::WrongText {
                name: "l1".to_string(),
                expected: "A".to_string(),
                got: "a".to_string(),
            },
            Mismatch::Missing {
                name: "l2".to_string(),
                text: "B".to_string(),
            },
        ]
    );
}

#[test]
fn wrong_container_and_order() {
    // l2 recorded under the wrong container (top-level instead of inside gb).
    let mut t = exact_trace();
    t.iter_mut().find(|e| e.name == "l2").unwrap().path = vec![];
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::WrongContainer {
            name: "l2".to_string(),
            expected: vec!["gb".to_string()],
            got: vec![],
        }]
    );

    // l2 drawn above l1: the two labels are in the wrong order inside gb.
    let mut t = exact_trace();
    t.iter_mut().find(|e| e.name == "l2").unwrap().rect = rect(10.0, 5.0, 50.0, 20.0);
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::WrongOrder {
            container: "gb".to_string(),
            expected: vec!["l1".to_string(), "l2".to_string()],
            got: vec!["l2".to_string(), "l1".to_string()],
        }]
    );
}

#[test]
fn extra_needs_allowlist_and_known_deviation() {
    let allow = vec![Allow {
        name: "x_feed2".to_string(),
        what: "extra".to_string(),
        reason: "Rust-only third feed slot".to_string(),
        deviation: "D-001".to_string(),
    }];
    let dev = "## D-001 Three feed slots\n";

    // Extra after l2, no allowlist: reported.
    let mut t = exact_trace();
    t.push(entry(
        &["gb"],
        "x_feed2",
        Kind::Text,
        "Feed 2",
        rect(10.0, 80.0, 100.0, 20.0),
    ));
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::Extra {
            name: "x_feed2".to_string(),
            container: "gb".to_string(),
        }]
    );

    // Allowlisted with a known deviation: suppressed.
    assert_eq!(compare(&reference(), &t, &allow, dev), vec![]);

    // Allowlisted but the deviation is not in the log: unknown.
    assert_eq!(
        compare(&reference(), &t, &allow, ""),
        vec![Mismatch::UnknownDeviation {
            name: "x_feed2".to_string(),
            deviation: "D-001".to_string(),
        }]
    );

    // Extra placed before the original controls: order mismatch (allowlist only
    // covers the extra, not the position).
    let mut t = exact_trace();
    t.push(entry(
        &["gb"],
        "x_feed2",
        Kind::Text,
        "Feed 2",
        rect(10.0, 5.0, 100.0, 20.0),
    ));
    assert_eq!(
        compare(&reference(), &t, &allow, dev),
        vec![Mismatch::WrongOrder {
            container: "gb".to_string(),
            expected: vec!["l1".to_string(), "l2".to_string()],
            got: vec!["x_feed2".to_string(), "l1".to_string(), "l2".to_string()],
        }]
    );
}

#[test]
fn duplicate_extra_emitted_once() {
    // The same Rust-only control recorded twice (e.g. once per merged frame)
    // must be reported as a single `Extra`, not one per duplicate entry.
    let mut t = exact_trace();
    t.push(entry(
        &["gb"],
        "x_dup",
        Kind::Text,
        "X",
        rect(10.0, 80.0, 80.0, 20.0),
    ));
    t.push(entry(
        &["gb"],
        "x_dup",
        Kind::Text,
        "X",
        rect(10.0, 80.0, 80.0, 20.0),
    ));
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::Extra {
            name: "x_dup".to_string(),
            container: "gb".to_string(),
        }]
    );
}

#[test]
fn known_deviations_only_accept_d_heading() {
    // Only `## D-NNN` headings are deviations; an ordinary `## Notes` heading
    // must not be accepted as a deviation id.
    let mut t = exact_trace();
    t.push(entry(
        &["gb"],
        "x",
        Kind::Text,
        "x",
        rect(10.0, 80.0, 80.0, 20.0),
    ));
    let dev = "## Notes\n## D-001 real\n";

    let allow_d = vec![Allow {
        name: "x".to_string(),
        what: "extra".to_string(),
        reason: "r".to_string(),
        deviation: "D-001".to_string(),
    }];
    assert_eq!(compare(&reference(), &t, &allow_d, dev), vec![]);

    let allow_notes = vec![Allow {
        name: "x".to_string(),
        what: "extra".to_string(),
        reason: "r".to_string(),
        deviation: "Notes".to_string(),
    }];
    assert_eq!(
        compare(&reference(), &t, &allow_notes, dev),
        vec![Mismatch::UnknownDeviation {
            name: "x".to_string(),
            deviation: "Notes".to_string(),
        }]
    );
}

#[test]
fn split_and_dock_checks() {
    // Split ratio 0.5 instead of the reference 0.6.
    let mut t = exact_trace();
    t.iter_mut().find(|e| e.name == "sc.Panel1").unwrap().rect = rect(0.0, 130.0, 500.0, 600.0);
    t.iter_mut().find(|e| e.name == "sc.Panel2").unwrap().rect = rect(504.0, 130.0, 496.0, 600.0);
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::WrongSplit {
            name: "sc".to_string(),
            expected: 0.6,
            got: 0.5,
        }]
    );

    // Top-docked label 4 pt below the top edge.
    let mut t = exact_trace();
    t.iter_mut().find(|e| e.name == "ld").unwrap().rect = rect(0.0, 4.0, 100.0, 20.0);
    assert_eq!(
        compare(&reference(), &t, &[], ""),
        vec![Mismatch::WrongDockSide {
            name: "ld".to_string(),
            dock: "Top".to_string(),
        }]
    );
}

#[test]
fn invisible_controls_are_ignored() {
    let mut r = reference();
    let mut hidden = control("l3", "Label", "gb", 2, Some("Hidden"), [10, 50], [50, 20]);
    hidden.visible = false;
    r.controls.push(hidden);

    assert_eq!(compare(&r, &exact_trace(), &[], ""), vec![]);
}

#[test]
fn mnemonic_and_newlines_normalised() {
    let mut w = Window::new("Test");
    w.controls = vec![
        control("s", "Button", "$this", 0, Some("&Save"), [0, 0], [80, 24]),
        control("m", "Label", "$this", 1, Some("a\r\nb"), [0, 30], [80, 40]),
        control(
            "amp",
            "Label",
            "$this",
            2,
            Some("Save && Exit"),
            [0, 60],
            [80, 20],
        ),
        control(
            "pad",
            "Label",
            "$this",
            3,
            Some("   padded   "),
            [0, 80],
            [80, 20],
        ),
    ];
    let t = vec![
        entry(&[], "s", Kind::Text, "Save", rect(0.0, 0.0, 80.0, 24.0)),
        entry(&[], "m", Kind::Text, "a\nb", rect(0.0, 30.0, 80.0, 40.0)),
        entry(
            &[],
            "amp",
            Kind::Text,
            "Save & Exit",
            rect(0.0, 60.0, 80.0, 20.0),
        ),
        entry(
            &[],
            "pad",
            Kind::Text,
            "padded",
            rect(0.0, 80.0, 80.0, 20.0),
        ),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);
}

#[test]
fn split_orientation_defaults_to_vertical_and_zero_extent_fails() {
    // A SplitContainer with `orientation: None` (WinForms default `Vertical`)
    // must be checked along its width, not silently skipped.
    let mut r = Window::new("Split");
    r.controls = vec![{
        let mut c = control(
            "sc",
            "SplitContainer",
            "$this",
            0,
            None,
            [0, 0],
            [1000, 600],
        );
        c.splitter_distance = Some(600);
        c // orientation stays None
    }];

    // Panel1 half the width: ratio 0.5 instead of 0.6 → WrongSplit.
    let t = vec![
        entry(
            &[],
            "sc",
            Kind::Container,
            "",
            rect(0.0, 0.0, 1000.0, 600.0),
        ),
        entry(
            &["sc"],
            "sc.Panel1",
            Kind::Container,
            "",
            rect(0.0, 0.0, 500.0, 600.0),
        ),
        entry(
            &["sc"],
            "sc.Panel2",
            Kind::Container,
            "",
            rect(504.0, 0.0, 496.0, 600.0),
        ),
    ];
    assert_eq!(
        compare(&r, &t, &[], ""),
        vec![Mismatch::WrongSplit {
            name: "sc".to_string(),
            expected: 0.6,
            got: 0.5,
        }]
    );

    // A zero-width container has no ratio: report WrongSplit rather than let a
    // NaN/Inf ratio silently pass.
    let t = vec![
        entry(&[], "sc", Kind::Container, "", rect(0.0, 0.0, 0.0, 600.0)),
        entry(
            &["sc"],
            "sc.Panel1",
            Kind::Container,
            "",
            rect(0.0, 0.0, 0.0, 600.0),
        ),
        entry(
            &["sc"],
            "sc.Panel2",
            Kind::Container,
            "",
            rect(0.0, 0.0, 0.0, 600.0),
        ),
    ];
    let got = compare(&r, &t, &[], "");
    assert_eq!(got.len(), 1);
    assert!(
        matches!(&got[0], Mismatch::WrongSplit { name, expected, .. }
        if name == "sc" && (*expected - 0.6).abs() < 1e-6)
    );
}

#[test]
fn tab_pages_ordered_by_index_with_header_and_body() {
    // TabPages share location [4,22] and are declared in a different order than
    // their index; both the tab-header `Text` entry and the page-body
    // `Container` entry are present, and a control docked inside a page is
    // checked against the page body.
    let mut w = Window::new("Tabs");
    w.client_size = Some([300, 200]);
    w.controls = vec![
        control("tc", "TabControl", "$this", 0, None, [0, 0], [300, 200]),
        // Declared out of index order: tp_b (index 1) first, tp_a (index 0) next.
        control("tp_b", "TabPage", "tc", 1, Some("B"), [4, 22], [292, 174]),
        control("tp_a", "TabPage", "tc", 0, Some("A"), [4, 22], [292, 174]),
        docked(
            control("btn", "Button", "tp_a", 0, Some("Go"), [0, 151], [80, 23]),
            "Bottom",
        ),
    ];

    let t = vec![
        entry(&[], "tc", Kind::Container, "", rect(0.0, 0.0, 300.0, 200.0)),
        entry(&["tc"], "tp_a", Kind::Text, "A", rect(0.0, 0.0, 80.0, 24.0)),
        entry(
            &["tc"],
            "tp_b",
            Kind::Text,
            "B",
            rect(80.0, 0.0, 80.0, 24.0),
        ),
        entry(
            &["tc"],
            "tp_a",
            Kind::Container,
            "",
            rect(0.0, 24.0, 300.0, 176.0),
        ),
        entry(
            &["tc"],
            "tp_b",
            Kind::Container,
            "",
            rect(0.0, 24.0, 300.0, 176.0),
        ),
        entry(
            &["tc", "tp_a"],
            "btn",
            Kind::Text,
            "Go",
            rect(0.0, 177.0, 300.0, 23.0),
        ),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);
}

#[test]
fn status_labels_ordered_by_index_without_location() {
    // ToolStripStatusLabels have no location; declared out of index order, the
    // comparer must order them by index.
    let mut w = Window::new("Status");
    w.controls = vec![
        control("ss", "StatusStrip", "$this", 0, None, [0, 0], [400, 24]),
        control(
            "tsl_2",
            "ToolStripStatusLabel",
            "ss",
            2,
            Some("Third"),
            [0, 0],
            [80, 20],
        ),
        control(
            "tsl_0",
            "ToolStripStatusLabel",
            "ss",
            0,
            Some("First"),
            [0, 0],
            [80, 20],
        ),
        control(
            "tsl_1",
            "ToolStripStatusLabel",
            "ss",
            1,
            Some("Second"),
            [0, 0],
            [80, 20],
        ),
    ];

    let t = vec![
        entry(&[], "ss", Kind::Container, "", rect(0.0, 0.0, 400.0, 24.0)),
        entry(
            &["ss"],
            "tsl_0",
            Kind::Text,
            "First",
            rect(0.0, 0.0, 80.0, 20.0),
        ),
        entry(
            &["ss"],
            "tsl_1",
            Kind::Text,
            "Second",
            rect(80.0, 0.0, 80.0, 20.0),
        ),
        entry(
            &["ss"],
            "tsl_2",
            Kind::Text,
            "Third",
            rect(160.0, 0.0, 80.0, 20.0),
        ),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);
}

#[test]
fn stacked_bottom_docked_controls() {
    // Two controls docked Bottom in the same parent stack in reverse index
    // order (b2 at the very bottom, b1 above it); a faithful trace passes and a
    // wrongly placed one fails.
    let mut w = Window::new("Dock");
    w.client_size = Some([200, 200]);
    w.controls = vec![
        control("p", "Panel", "$this", 0, None, [0, 0], [200, 200]),
        docked(
            control("b1", "Panel", "p", 0, None, [0, 160], [200, 20]),
            "Bottom",
        ),
        docked(
            control("b2", "Panel", "p", 1, None, [0, 180], [200, 20]),
            "Bottom",
        ),
    ];

    // Correct stacking: b2 (index 1) at the bottom, b1 (index 0) above it.
    let t = vec![
        entry(&[], "p", Kind::Container, "", rect(0.0, 0.0, 200.0, 200.0)),
        entry(
            &["p"],
            "b2",
            Kind::Container,
            "",
            rect(0.0, 180.0, 200.0, 20.0),
        ),
        entry(
            &["p"],
            "b1",
            Kind::Container,
            "",
            rect(0.0, 160.0, 200.0, 20.0),
        ),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);

    // b1 placed 60 pt away from where the stack expects it.
    let t = vec![
        entry(&[], "p", Kind::Container, "", rect(0.0, 0.0, 200.0, 200.0)),
        entry(
            &["p"],
            "b2",
            Kind::Container,
            "",
            rect(0.0, 180.0, 200.0, 20.0),
        ),
        entry(
            &["p"],
            "b1",
            Kind::Container,
            "",
            rect(0.0, 100.0, 200.0, 20.0),
        ),
    ];
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongDockSide {
            name: "b1".to_string(),
            dock: "Bottom".to_string(),
        }]
    );
}

#[test]
fn extras_in_empty_and_rust_only_containers() {
    // An extra inside a reference container with no reference children, and one
    // inside a Rust-only container, are both reported unless allowlisted.
    let mut w = Window::new("Extras");
    w.client_size = Some([300, 200]);
    w.controls = vec![
        control("tc", "TabControl", "$this", 0, None, [0, 0], [300, 200]),
        // tp has no children: an empty reference container.
        control("tp", "TabPage", "tc", 0, Some("Page"), [4, 22], [292, 174]),
    ];

    let t = vec![
        entry(&[], "tc", Kind::Container, "", rect(0.0, 0.0, 300.0, 200.0)),
        entry(
            &["tc"],
            "tp",
            Kind::Text,
            "Page",
            rect(0.0, 0.0, 80.0, 24.0),
        ),
        // Extra inside the empty reference container.
        entry(
            &["tc", "tp"],
            "x_in_tab",
            Kind::Text,
            "x",
            rect(0.0, 24.0, 80.0, 20.0),
        ),
        // Rust-only menu strip (not in the reference) and an item inside it.
        entry(
            &[],
            "menu",
            Kind::Container,
            "",
            rect(0.0, 0.0, 300.0, 20.0),
        ),
        entry(
            &["menu"],
            "x_menu_item",
            Kind::Text,
            "Download",
            rect(0.0, 0.0, 80.0, 20.0),
        ),
    ];

    let mut got = compare(&w, &t, &[], "");
    // The comparer reports extras per reference container (sorted) first, then
    // Rust-only containers, so sort before asserting.
    got.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(
        got,
        vec![
            Mismatch::Extra {
                name: "menu".to_string(),
                container: "$this".to_string(),
            },
            Mismatch::Extra {
                name: "x_in_tab".to_string(),
                container: "tp".to_string(),
            },
            Mismatch::Extra {
                name: "x_menu_item".to_string(),
                container: "menu".to_string(),
            },
        ]
    );

    // Allowlisted: the empty-container and Rust-only extras are suppressed.
    let allow = vec![
        Allow {
            name: "x_in_tab".to_string(),
            what: "extra".to_string(),
            reason: "test".to_string(),
            deviation: "D-001".to_string(),
        },
        Allow {
            name: "menu".to_string(),
            what: "extra".to_string(),
            reason: "test".to_string(),
            deviation: "D-002".to_string(),
        },
        Allow {
            name: "x_menu_item".to_string(),
            what: "extra".to_string(),
            reason: "test".to_string(),
            deviation: "D-002".to_string(),
        },
    ];
    let dev = "## D-001 a\n## D-002 b\n";
    assert_eq!(compare(&w, &t, &allow, dev), vec![]);
}

#[test]
fn moved_allowlisted_control_still_checks_text() {
    // A control moved to another container (and allowlisted as `moved`) must
    // still have its text verified.
    let mut t = exact_trace();
    let l1 = t.iter_mut().find(|e| e.name == "l1").unwrap();
    l1.path = vec![]; // moved out of `gb`
    l1.text = "a".to_string(); // and the wrong text

    let allow = vec![Allow {
        name: "l1".to_string(),
        what: "moved".to_string(),
        reason: "moved for testing".to_string(),
        deviation: "D-001".to_string(),
    }];
    let dev = "## D-001 moved\n";
    assert_eq!(
        compare(&reference(), &t, &allow, dev),
        vec![Mismatch::WrongText {
            name: "l1".to_string(),
            expected: "A".to_string(),
            got: "a".to_string(),
        }]
    );
}

/// The names of every committed window reference under `docs/gui-reference/`.
fn committed_windows() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui-reference");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("gui-reference dir not readable: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    names.sort();
    names
}

#[test]
fn smoke_committed_references_render_without_mismatch() {
    // A trace built from each committed reference (containers as Container
    // entries, checked controls as Text entries, split panels from
    // geometry::split, docked children from geometry::dock_layout) must be
    // accepted as a faithful rendering of that window.
    let windows = committed_windows();
    assert!(!windows.is_empty(), "no window references found");
    for window in &windows {
        let reference = gui_ref::load(window);
        let trace = trace_from_reference(&reference);
        let mismatches = compare(&reference, &trace, &[], "");
        assert!(
            mismatches.is_empty(),
            "{window} reference rendered {} mismatch(es): {mismatches:#?}",
            mismatches.len()
        );
    }
}

#[test]
fn assert_window_passes_on_faithful_trace() {
    // `assert_window` loads the reference, allowlist and deviations log and
    // must accept a faithful trace without panicking.
    let reference = gui_ref::load("MapDlg");
    let trace = trace_from_reference(&reference);
    assert_window("MapDlg", &trace);
}

#[test]
#[should_panic(expected = "wrong text on")]
fn assert_window_panics_on_mismatch() {
    // A drawn trace with a wrong label text must panic and name the mismatch.
    let reference = gui_ref::load("MapDlg");
    let mut trace = trace_from_reference(&reference);
    let label = trace
        .iter_mut()
        .find(|e| e.kind == Kind::Text)
        .expect("MapDlg has text controls");
    label.text.push('!');
    assert_window("MapDlg", &trace);
}

#[test]
fn statusstrip_default_bottom_dock_shrinks_parent() {
    // WinForms docks a StatusStrip Bottom by default, so the Designer omits
    // `Dock` and the reference records `"dock": null`. A Bottom-docked panel
    // stacked above the strip must not false-fail, and a panel that overlaps the
    // strip must be reported as `WrongDockSide`.
    let mut w = Window::new("StatusDock");
    w.client_size = Some([400, 200]);
    w.controls = vec![
        // The strip sits at the very bottom (highest index, as in
        // CrossingHistoryDlg / DatabaseEditDlg / HorizonDlg).
        control("ss", "StatusStrip", "$this", 2, None, [0, 178], [400, 22]),
        docked(
            control("panel", "Panel", "$this", 1, None, [0, 158], [400, 20]),
            "Bottom",
        ),
    ];

    // Correct stacking: strip at the bottom, panel above it.
    let t = vec![
        entry(
            &[],
            "ss",
            Kind::Container,
            "",
            rect(0.0, 178.0, 400.0, 22.0),
        ),
        entry(
            &[],
            "panel",
            Kind::Container,
            "",
            rect(0.0, 158.0, 400.0, 20.0),
        ),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);

    // The panel overlaps the strip: after the strip has been stacked the panel
    // no longer touches the Bottom side of the remaining rect.
    let t = vec![
        entry(
            &[],
            "ss",
            Kind::Container,
            "",
            rect(0.0, 178.0, 400.0, 22.0),
        ),
        entry(
            &[],
            "panel",
            Kind::Container,
            "",
            rect(0.0, 168.0, 400.0, 30.0),
        ),
    ];
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongDockSide {
            name: "panel".to_string(),
            dock: "Bottom".to_string(),
        }]
    );
}

#[test]
fn top_level_dock_measured_from_recorded_window_rect() {
    // A window drawn below a 24 pt menu bar (offset from (0,0)) records its
    // client rect as `"$this"`. A top-docked control must be measured against
    // that recorded rect, not the reference's assumed (0,0)+client_size.
    let mut w = Window::new("Offset");
    w.client_size = Some([300, 200]);
    w.controls = vec![docked(
        control("ld", "Label", "$this", 0, Some("D"), [0, 0], [300, 20]),
        "Top",
    )];

    let t = vec![
        entry(
            &[],
            "$this",
            Kind::Container,
            "",
            rect(0.0, 24.0, 300.0, 200.0),
        ),
        entry(&[], "ld", Kind::Text, "D", rect(0.0, 24.0, 300.0, 20.0)),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);

    // A label 4 pt below the recorded window top still fails the dock check.
    let t = vec![
        entry(
            &[],
            "$this",
            Kind::Container,
            "",
            rect(0.0, 24.0, 300.0, 200.0),
        ),
        entry(&[], "ld", Kind::Text, "D", rect(0.0, 28.0, 300.0, 20.0)),
    ];
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongDockSide {
            name: "ld".to_string(),
            dock: "Top".to_string(),
        }]
    );
}

#[test]
fn independent_hand_built_trace_checks() {
    // An independent sanity check that does NOT use `trace_from_reference`: a
    // hand-built trace whose rects are computed by hand must pass, and each of
    // four deliberate mutations (wrong split ratio, swapped tab order, swapped
    // status labels, swapped stacked docks) must yield its expected mismatch.
    let mut w = Window::new("Hand");
    w.client_size = Some([400, 300]);
    w.controls = vec![
        {
            // Vertical SplitContainer (orientation None → Vertical), dock Fill.
            let mut c = control("sc", "SplitContainer", "$this", 0, None, [0, 0], [400, 238]);
            c.splitter_distance = Some(120);
            c.dock = Some("Fill".to_string());
            c
        },
        control("tc", "TabControl", "sc.Panel2", 0, None, [0, 0], [276, 238]),
        control(
            "tp_a",
            "TabPage",
            "tc",
            0,
            Some("Alpha"),
            [4, 22],
            [268, 212],
        ),
        control(
            "tp_b",
            "TabPage",
            "tc",
            1,
            Some("Beta"),
            [4, 22],
            [268, 212],
        ),
        // Two stacked Bottom docks (b1 above b2) plus the StatusStrip at the
        // very bottom (highest index → docks first).
        docked(
            control("b1", "Panel", "$this", 1, None, [0, 238], [400, 20]),
            "Bottom",
        ),
        docked(
            control("b2", "Panel", "$this", 2, None, [0, 258], [400, 20]),
            "Bottom",
        ),
        control("ss", "StatusStrip", "$this", 3, None, [0, 278], [400, 22]),
        flow_item("tsl_0", "ToolStripStatusLabel", "ss", 0, "One", 40),
        flow_item("tsl_1", "ToolStripStatusLabel", "ss", 1, "Two", 40),
        flow_item("tsl_2", "ToolStripStatusLabel", "ss", 2, "Three", 40),
    ];

    // Hand-computed rects: sc fills 0..238; the two panels and the strip stack
    // up from the bottom (b1 238..258, b2 258..278, ss 278..300); sc splits
    // vertically at 120 (Panel1 0..120, Panel2 124..400); the tab headers sit
    // 80 pt apart above the shared page body; the status labels flow left to
    // right inside the strip.
    let correct = vec![
        entry(&[], "sc", Kind::Container, "", rect(0.0, 0.0, 400.0, 238.0)),
        entry(
            &["sc"],
            "sc.Panel1",
            Kind::Container,
            "",
            rect(0.0, 0.0, 120.0, 238.0),
        ),
        entry(
            &["sc"],
            "sc.Panel2",
            Kind::Container,
            "",
            rect(124.0, 0.0, 276.0, 238.0),
        ),
        entry(
            &["sc", "sc.Panel2"],
            "tc",
            Kind::Container,
            "",
            rect(124.0, 0.0, 276.0, 238.0),
        ),
        entry(
            &["sc", "sc.Panel2", "tc"],
            "tp_a",
            Kind::Text,
            "Alpha",
            rect(124.0, 0.0, 80.0, 24.0),
        ),
        entry(
            &["sc", "sc.Panel2", "tc"],
            "tp_b",
            Kind::Text,
            "Beta",
            rect(204.0, 0.0, 80.0, 24.0),
        ),
        entry(
            &["sc", "sc.Panel2", "tc"],
            "tp_a",
            Kind::Container,
            "",
            rect(124.0, 24.0, 276.0, 214.0),
        ),
        entry(
            &["sc", "sc.Panel2", "tc"],
            "tp_b",
            Kind::Container,
            "",
            rect(124.0, 24.0, 276.0, 214.0),
        ),
        entry(
            &[],
            "b1",
            Kind::Container,
            "",
            rect(0.0, 238.0, 400.0, 20.0),
        ),
        entry(
            &[],
            "b2",
            Kind::Container,
            "",
            rect(0.0, 258.0, 400.0, 20.0),
        ),
        entry(
            &[],
            "ss",
            Kind::Container,
            "",
            rect(0.0, 278.0, 400.0, 22.0),
        ),
        entry(
            &["ss"],
            "tsl_0",
            Kind::Text,
            "One",
            rect(0.0, 278.0, 40.0, 17.0),
        ),
        entry(
            &["ss"],
            "tsl_1",
            Kind::Text,
            "Two",
            rect(40.0, 278.0, 40.0, 17.0),
        ),
        entry(
            &["ss"],
            "tsl_2",
            Kind::Text,
            "Three",
            rect(80.0, 278.0, 40.0, 17.0),
        ),
    ];
    assert_eq!(compare(&w, &correct, &[], ""), vec![]);

    // Wrong split ratio: Panel1 200 wide instead of 120 → 0.5 vs 0.3.
    let mut t = correct.clone();
    t.iter_mut().find(|e| e.name == "sc.Panel1").unwrap().rect = rect(0.0, 0.0, 200.0, 238.0);
    t.iter_mut().find(|e| e.name == "sc.Panel2").unwrap().rect = rect(204.0, 0.0, 196.0, 238.0);
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongSplit {
            name: "sc".to_string(),
            expected: 0.3,
            got: 0.5,
        }]
    );

    // Swapped tab order: the two headers exchange x positions.
    let mut t = correct.clone();
    t.iter_mut()
        .find(|e| e.name == "tp_a" && e.kind == Kind::Text)
        .unwrap()
        .rect = rect(204.0, 0.0, 80.0, 24.0);
    t.iter_mut()
        .find(|e| e.name == "tp_b" && e.kind == Kind::Text)
        .unwrap()
        .rect = rect(124.0, 0.0, 80.0, 24.0);
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongOrder {
            container: "tc".to_string(),
            expected: vec!["tp_a".to_string(), "tp_b".to_string()],
            got: vec!["tp_b".to_string(), "tp_a".to_string()],
        }]
    );

    // Swapped status labels: the first two labels exchange x positions.
    let mut t = correct.clone();
    t.iter_mut().find(|e| e.name == "tsl_0").unwrap().rect = rect(40.0, 278.0, 40.0, 17.0);
    t.iter_mut().find(|e| e.name == "tsl_1").unwrap().rect = rect(0.0, 278.0, 40.0, 17.0);
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![Mismatch::WrongOrder {
            container: "ss".to_string(),
            expected: vec![
                "tsl_0".to_string(),
                "tsl_1".to_string(),
                "tsl_2".to_string()
            ],
            got: vec![
                "tsl_1".to_string(),
                "tsl_0".to_string(),
                "tsl_2".to_string()
            ],
        }]
    );

    // Swapped stacked docks: the two Bottom panels exchange y positions, so
    // neither touches the Bottom side of the (shrunk) remaining rect.
    let mut t = correct.clone();
    t.iter_mut().find(|e| e.name == "b1").unwrap().rect = rect(0.0, 258.0, 400.0, 20.0);
    t.iter_mut().find(|e| e.name == "b2").unwrap().rect = rect(0.0, 238.0, 400.0, 20.0);
    assert_eq!(
        compare(&w, &t, &[], ""),
        vec![
            Mismatch::WrongDockSide {
                name: "b2".to_string(),
                dock: "Bottom".to_string(),
            },
            Mismatch::WrongDockSide {
                name: "b1".to_string(),
                dock: "Bottom".to_string(),
            },
        ]
    );
}
