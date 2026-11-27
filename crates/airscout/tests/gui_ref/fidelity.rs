//! Fidelity comparer: checks a drawn window's trace against the upstream
//! reference, applying the per-window allowlist and the deviations log.
//!
//! The reference side is the parsed WinForms tree ([`super::model::Window`]);
//! the drawn side is a flat list of [`Entry`]s recorded by the widget trace.
//! The two are compared per control: every visible text control must exist
//! under the same ancestor chain, in the same order, with the same text; split
//! ratios and docked sides are checked within tolerance; Rust-only extras are
//! reported unless allowlisted against a known deviation.

use super::model::{Control, Window};
use airscout::ui::forms::trace::Entry;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

/// One approved difference in `docs/gui-reference/<Window>.allow.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Allow {
    /// Control name, or a Rust-only name for an extra.
    pub name: String,
    /// What kind of difference: `missing`, `extra`, `text`, `moved`, `geometry`.
    pub what: String,
    /// Why the difference exists and is approved.
    pub reason: String,
    /// The `## D-NNN` heading that records this deviation, e.g. `D-003`.
    pub deviation: String,
}

/// One fidelity mismatch between a drawn trace and the reference.
#[derive(Debug, Clone, PartialEq)]
pub enum Mismatch {
    Missing {
        name: String,
        text: String,
    },
    WrongText {
        name: String,
        expected: String,
        got: String,
    },
    WrongContainer {
        name: String,
        expected: Vec<String>,
        got: Vec<String>,
    },
    WrongOrder {
        container: String,
        expected: Vec<String>,
        got: Vec<String>,
    },
    Extra {
        name: String,
        container: String,
    },
    WrongDockSide {
        name: String,
        dock: String,
    },
    WrongSplit {
        name: String,
        expected: f32,
        got: f32,
    },
    UnknownDeviation {
        name: String,
        deviation: String,
    },
}

/// Control kinds whose visible text must exist, 1:1, in the drawn window.
const CHECKED_KINDS: &[&str] = &[
    "Label",
    "LinkLabel",
    "Button",
    "CheckBox",
    "RadioButton",
    "GroupBox",
    "TabPage",
    "ColumnHeader",
    "ToolStripStatusLabel",
];

/// Dock sides that are position-checked (`Fill` and `None` are not).
const DOCK_SIDES: &[&str] = &["Top", "Bottom", "Left", "Right"];

/// Maximum distance, in points, a docked control may be from its parent edge.
const DOCK_TOLERANCE: f32 = 2.0;

/// Maximum split-ratio error, as a fraction of the container extent.
const SPLIT_TOLERANCE: f32 = 0.05;

/// Compare a drawn `trace` against the `reference`, reporting every mismatch
/// that is not approved by `allow` and recorded in `deviations`.
pub fn compare(
    reference: &Window,
    trace: &[Entry],
    allow: &[Allow],
    deviations: &str,
) -> Vec<Mismatch> {
    let known = known_deviations(deviations);
    let raw = raw_mismatches(reference, trace);

    let mut out = Vec::new();
    let mut consumed = vec![false; allow.len()];
    for mismatch in raw {
        let (name, what) = mismatch_key(&mismatch);
        if let Some(i) = allow.iter().position(|a| a.name == name && a.what == what) {
            consumed[i] = true;
            let a = &allow[i];
            if known.contains(&a.deviation) {
                continue; // approved and recorded: suppressed
            }
            out.push(Mismatch::UnknownDeviation {
                name: a.name.clone(),
                deviation: a.deviation.clone(),
            });
            continue;
        }
        out.push(mismatch);
    }
    // An allowlist entry whose deviation is not a `## D-NNN` heading is itself
    // an error, even when no mismatch consulted it.
    for (i, a) in allow.iter().enumerate() {
        if !consumed[i] && !known.contains(&a.deviation) {
            out.push(Mismatch::UnknownDeviation {
                name: a.name.clone(),
                deviation: a.deviation.clone(),
            });
        }
    }
    out
}

/// Load the reference, allowlist and deviations log for `window` and panic
/// (listing every mismatch) unless the drawn `trace` is faithful.
pub fn assert_window(window: &str, trace: &[Entry]) {
    let reference = super::load(window);
    let allow = load_allow(window);
    let deviations = load_deviations();
    let mismatches = compare(&reference, trace, &allow, &deviations);
    if mismatches.is_empty() {
        return;
    }
    let mut msg = format!("{} fidelity mismatches for {window}:\n", mismatches.len());
    for m in &mismatches {
        msg.push_str(&format_mismatch(m));
        msg.push('\n');
    }
    panic!("{msg}");
}

