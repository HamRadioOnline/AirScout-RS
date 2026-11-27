//! WinForms layout: dock/split/anchor math (pure) and the trace + widget helpers.
use airscout::ui::forms::geometry::{
    anchored, designer_rect, dock_layout, split, Anchor, Dock, Orientation, SPLITTER_WIDTH,
};
use airscout::ui::forms::trace::{self, Entry, Kind};
use airscout::ui::forms::widgets;
use eframe::egui::{pos2, vec2, Rect};
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::rc::Rc;

const EPS: f32 = 1e-4;

fn rect(min_x: f32, min_y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(min_x, min_y), vec2(w, h))
}

fn assert_close(a: f32, b: f32) {
    assert!((a - b).abs() < EPS, "{a} != {b}");
}

fn assert_rect(got: Rect, want: Rect) {
    assert_close(got.min.x, want.min.x);
    assert_close(got.min.y, want.min.y);
    assert_close(got.max.x, want.max.x);
    assert_close(got.max.y, want.max.y);
}

#[test]
fn dock_processes_reverse_add_order() {
    let area = rect(0.0, 0.0, 100.0, 100.0);
    let children = [
        (Dock::Fill, vec2(0.0, 0.0)),
        (Dock::Top, vec2(0.0, 20.0)),
        (Dock::Left, vec2(30.0, 0.0)),
    ];
    let out = dock_layout(area, &children);
    assert_rect(out[0], rect(30.0, 20.0, 70.0, 80.0)); // Fill
    assert_rect(out[1], rect(30.0, 0.0, 70.0, 20.0)); // Top
    assert_rect(out[2], rect(0.0, 0.0, 30.0, 100.0)); // Left
}

#[test]
fn dock_bottom_and_right() {
    let area = rect(0.0, 0.0, 100.0, 100.0);
    let children = [
        (Dock::Fill, vec2(0.0, 0.0)),
        (Dock::Bottom, vec2(0.0, 10.0)),
        (Dock::Right, vec2(25.0, 0.0)),
    ];
    let out = dock_layout(area, &children);
    assert_rect(out[0], rect(0.0, 0.0, 75.0, 90.0)); // Fill
    assert_rect(out[1], rect(0.0, 90.0, 75.0, 10.0)); // Bottom
    assert_rect(out[2], rect(75.0, 0.0, 25.0, 100.0)); // Right
}

#[test]
fn split_vertical_and_horizontal() {
    let (left, right) = split(rect(0.0, 0.0, 1000.0, 600.0), Orientation::Vertical, 852.0);
    assert_rect(left, rect(0.0, 0.0, 852.0, 600.0));
    assert_close(left.width(), 852.0);
    assert_rect(right, rect(856.0, 0.0, 144.0, 600.0));

    let (top, bottom) = split(rect(0.0, 0.0, 800.0, 600.0), Orientation::Horizontal, 342.0);
    assert_rect(top, rect(0.0, 0.0, 800.0, 342.0));
    assert_rect(bottom, rect(0.0, 346.0, 800.0, 254.0));

    // Distance beyond the splitter clamps to extent - SPLITTER_WIDTH.
    let (l, r) = split(rect(0.0, 0.0, 1000.0, 600.0), Orientation::Vertical, 5000.0);
    assert_close(l.width(), 1000.0 - SPLITTER_WIDTH);
    assert_close(r.min.x, 1000.0);
    assert_close(r.width(), 0.0);
}

#[test]
fn anchor_stretch_and_far_edge() {
    let designer_parent = vec2(200.0, 100.0);
    let parent = rect(0.0, 0.0, 300.0, 100.0);
    let loc = [10, 10];
    let size = [50, 20];

    // left + right anchored: keep both margins, stretch the width.
    let a = anchored(
        designer_parent,
        parent,
        loc,
        size,
        Anchor {
            top: true,
            bottom: false,
            left: true,
            right: true,
        },
    );
    assert_close(a.width(), 150.0);
    assert_close(a.min.x, 10.0);

    // right only: keep the far (right) margin, width unchanged.
    let a = anchored(
        designer_parent,
        parent,
        loc,
        size,
        Anchor {
            top: false,
            bottom: false,
            left: false,
            right: true,
        },
    );
    assert_close(a.min.x, 110.0);
    assert_close(a.width(), 50.0);

    // default (top + left): keep the near position and size.
    let a = anchored(designer_parent, parent, loc, size, Anchor::default());
    assert_close(a.min.x, 10.0);
    assert_close(a.width(), 50.0);
}

