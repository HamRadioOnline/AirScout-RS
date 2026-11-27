//! Pure WinForms layout geometry: `Dock`, `SplitContainer` and `Anchor`
//! placement mapped onto egui `Rect`s, independent of any widget rendering.

use eframe::egui::{pos2, vec2, Rect, Vec2};

/// Designer pixels (96 dpi) map 1:1 to egui points.
pub const DESIGNER_SCALE: f32 = 1.0;

/// `SplitContainer.SplitterWidth` in egui points.
pub const SPLITTER_WIDTH: f32 = 4.0;

/// `Control.Dock`: which edge (or all of them, for `Fill`) a control sticks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    None,
    Top,
    Bottom,
    Left,
    Right,
    Fill,
}

/// `Control.Anchor`: which edges of the parent a control keeps its margin to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// Mirrors WinForms' default anchor of `Top | Left`.
impl Default for Anchor {
    fn default() -> Self {
        Self {
            top: true,
            bottom: false,
            left: true,
            right: false,
        }
    }
}

/// `Orientation` of a `SplitContainer`: `Vertical` stacks its panels side by
/// side, `Horizontal` stacks them one above the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// Places a control at `location` of `size` relative to the parent's top-left,
/// as a designer does before any docking or anchoring.
pub fn designer_rect(parent: Rect, location: [i32; 2], size: [i32; 2]) -> Rect {
    let min = parent.min + vec2(location[0] as f32, location[1] as f32) * DESIGNER_SCALE;
    let size = vec2(size[0] as f32, size[1] as f32) * DESIGNER_SCALE;
    Rect::from_min_size(min, size)
}

/// Lays out `children` (in `Controls.Add` order, each with its designer size)
/// inside `area`, mirroring WinForms reverse z-order docking.
pub fn dock_layout(area: Rect, children: &[(Dock, Vec2)]) -> Vec<Rect> {
    let mut remaining = area;
    let mut out = vec![Rect::NOTHING; children.len()];
    for (i, (dock, size)) in children.iter().enumerate().rev() {
        out[i] = match dock {
            Dock::None => Rect::NOTHING,
            Dock::Top => {
                let h = size.y.clamp(0.0, remaining.height().max(0.0));
                let r = Rect::from_min_size(remaining.min, vec2(remaining.width(), h));
                remaining.min.y += h;
                r
            }
            Dock::Bottom => {
                let h = size.y.clamp(0.0, remaining.height().max(0.0));
                let r =
                    Rect::from_min_max(pos2(remaining.min.x, remaining.max.y - h), remaining.max);
                remaining.max.y -= h;
                r
            }
            Dock::Left => {
                let w = size.x.clamp(0.0, remaining.width().max(0.0));
                let r = Rect::from_min_size(remaining.min, vec2(w, remaining.height()));
                remaining.min.x += w;
                r
            }
            Dock::Right => {
                let w = size.x.clamp(0.0, remaining.width().max(0.0));
                let r =
                    Rect::from_min_max(pos2(remaining.max.x - w, remaining.min.y), remaining.max);
                remaining.max.x -= w;
                r
            }
            Dock::Fill => {
                let r = remaining;
                remaining = Rect::from_min_size(remaining.min, Vec2::ZERO);
                r
            }
        };
    }
    out
}

/// Splits `area` along `orientation` at `distance` (clamped to
/// `[0, extent − SPLITTER_WIDTH]`), leaving the splitter gap between the panels.
pub fn split(area: Rect, orientation: Orientation, distance: f32) -> (Rect, Rect) {
    let w = area.width().max(0.0);
    let h = area.height().max(0.0);
    match orientation {
        Orientation::Vertical => {
            let d = distance.clamp(0.0, (w - SPLITTER_WIDTH).max(0.0));
            let left = Rect::from_min_size(area.min, vec2(d, h));
            let right_x = (area.min.x + d + SPLITTER_WIDTH).min(area.max.x);
            let right = Rect::from_min_max(pos2(right_x, area.min.y), area.max);
            (left, right)
        }
        Orientation::Horizontal => {
            let d = distance.clamp(0.0, (h - SPLITTER_WIDTH).max(0.0));
            let top = Rect::from_min_size(area.min, vec2(w, d));
            let bottom_y = (area.min.y + d + SPLITTER_WIDTH).min(area.max.y);
            let bottom = Rect::from_min_max(pos2(area.min.x, bottom_y), area.max);
            (top, bottom)
        }
    }
}

/// Anchors a control (designer `location`/`size` in a `designer_parent`) to the
/// current `parent`, keeping the relevant margins the way WinForms does.
pub fn anchored(
    designer_parent: Vec2,
    parent: Rect,
    location: [i32; 2],
    size: [i32; 2],
    anchor: Anchor,
) -> Rect {
    let x = axis(
        anchor.left,
        anchor.right,
        parent.min.x,
        parent.max.x,
        designer_parent.x,
        location[0] as f32,
        size[0] as f32,
    );
    let y = axis(
        anchor.top,
        anchor.bottom,
        parent.min.y,
        parent.max.y,
        designer_parent.y,
        location[1] as f32,
        size[1] as f32,
    );
    Rect::from_min_max(pos2(x.0, y.0), pos2(x.1, y.1))
}

/// Resolves one axis to `(near, far)` coordinates in the current parent.
fn axis(
    near_anchor: bool,
    far_anchor: bool,
    parent_near: f32,
    parent_far: f32,
    designer_parent: f32,
    location: f32,
    size: f32,
) -> (f32, f32) {
    let near_margin = location;
    let far_margin = designer_parent - (location + size);
    let near = parent_near + near_margin;
    let far = parent_far - far_margin;
    match (near_anchor, far_anchor) {
        // Both edges anchored: keep both margins, stretch. Clamp far to near
        // so a shrunk parent cannot invert the axis into a negative size.
        (true, true) => (near, far.max(near)),
        // Only the far edge anchored: keep the far margin, size unchanged.
        (false, true) => (far - size, far),
        // Near edge only (or neither, treated as near): keep the near position.
        (true, false) | (false, false) => (near, near + size),
    }
}