/// Compute every mismatch before any allowlist is applied.
fn raw_mismatches(reference: &Window, trace: &[Entry]) -> Vec<Mismatch> {
    let mut out = Vec::new();

    // Existence / text / container, in reference declaration order.
    for control in &reference.controls {
        if !is_checked(control) {
            continue;
        }
        let expected_path = chain(reference, &control.parent);
        let text = normalize(control.text.as_deref().unwrap_or(""));
        match trace.iter().find(|e| e.name == control.name) {
            None => out.push(Mismatch::Missing {
                name: control.name.clone(),
                text,
            }),
            Some(entry) => {
                if entry.path != expected_path {
                    out.push(Mismatch::WrongContainer {
                        name: control.name.clone(),
                        expected: expected_path,
                        got: entry.path.clone(),
                    });
                } else if normalize(&entry.text) != text {
                    out.push(Mismatch::WrongText {
                        name: control.name.clone(),
                        expected: control.text.clone().unwrap_or_default(),
                        got: entry.text.clone(),
                    });
                }
            }
        }
    }

    // Order and extras, per container.
    let mut containers = reference
        .controls
        .iter()
        .map(|c| c.parent.clone())
        .collect::<Vec<_>>();
    containers.push("$this".to_string());
    containers.sort();
    containers.dedup();
    for container in containers {
        out.extend(container_mismatches(reference, trace, &container));
    }

    // Dock sides.
    for control in &reference.controls {
        if !control.visible {
            continue;
        }
        let Some(dock) = control.dock.as_deref() else {
            continue;
        };
        if !DOCK_SIDES.contains(&dock) {
            continue;
        }
        let Some(entry) = trace.iter().find(|e| e.name == control.name) else {
            continue;
        };
        let Some(parent) = parent_rect(reference, trace, control) else {
            continue;
        };
        if !touches(entry.rect, parent, dock) {
            out.push(Mismatch::WrongDockSide {
                name: control.name.clone(),
                dock: dock.to_string(),
            });
        }
    }

    // Split ratios.
    for control in &reference.controls {
        if control.kind != "SplitContainer" {
            continue;
        }
        let (Some(distance), Some(orientation), Some(size)) = (
            control.splitter_distance,
            control.orientation.as_deref(),
            control.size,
        ) else {
            continue;
        };
        let expected = split_ratio(distance, size, orientation);
        let Some(expected) = expected else {
            continue;
        };
        let sc = trace.iter().find(|e| e.name == control.name);
        let panel1 = trace
            .iter()
            .find(|e| e.name == format!("{}.Panel1", control.name));
        let (Some(sc), Some(panel1)) = (sc, panel1) else {
            continue;
        };
        let got = panel1.rect.extent(orientation) / sc.rect.extent(orientation);
        if (expected - got).abs() > SPLIT_TOLERANCE {
            out.push(Mismatch::WrongSplit {
                name: control.name.clone(),
                expected,
                got,
            });
        }
    }

    out
}

/// Existence-independent per-container checks: child order and Rust-only extras.
fn container_mismatches(reference: &Window, trace: &[Entry], container: &str) -> Vec<Mismatch> {
    let mut out = Vec::new();

    // Reference children of this container, in geometry order (checked only).
    let mut expected = Vec::new();
    for c in &reference.controls {
        if c.parent == container && is_checked(c) {
            expected.push(c.name.clone());
        }
    }
    expected.sort_by(|a, b| {
        let (a, b) = (control_by_name(reference, a), control_by_name(reference, b));
        cmp_geom(geometry_key(a), geometry_key(b))
    });
    let expected_set: HashSet<&str> = expected.iter().map(|s| s.as_str()).collect();

    // Drawn children of this container, in geometry order (checked names and
    // extras only; other entries such as split containers are ignored).
    let container_path = chain(reference, container);
    let mut got: Vec<&Entry> = trace
        .iter()
        .filter(|e| {
            e.path == container_path
                && (expected_set.contains(e.name.as_str()) || is_extra(reference, &e.name))
        })
        .collect();
    got.sort_by(|a, b| {
        cmp_geom(
            ((a.rect.min.y / 10.0).round(), a.rect.min.x),
            ((b.rect.min.y / 10.0).round(), b.rect.min.x),
        )
    });

    let got_names: Vec<String> = got.iter().map(|e| e.name.clone()).collect();
    let originals: Vec<String> = got_names
        .iter()
        .filter(|n| expected_set.contains(n.as_str()))
        .cloned()
        .collect();

    // Originals must appear in the expected relative order, and every extra
    // must come after all originals.
    let in_order = is_subsequence(&originals, &expected);
    let extras_trailing = {
        let mut ok = true;
        let mut seen_extra = false;
        for n in &got_names {
            if is_extra(reference, n) {
                seen_extra = true;
            } else if seen_extra {
                ok = false; // an original appears after an extra
            }
        }
        ok
    };
    if !in_order || !extras_trailing {
        out.push(Mismatch::WrongOrder {
            container: container.to_string(),
            expected,
            got: got_names,
        });
    }

    // Rust-only extras (not allowlisted here; the caller applies the allowlist).
    for e in trace {
        if e.path != container_path {
            continue;
        }
        if is_extra(reference, &e.name) {
            out.push(Mismatch::Extra {
                name: e.name.clone(),
                container: container.to_string(),
            });
        }
    }

    out
}

