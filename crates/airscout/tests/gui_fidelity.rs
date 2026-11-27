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
use gui_ref::fidelity::{compare, Allow, Mismatch};
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
        {
            let mut c = control("ld", "Label", "$this", 2, Some("D"), [0, 0], [100, 20]);
            c.dock = Some("Top".to_string());
            c
        },
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
    ];
    let t = vec![
        entry(&[], "s", Kind::Text, "Save", rect(0.0, 0.0, 80.0, 24.0)),
        entry(&[], "m", Kind::Text, "a\nb", rect(0.0, 30.0, 80.0, 40.0)),
    ];
    assert_eq!(compare(&w, &t, &[], ""), vec![]);
}