#[test]
fn designer_rect_offsets_parent() {
    let parent = rect(5.0, 7.0, 200.0, 100.0);
    let r = designer_rect(parent, [8, 76], [64, 52]);
    assert_rect(r, rect(13.0, 83.0, 64.0, 52.0));
}

#[test]
fn dock_clamps_overflow_and_child_after_fill() {
    // Overflow: Top(25) + Bottom(20) + Fill in a 30-tall area. The docked
    // panels exceed the area, so Bottom is clamped to the 5px that remain and
    // Fill collapses to zero height (never a negative size).
    let area = rect(0.0, 0.0, 100.0, 30.0);
    let children = [
        (Dock::Fill, vec2(0.0, 0.0)),
        (Dock::Bottom, vec2(0.0, 20.0)),
        (Dock::Top, vec2(0.0, 25.0)),
    ];
    let out = dock_layout(area, &children);
    assert_rect(out[0], rect(0.0, 25.0, 100.0, 0.0)); // Fill collapsed
    assert_rect(out[1], rect(0.0, 25.0, 100.0, 5.0)); // Bottom clamped to 5
    assert_rect(out[2], rect(0.0, 0.0, 100.0, 25.0)); // Top full 25

    // Child after Fill: a docked panel added before Fill gets a zero-size
    // rect (remaining is collapsed to zero), never a ±∞/NaN rect.
    let area2 = rect(0.0, 0.0, 100.0, 100.0);
    let children2 = [(Dock::Left, vec2(10.0, 0.0)), (Dock::Fill, vec2(0.0, 0.0))];
    let out2 = dock_layout(area2, &children2);
    assert_rect(out2[0], rect(0.0, 0.0, 0.0, 0.0)); // Left collapsed to zero
    assert_rect(out2[1], rect(0.0, 0.0, 100.0, 100.0)); // Fill full area
}

#[test]
fn split_clamps_area_narrower_than_splitter() {
    // Vertical: area width 2 < SPLITTER_WIDTH (4); both panels collapse to
    // zero width rather than the second starting past area.max.
    let (left, right) = split(rect(0.0, 0.0, 2.0, 100.0), Orientation::Vertical, 0.0);
    assert_rect(left, rect(0.0, 0.0, 0.0, 100.0));
    assert_rect(right, rect(2.0, 0.0, 0.0, 100.0));

    // Horizontal: area height 2 < SPLITTER_WIDTH; both panels collapse to
    // zero height.
    let (top, bottom) = split(rect(0.0, 0.0, 100.0, 2.0), Orientation::Horizontal, 0.0);
    assert_rect(top, rect(0.0, 0.0, 100.0, 0.0));
    assert_rect(bottom, rect(0.0, 2.0, 100.0, 0.0));
}

#[test]
fn anchor_stretch_clamps_shrunk_parent() {
    let designer_parent = vec2(200.0, 100.0);
    let parent = rect(0.0, 0.0, 100.0, 100.0);
    let loc = [10, 10];
    let size = [50, 20];

    // left + right anchored: far margin (-40) would put far < near; clamp far
    // to near so the width stays >= 0.
    let a = anchored(
        designer_parent,
        parent,
        loc,
        size,
        Anchor {
            top: true,
            bottom: false,
            left: true,
            right: true,
        },
    );
    assert_close(a.min.x, 10.0);
    assert_close(a.width(), 0.0);
    assert_close(a.height(), 20.0);
}