/// The expected trace path of a control: its ancestor chain, outermost first,
/// excluding `$this` (a `sc.Panel1` pseudo-parent counts as its own element).
fn chain(reference: &Window, name: &str) -> Vec<String> {
    if name == "$this" {
        return Vec::new();
    }
    let mut chain = Vec::new();
    let mut cur = name.to_string();
    loop {
        chain.push(cur.clone());
        let parent = parent_name(reference, &cur);
        if parent == "$this" {
            break;
        }
        cur = parent;
    }
    chain.reverse();
    chain
}

/// The parent of `name` in the reference tree. Split-container panels are
/// pseudo-parents: `sc.Panel1` sits under `sc`, which has no dedicated control.
fn parent_name(reference: &Window, name: &str) -> String {
    if let Some(base) = name
        .strip_suffix(".Panel1")
        .or_else(|| name.strip_suffix(".Panel2"))
    {
        return base.to_string();
    }
    reference
        .controls
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.parent.clone())
        .unwrap_or_else(|| "$this".to_string())
}

/// The drawn rect a docked control must touch: its parent container's rect, or
/// the form's client area for a top-level control (falling back to the bounding
/// box of every entry when the reference records no client size).
fn parent_rect(reference: &Window, trace: &[Entry], control: &Control) -> Option<Rect> {
    if control.parent == "$this" {
        if let Some([w, h]) = reference.client_size {
            return Some(Rect::from_min_size(
                eframe::egui::pos2(0.0, 0.0),
                eframe::egui::vec2(w as f32, h as f32),
            ));
        }
        return bounding_rect(trace);
    }
    trace
        .iter()
        .find(|e| e.name == control.parent)
        .map(|e| e.rect)
}

/// Whether a docked control's `rect` touches `dock` side of `parent`.
fn touches(rect: Rect, parent: Rect, dock: &str) -> bool {
    match dock {
        "Top" => (rect.min.y - parent.min.y).abs() <= DOCK_TOLERANCE,
        "Bottom" => (rect.max.y - parent.max.y).abs() <= DOCK_TOLERANCE,
        "Left" => (rect.min.x - parent.min.x).abs() <= DOCK_TOLERANCE,
        "Right" => (rect.max.x - parent.max.x).abs() <= DOCK_TOLERANCE,
        _ => true,
    }
}

/// The bounding box of every entry, or `None` when the trace is empty.
fn bounding_rect(trace: &[Entry]) -> Option<Rect> {
    let mut it = trace.iter().map(|e| e.rect);
    let first = it.next()?;
    Some(it.fold(first, |a, b| a.union(b)))
}

/// The reference split ratio `distance / extent` along the split axis.
fn split_ratio(distance: i32, size: [i32; 2], orientation: &str) -> Option<f32> {
    let extent = match orientation {
        "Vertical" => size[0],
        "Horizontal" => size[1],
        _ => return None,
    };
    if extent <= 0 {
        return None;
    }
    Some(distance as f32 / extent as f32)
}

/// Whether `control` must exist, 1:1, in the drawn window.
fn is_checked(control: &Control) -> bool {
    control.visible
        && CHECKED_KINDS.contains(&control.kind.as_str())
        && control
            .text
            .as_deref()
            .map(|t| !normalize(t).is_empty())
            .unwrap_or(false)
}

/// Whether `name` is a Rust-only control absent from the reference.
fn is_extra(reference: &Window, name: &str) -> bool {
    !is_pseudo_panel(name) && !reference.controls.iter().any(|c| c.name == name)
}

/// Whether `name` is a split-container pseudo-panel (`sc.Panel1` / `sc.Panel2`).
fn is_pseudo_panel(name: &str) -> bool {
    name.ends_with(".Panel1") || name.ends_with(".Panel2")
}

