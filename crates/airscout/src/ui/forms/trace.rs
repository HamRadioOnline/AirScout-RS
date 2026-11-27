//! Per-frame trace of what the WinForms-style widgets draw.
//!
//! Tracing is off by default: nothing is allocated or recorded until
//! [`enable`] is called on the [`egui::Context`]. While on, every widget and
//! container records an [`Entry`] into a per-context buffer, and [`scope`]
//! maintains a stack of container names so entries can be given a stable
//! `["gb_Map_Zoom", "gb_Map_Zoom.Panel1", …]`-style path. [`take`] drains the
//! buffer for a single frame's assertions.

use eframe::egui::{self, Rect};

/// What kind of control an entry describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A container: group box, split panel, tab control, window.
    Container,
    /// A leaf with text: label, button, check, radio, tab, group title,
    /// column, status item.
    Text,
}

/// One control drawn this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The container names scoped around this entry, outermost first.
    pub path: Vec<String>,
    /// The Designer control name.
    pub name: String,
    pub kind: Kind,
    /// The displayed text, mnemonic already stripped.
    pub text: String,
    /// The designer rect the control was placed in.
    pub rect: Rect,
}

/// Fixed [`egui::Id`]s for the per-context trace storage.
const ENABLED: &str = "airscout.forms.trace.enabled";
const ENTRIES: &str = "airscout.forms.trace.entries";
const PATH: &str = "airscout.forms.trace.path";

/// Turn tracing on for `ctx`. Idempotent; does not clear existing entries.
pub fn enable(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique(ENABLED), true);
    });
}

/// Whether tracing is currently on.
pub fn is_enabled(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::unique(ENABLED))
            .unwrap_or(false)
    })
}

/// Drain every entry recorded since the last [`take`] (or [`enable`]).
pub fn take(ctx: &egui::Context) -> Vec<Entry> {
    ctx.data_mut(|d| std::mem::take(d.get_temp_mut_or(egui::Id::unique(ENTRIES), Vec::new())))
}

/// Record one entry. A no-op while tracing is off.
pub fn record(ui: &egui::Ui, name: &str, kind: Kind, text: &str, rect: Rect) {
    if !is_enabled(ui.ctx()) {
        return;
    }
    let path = path(ui.ctx());
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or(egui::Id::unique(ENTRIES), Vec::new())
            .push(Entry {
                path,
                name: name.to_owned(),
                kind,
                text: text.to_owned(),
                rect,
            });
    });
}

/// Record the window rect as a `"$this"` container without pushing it onto the
/// path stack, so top-level docked controls can be measured against the actual
/// window rect rather than the reference's assumed `(0,0)+client_size`.
pub fn record_window(ui: &egui::Ui, rect: Rect) {
    record(ui, "$this", Kind::Container, "", rect);
}

/// Record a `Container` and run `f` with `name` pushed onto the trace path.
///
/// Nested calls nest the path, and the path is restored afterwards. A no-op
/// (beyond running `f`) while tracing is off.
pub fn scope<R>(
    ui: &mut egui::Ui,
    name: &str,
    rect: Rect,
    f: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    scope_with_text(ui, name, "", rect, f)
}

/// Like [`scope`], but the container carries `text` (a group title, say).
pub(crate) fn scope_with_text<R>(
    ui: &mut egui::Ui,
    name: &str,
    text: &str,
    rect: Rect,
    f: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if !is_enabled(ui.ctx()) {
        return f(ui);
    }
    record(ui, name, Kind::Container, text, rect);
    push(ui.ctx(), name);
    let result = f(ui);
    pop(ui.ctx());
    result
}

fn path(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| {
        d.get_temp::<Vec<String>>(egui::Id::unique(PATH))
            .unwrap_or_default()
    })
}

fn push(ctx: &egui::Context, name: &str) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or(egui::Id::unique(PATH), Vec::new())
            .push(name.to_owned());
    });
}

fn pop(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or(egui::Id::unique(PATH), Vec::<String>::new())
            .pop();
    });
}
