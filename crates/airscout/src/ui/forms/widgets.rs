//! WinForms-style widgets: each draws itself at an explicit designer `rect`
//! (via [`egui::Ui::put`]) and records what it drew into the frame trace.
//!
//! The Designer control `name` is the recorded identity; the widget's own
//! egui label is the displayed text, so `egui_kittest` still finds widgets by
//! their (mnemonic-stripped) label.

use crate::ui::forms::geometry::{split, Orientation, SPLITTER_WIDTH};
use crate::ui::forms::trace::{self, Kind};
use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense};

/// Height reserved for a `group_box` title at the top of the frame.
const GROUP_TITLE_HEIGHT: f32 = 20.0;

/// Strip the WinForms mnemonic: `&&` renders a literal `&`, a lone `&`
/// marks the following character and is not shown.
fn strip_mnemonic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.clone().next() == Some('&') {
                chars.next();
                out.push('&');
            }
            // A lone '&' is a mnemonic marker and is dropped.
        } else {
            out.push(c);
        }
    }
    out
}

/// A plain text label placed at `rect`. Text in a fixed Designer rect must not
/// wrap: it renders on a single line and may extend to the right past `rect`
/// (WinForms labels auto-size the same way).
pub fn label(ui: &mut egui::Ui, name: &str, text: &str, rect: Rect) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    put_left(ui, rect, egui::Label::new(display).extend())
}

/// A single-line label that elides with "…" instead of overflowing (for runtime
/// values in fixed rects).
pub fn label_truncated(ui: &mut egui::Ui, name: &str, text: &str, rect: Rect) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    put_left(ui, rect, egui::Label::new(display).truncate())
}

/// Place `widget` at `rect` with a top-left origin (the WinForms `Label`
/// `TextAlign = TopLeft`): the content starts at `rect.min` and grows or
/// truncates to the right. [`egui::Ui::put`] uses a centred layout that shifts
/// overflowing text left of `rect` and centres short values inside it.
pub fn put_left(ui: &mut egui::Ui, rect: Rect, widget: impl egui::Widget) -> egui::Response {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Min)),
        |ui| ui.add(widget),
    )
    .inner
}

/// A push button placed at `rect`.
pub fn button(ui: &mut egui::Ui, name: &str, text: &str, rect: Rect) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    ui.put(rect, egui::Button::new(display.clone()))
}

/// A push button with a fixed background fill (the WinForms `BackColor`).
pub fn button_fill(
    ui: &mut egui::Ui,
    name: &str,
    text: &str,
    rect: Rect,
    fill: Color32,
) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    ui.put(rect, egui::Button::new(display.clone()).fill(fill))
}

/// A checkbox placed at `rect`.
pub fn checkbox(
    ui: &mut egui::Ui,
    name: &str,
    text: &str,
    value: &mut bool,
    rect: Rect,
) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    ui.put(rect, egui::Checkbox::new(value, display.clone()))
}

/// A radio button placed at `rect`, left-aligned like [`label`] (see [`put_left`]).
pub fn radio(
    ui: &mut egui::Ui,
    name: &str,
    text: &str,
    selected: bool,
    rect: Rect,
) -> egui::Response {
    let display = strip_mnemonic(text);
    trace::record(ui, name, Kind::Text, &display, rect);
    put_left(ui, rect, egui::RadioButton::new(selected, display))
}

/// A titled group box around `rect`; `f` draws its contents.
pub fn group_box<R>(
    ui: &mut egui::Ui,
    name: &str,
    title: &str,
    rect: Rect,
    f: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let display = strip_mnemonic(title);

    // Draw a visible frame around the group.
    ui.painter().rect_stroke(
        rect,
        0.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );

    // The title sits at the top of the frame. Placed directly (not through
    // [`label`]) so only the `Container` entry is recorded for the group.
    if !display.is_empty() {
        let title_rect = Rect::from_min_size(
            rect.min + vec2(8.0, 0.0),
            vec2((rect.width() - 8.0).max(0.0), GROUP_TITLE_HEIGHT),
        );
        ui.put(title_rect, egui::Label::new(display.clone()));
    }

    // Run the body in a child `Ui` bounded (and clipped) to the group rect.
    trace::scope_with_text(ui, name, &display, rect, |ui| scoped(ui, rect, f))
}

/// A tab control: the `pages` (`(name, text)`) are selectable tabs along the
/// top of `rect`; the active page's `body` runs in a child `Ui` scoped to the
/// content rect below the tab strip (which wraps onto additional rows when the
/// next header would pass `rect.max.x`).
pub fn tab_control(
    ui: &mut egui::Ui,
    name: &str,
    rect: Rect,
    pages: &[(&str, &str)],
    selected: &mut usize,
    body: impl FnOnce(&mut egui::Ui, usize),
) {
    tab_control_with_guard(ui, name, rect, pages, selected, |_| true, body);
}

/// Like [`tab_control`], with the option to refuse a click before drawing its page.
pub fn tab_control_with_selection(
    ui: &mut egui::Ui,
    name: &str,
    rect: Rect,
    pages: &[(&str, &str)],
    selected: &mut usize,
    allow_change: bool,
    body: impl FnOnce(&mut egui::Ui, usize),
) {
    tab_control_with_guard(ui, name, rect, pages, selected, move |_| allow_change, body);
}