fn control_by_name<'a>(reference: &'a Window, name: &str) -> &'a Control {
    reference
        .controls
        .iter()
        .find(|c| c.name == name)
        .expect("control in expected set must exist")
}

/// Order key on the reference side: `(round(y / 10), x)` from `Location`.
fn geometry_key(control: &Control) -> (f32, f32) {
    let [x, y] = control.location.unwrap_or([0, 0]);
    ((y as f32 / 10.0).round(), x as f32)
}

/// Compare two `(round(y / 10), x)` geometry keys.
fn cmp_geom(a: (f32, f32), b: (f32, f32)) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// Whether `needles` appears in `haystack` in the same relative order.
fn is_subsequence(needles: &[String], haystack: &[String]) -> bool {
    let mut j = 0;
    for h in haystack {
        if j < needles.len() && h == &needles[j] {
            j += 1;
        }
    }
    j == needles.len()
}

/// Strip the WinForms mnemonic (`&&` is a literal `&`, a lone `&` is dropped)
/// and normalise `\r\n` to `\n` before comparing text.
fn normalize(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.clone().next() == Some('&') {
                chars.next();
                out.push('&');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The set of `D-NNN` ids that appear as `## D-NNN …` headings in the log.
fn known_deviations(deviations: &str) -> HashSet<String> {
    let mut known = HashSet::new();
    for line in deviations.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("## ") else {
            continue;
        };
        let id: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        if !id.is_empty() {
            known.insert(id);
        }
    }
    known
}

/// The `(name, what)` pair a mismatch consults in the allowlist.
fn mismatch_key(mismatch: &Mismatch) -> (String, &'static str) {
    match mismatch {
        Mismatch::Missing { name, .. } => (name.clone(), "missing"),
        Mismatch::WrongText { name, .. } => (name.clone(), "text"),
        Mismatch::WrongContainer { name, .. } => (name.clone(), "moved"),
        Mismatch::WrongOrder { container, .. } => (container.clone(), "moved"),
        Mismatch::Extra { name, .. } => (name.clone(), "extra"),
        Mismatch::WrongDockSide { name, .. } => (name.clone(), "moved"),
        Mismatch::WrongSplit { name, .. } => (name.clone(), "geometry"),
        Mismatch::UnknownDeviation { name, .. } => (name.clone(), "missing"),
    }
}

fn format_mismatch(m: &Mismatch) -> String {
    match m {
        Mismatch::Missing { name, text } => format!("- missing `{name}` ({text:?})"),
        Mismatch::WrongText {
            name,
            expected,
            got,
        } => format!("- wrong text on `{name}`: expected {expected:?}, got {got:?}"),
        Mismatch::WrongContainer {
            name,
            expected,
            got,
        } => format!("- wrong container for `{name}`: expected {expected:?}, got {got:?}"),
        Mismatch::WrongOrder {
            container,
            expected,
            got,
        } => format!("- wrong order in `{container}`: expected {expected:?}, got {got:?}"),
        Mismatch::Extra { name, container } => {
            format!("- extra `{name}` in `{container}`")
        }
        Mismatch::WrongDockSide { name, dock } => {
            format!("- `{name}` does not touch the {dock} side")
        }
        Mismatch::WrongSplit {
            name,
            expected,
            got,
        } => format!("- wrong split on `{name}`: expected {expected}, got {got}"),
        Mismatch::UnknownDeviation { name, deviation } => {
            format!("- unknown deviation {deviation} cited for `{name}`")
        }
    }
}

/// The path of the per-window allowlist `docs/gui-reference/<window>.allow.json`.
fn allowlist_path(window: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gui-reference")
        .join(format!("{window}.allow.json"))
}

/// Load the optional allowlist for `window` (empty when absent).
fn load_allow(window: &str) -> Vec<Allow> {
    let path = allowlist_path(window);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("allowlist {} is not valid JSON: {e}", path.display()))
}

/// Load the deviations log `docs/gui-deviations.md` (empty when absent).
fn load_deviations() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui-deviations.md");
    std::fs::read_to_string(&path).unwrap_or_default()
}

/// The `Rect` type the trace entries carry.
type Rect = eframe::egui::Rect;

/// The extent of a rect along a split orientation (width for `Vertical`,
/// height for `Horizontal`).
trait Extent {
    fn extent(&self, orientation: &str) -> f32;
}

impl Extent for Rect {
    fn extent(&self, orientation: &str) -> f32 {
        match orientation {
            "Horizontal" => self.height(),
            _ => self.width(),
        }
    }
}
