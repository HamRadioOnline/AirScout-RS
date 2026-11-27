//! WinForms layout geometry: dock, split and anchor math (pure, no rendering).
use airscout::ui::forms::geometry::{
    anchored, designer_rect, dock_layout, split, Anchor, Dock, Orientation, SPLITTER_WIDTH,
};
use eframe::egui::{pos2, vec2, Rect};

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