#[test]
fn trace_records_path_and_kinds() {
    let gb = rect(0.0, 0.0, 200.0, 100.0);
    let btn = rect(10.0, 30.0, 80.0, 24.0);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::group_box(ui, "gb_Map_Zoom", "Map Zoom", gb, |ui| {
            widgets::button(ui, "btn_Zoom_In", "+", btn);
        });
    });
    // The harness runs a couple of setup frames; drop them for one clean frame.
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);

    assert_eq!(entries.len(), 2, "container then the button inside it");
    assert_eq!(entries[0].name, "gb_Map_Zoom");
    assert_eq!(entries[0].kind, Kind::Container);
    assert_eq!(entries[0].text, "Map Zoom");
    assert!(entries[0].path.is_empty());
    assert_eq!(entries[1].name, "btn_Zoom_In");
    assert_eq!(entries[1].kind, Kind::Text);
    assert_eq!(entries[1].text, "+");
    assert_eq!(entries[1].path, ["gb_Map_Zoom".to_owned()]);

    // Entries are kept only until taken: a second take is empty.
    assert!(trace::take(&h.ctx).is_empty());
}

#[test]
fn trace_off_records_nothing() {
    let gb = rect(0.0, 0.0, 200.0, 100.0);
    let btn = rect(10.0, 30.0, 80.0, 24.0);
    let h = egui_kittest::Harness::new_ui(move |ui| {
        widgets::group_box(ui, "gb_Map_Zoom", "Map Zoom", gb, |ui| {
            widgets::button(ui, "btn_Zoom_In", "+", btn);
        });
    });
    assert!(!trace::is_enabled(&h.ctx));
    assert!(trace::take(&h.ctx).is_empty());
}

#[test]
fn record_window_records_this_without_pushing_path() {
    let win = rect(0.0, 24.0, 300.0, 200.0);
    let btn = rect(10.0, 34.0, 80.0, 24.0);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        trace::record_window(ui, win);
        widgets::button(ui, "btn", "B", btn);
    });
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);

    let this = entries.iter().find(|e| e.name == "$this").expect("$this");
    assert_eq!(this.kind, Kind::Container);
    assert!(this.path.is_empty());
    assert_rect(this.rect, win);

    // The button after the window record is still at the top level.
    let b = entries.iter().find(|e| e.name == "btn").expect("btn");
    assert!(b.path.is_empty());
}

#[test]
fn widgets_stay_findable_by_label() {
    let btn = rect(10.0, 10.0, 120.0, 24.0);
    let clicked = Rc::new(RefCell::new(false));
    let clicked_in = Rc::clone(&clicked);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        let response = widgets::button(ui, "btn_Options", "&Options", btn);
        *clicked_in.borrow_mut() |= response.clicked();
    });

    // The mnemonic '&' is stripped from the label egui_kittest sees.
    h.get_by_label("Options");
    h.get_by_label("Options").click();
    h.run();
    assert!(*clicked.borrow(), "the button should report clicked()");
}

#[test]
fn tab_control_switches_pages() {
    let tc = rect(0.0, 0.0, 400.0, 200.0);
    let selected = Rc::new(RefCell::new(0usize));
    let selected_in = Rc::clone(&selected);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::tab_control(
            ui,
            "tab_control",
            tc,
            &[("tp_Map", "Map"), ("tp_News", "Latest News")],
            &mut selected_in.borrow_mut(),
            |_ui, _i| {},
        );
    });

    h.get_by_label("Latest News").click();
    h.run();
    assert_eq!(*selected.borrow(), 1);

    // Both page texts are Text entries recorded under the tab control's name.
    let entries = trace::take(&h.ctx);
    let pages: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.kind == Kind::Text && e.path == ["tab_control".to_owned()])
        .collect();
    assert!(
        pages.iter().any(|e| e.name == "tp_Map" && e.text == "Map"),
        "tp_Map page missing from {pages:?}"
    );
    assert!(
        pages
            .iter()
            .any(|e| e.name == "tp_News" && e.text == "Latest News"),
        "tp_News page missing from {pages:?}"
    );
}