/// Like [`tab_control`], but each click on tab `i` is refused unless `allow(i)`
/// accepts the switch. The active page's body always runs.
pub fn tab_control_with_guard(
    ui: &mut egui::Ui,
    name: &str,
    rect: Rect,
    pages: &[(&str, &str)],
    selected: &mut usize,
    mut allow: impl FnMut(usize) -> bool,
    body: impl FnOnce(&mut egui::Ui, usize),
) {
    const TAB_HEIGHT: f32 = 24.0;
    trace::scope(ui, name, rect, |ui| {
        let mut x = rect.min.x;
        let mut y = rect.min.y;
        for (i, (page_name, text)) in pages.iter().enumerate() {
            let display = strip_mnemonic(text);
            let width = tab_width(ui, &display);
            if x > rect.min.x && x + width > rect.max.x {
                // Wrap onto a new header row instead of overflowing.
                x = rect.min.x;
                y += TAB_HEIGHT;
            }
            let header = Rect::from_min_size(pos2(x, y), vec2(width, TAB_HEIGHT));
            trace::record(ui, page_name, Kind::Text, &display, header);
            let response = ui.put(header, egui::Button::selectable(*selected == i, display));
            // A click on the already-selected tab is not a switch: skip the guard so
            // a no-op re-click cannot spuriously run departing-page validation.
            if response.clicked() && *selected != i && allow(i) {
                *selected = i;
            }
            x += width;
        }
        // Clamp so a control shorter than the tab strip never yields an inverted
        // (negative-height) content rect.
        let content_top = (y + TAB_HEIGHT).min(rect.max.y);
        let content_rect = Rect::from_min_max(pos2(rect.min.x, content_top), rect.max);

        // The active page is recorded as a container at `[.., name, page_name]`
        // and its body runs in a rect-scoped child Ui of the content rect.
        let active = *selected;
        if let Some((page_name, _)) = pages.get(active) {
            trace::scope(ui, page_name, content_rect, |ui| {
                scoped(ui, content_rect, |ui| body(ui, active));
            });
        }
    });
}

/// A `SplitContainer`: splits `rect` into two panels at `*distance` and runs
/// `first` / `second` in each. The splitter bar between them is draggable and
/// updates `*distance` (clamped as in [`split`]).
///
/// Records a `Container` for `name` (the whole area) and scoped `Container`s
/// for `<name>.Panel1` and `<name>.Panel2`.
pub fn split_container(
    ui: &mut egui::Ui,
    name: &str,
    orientation: Orientation,
    distance: &mut f32,
    rect: Rect,
    first: impl FnOnce(&mut egui::Ui),
    second: impl FnOnce(&mut egui::Ui),
) {
    let mut first = Some(first);
    let mut second = Some(second);
    split_panels(ui, name, orientation, distance, rect, |ui, idx| {
        if idx == 0 {
            first.take().expect("Panel1 is drawn once")(ui);
        } else {
            second.take().expect("Panel2 is drawn once")(ui);
        }
    });
}

/// Draw both panels via a single callback so they can share a mutable borrow.
/// Returns true when a drag supplies a new splitter distance.
pub(crate) fn split_panels(
    ui: &mut egui::Ui,
    name: &str,
    orientation: Orientation,
    distance: &mut f32,
    rect: Rect,
    mut body: impl FnMut(&mut egui::Ui, usize),
) -> bool {
    let (panel1, panel2) = split(rect, orientation, *distance);

    // The splitter is the gap `split` leaves between the panels.
    let splitter = splitter_rect(rect, panel1, panel2, orientation);
    let response = ui.interact(
        splitter,
        ui.scope_id().with(name).with("splitter"),
        Sense::drag(),
    );
    let drag_pointer = response
        .interact_pointer_pos()
        .filter(|_| response.dragged());
    if let Some(pointer) = drag_pointer {
        let new_distance = match orientation {
            Orientation::Vertical => pointer.x - rect.min.x - SPLITTER_WIDTH * 0.5,
            Orientation::Horizontal => pointer.y - rect.min.y - SPLITTER_WIDTH * 0.5,
        };
        let extent = match orientation {
            Orientation::Vertical => rect.width(),
            Orientation::Horizontal => rect.height(),
        };
        *distance = new_distance.clamp(0.0, (extent - SPLITTER_WIDTH).max(0.0));
    }
    let splitter_color = if response.dragged() || response.hovered() {
        ui.visuals().widgets.active.bg_fill
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    };
    ui.painter().rect_filled(splitter, 0.0, splitter_color);

    if trace::is_enabled(ui.ctx()) {
        trace::scope(ui, name, rect, |ui| {
            trace::scope(ui, &format!("{name}.Panel1"), panel1, |ui| {
                scoped(ui, panel1, |ui| body(ui, 0));
            });
            trace::scope(ui, &format!("{name}.Panel2"), panel2, |ui| {
                scoped(ui, panel2, |ui| body(ui, 1));
            });
        });
    } else {
        // Tracing off: run the panels directly, with no panel-name allocation.
        scoped(ui, panel1, |ui| body(ui, 0));
        scoped(ui, panel2, |ui| body(ui, 1));
    }
    drag_pointer.is_some()
}

/// Run `f` in a fresh child `Ui` bounded (and clipped) to `rect`, so the body
/// can read its panel via `ui.max_rect()` and cannot paint outside `rect`.
pub(crate) fn scoped<R>(ui: &mut egui::Ui, rect: Rect, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.shrink_clip_rect(rect);
    f(&mut child)
}

/// The gap between the two panels of a split container.
fn splitter_rect(area: Rect, panel1: Rect, panel2: Rect, orientation: Orientation) -> Rect {
    match orientation {
        Orientation::Vertical => Rect::from_min_max(
            pos2(panel1.max.x, area.min.y),
            pos2(panel2.min.x, area.max.y),
        ),
        Orientation::Horizontal => Rect::from_min_max(
            pos2(area.min.x, panel1.max.y),
            pos2(area.max.x, panel2.min.y),
        ),
    }
}

/// The natural width of a tab header: text plus the button padding.
fn tab_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE);
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}