#[test]
fn split_container_records_panels() {
    let area = rect(0.0, 0.0, 1000.0, 600.0);
    let lbl = rect(4.0, 4.0, 120.0, 20.0);
    let mut distance = 300.0f32;
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::split_container(
            ui,
            "sc",
            Orientation::Vertical,
            &mut distance,
            area,
            |ui| {
                widgets::label(ui, "lbl_Inside", "inside", lbl);
            },
            |_ui| {},
        );
    });
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);

    let (panel1, panel2) = split(area, Orientation::Vertical, 300.0);

    let sc = entries.iter().find(|e| e.name == "sc").expect("sc");
    assert_eq!(sc.kind, Kind::Container);
    assert!(sc.path.is_empty());
    assert_rect(sc.rect, area);

    let p1 = entries
        .iter()
        .find(|e| e.name == "sc.Panel1")
        .expect("Panel1");
    assert_eq!(p1.kind, Kind::Container);
    assert_eq!(p1.path, ["sc".to_owned()]);
    assert_rect(p1.rect, panel1);

    let p2 = entries
        .iter()
        .find(|e| e.name == "sc.Panel2")
        .expect("Panel2");
    assert_eq!(p2.kind, Kind::Container);
    assert_eq!(p2.path, ["sc".to_owned()]);
    assert_rect(p2.rect, panel2);

    let label = entries
        .iter()
        .find(|e| e.name == "lbl_Inside")
        .expect("label inside Panel1");
    assert_eq!(label.kind, Kind::Text);
    assert_eq!(label.path, ["sc".to_owned(), "sc.Panel1".to_owned()]);
}

#[test]
fn group_box_strips_mnemonic_draws_title_and_scopes_body() {
    let gb = rect(0.0, 0.0, 300.0, 150.0);
    let body = Rc::new(RefCell::new(Rect::NOTHING));
    let body_in = Rc::clone(&body);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::group_box(ui, "gb_Map", "&Map && Zoom", gb, |ui| {
            *body_in.borrow_mut() = ui.max_rect();
        });
    });
    trace::take(&h.ctx);
    h.run();

    // The group title is drawn (findable) with the mnemonic stripped.
    h.get_by_label("Map & Zoom");

    // The body runs in a child Ui scoped to the group rect.
    assert_rect(*body.borrow(), gb);

    // The trace records the container with the stripped title.
    let entries = trace::take(&h.ctx);
    let container = entries.iter().find(|e| e.name == "gb_Map").expect("gb_Map");
    assert_eq!(container.kind, Kind::Container);
    assert_eq!(container.text, "Map & Zoom");
}

#[test]
fn split_container_scopes_panels_to_their_rects() {
    let area = rect(0.0, 0.0, 600.0, 400.0);
    let mut distance = 200.0f32;
    let rects = Rc::new(RefCell::new(Vec::new()));
    let rects_in = Rc::clone(&rects);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        widgets::split_container(
            ui,
            "sc",
            Orientation::Vertical,
            &mut distance,
            area,
            |ui| rects_in.borrow_mut().push(ui.max_rect()),
            |ui| rects_in.borrow_mut().push(ui.max_rect()),
        );
    });
    h.run();
    rects.borrow_mut().clear();
    h.run();
    let (panel1, panel2) = split(area, Orientation::Vertical, 200.0);
    let rects = rects.borrow();
    assert_eq!(rects.len(), 2);
    assert_rect(rects[0], panel1);
    assert_rect(rects[1], panel2);
}

#[test]
fn tab_control_content_rect_clamps_short_control() {
    let tc = rect(0.0, 0.0, 200.0, 10.0);
    let content = Rc::new(RefCell::new(Rect::NOTHING));
    let content_in = Rc::clone(&content);
    let mut selected = 0usize;
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        widgets::tab_control(ui, "tc", tc, &[("tp_A", "A")], &mut selected, |ui, _i| {
            *content_in.borrow_mut() = ui.max_rect();
        });
    });
    h.run();
    let r = *content.borrow();
    assert!(r.min.y <= r.max.y, "content rect must not invert: {r:?}");
    assert!(r.min.x <= r.max.x, "content rect must not invert: {r:?}");
    assert_close(r.min.y, tc.max.y);
    assert_close(r.max.y, tc.max.y);
}

#[test]
fn tab_control_body_control_is_scoped_under_page() {
    let tc = rect(0.0, 0.0, 400.0, 200.0);
    let lbl = rect(10.0, 40.0, 80.0, 20.0);
    let mut selected = 0usize;
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::tab_control(
            ui,
            "tc",
            tc,
            &[("tp_Map", "Map"), ("tp_News", "Latest News")],
            &mut selected,
            |ui, i| {
                if i == 0 {
                    widgets::label(ui, "lbl_Inside", "inside", lbl);
                }
            },
        );
    });
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);
    let label = entries
        .iter()
        .find(|e| e.name == "lbl_Inside")
        .expect("label inside the active page");
    assert_eq!(label.kind, Kind::Text);
    assert_eq!(label.path, ["tc".to_owned(), "tp_Map".to_owned()]);
}

#[test]
fn tab_control_wraps_headers_within_rect() {
    let tc = rect(0.0, 0.0, 160.0, 200.0);
    let names: Vec<String> = (0..12).map(|i| format!("tp_{i}")).collect();
    let labels: Vec<String> = (0..12).map(|i| format!("Page {i}")).collect();
    let pages: Vec<(&str, &str)> = names
        .iter()
        .zip(labels.iter())
        .map(|(n, l)| (n.as_str(), l.as_str()))
        .collect();
    let pages_len = pages.len();
    let content = Rc::new(RefCell::new(Rect::NOTHING));
    let content_in = Rc::clone(&content);
    let mut selected = 0usize;
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        trace::enable(ui.ctx());
        widgets::tab_control(ui, "tc", tc, &pages, &mut selected, |ui, _i| {
            *content_in.borrow_mut() = ui.max_rect();
        });
    });
    trace::take(&h.ctx);
    h.run();
    let entries = trace::take(&h.ctx);

    let headers: Vec<Rect> = entries
        .iter()
        .filter(|e| e.kind == Kind::Text && e.path == ["tc".to_owned()])
        .map(|e| e.rect)
        .collect();
    assert_eq!(headers.len(), pages_len);

    for r in &headers {
        assert!(
            r.min.x >= tc.min.x - EPS && r.max.x <= tc.max.x + EPS,
            "header {r:?} overflows {tc:?}"
        );
    }

    let min_y = headers.iter().map(|r| r.min.y).fold(f32::MAX, f32::min);
    let max_y = headers.iter().map(|r| r.min.y).fold(f32::MIN, f32::max);
    assert!(
        max_y - min_y > EPS,
        "expected multiple header rows, got y {min_y}..{max_y}"
    );

    let last_bottom = headers.iter().map(|r| r.max.y).fold(f32::MIN, f32::max);
    assert!(
        content.borrow().min.y >= last_bottom - EPS,
        "content {:?} starts above the last header row bottom {last_bottom}",
        *content.borrow()
    );
}

#[test]
fn split_container_drag_updates_distance_and_clamps() {
    let area = rect(0.0, 0.0, 600.0, 400.0);
    let distance = Rc::new(RefCell::new(200.0f32));
    let distance_in = Rc::clone(&distance);
    let mut h = egui_kittest::Harness::new_ui(move |ui| {
        widgets::split_container(
            ui,
            "sc",
            Orientation::Vertical,
            &mut distance_in.borrow_mut(),
            area,
            |_ui| {},
            |_ui| {},
        );
    });
    h.run();

    // The splitter gap sits between the panels at x = distance..distance + 4.
    let splitter_center = pos2(200.0 + SPLITTER_WIDTH * 0.5, 200.0);

    // Dragging the splitter right updates `*distance`.
    h.drag_at(splitter_center);
    h.run();
    h.hover_at(pos2(400.0, 200.0));
    h.run();
    h.drop_at(pos2(400.0, 200.0));
    h.run();
    assert_close(*distance.borrow(), 400.0 - SPLITTER_WIDTH * 0.5);

    // Dragging far past the right edge clamps to extent - SPLITTER_WIDTH.
    let splitter_center = pos2(*distance.borrow() + SPLITTER_WIDTH * 0.5, 200.0);
    h.drag_at(splitter_center);
    h.run();
    h.hover_at(pos2(700.0, 200.0));
    h.run();
    h.drop_at(pos2(700.0, 200.0));
    h.run();
    assert_close(*distance.borrow(), area.width() - SPLITTER_WIDTH);

    // Dragging far past the left edge clamps to 0.0.
    let splitter_center = pos2(*distance.borrow() + SPLITTER_WIDTH * 0.5, 200.0);
    h.drag_at(splitter_center);
    h.run();
    h.hover_at(pos2(-50.0, 200.0));
    h.run();
    h.drop_at(pos2(-50.0, 200.0));
    h.run();
    assert_close(*distance.borrow(), 0.0);
}
