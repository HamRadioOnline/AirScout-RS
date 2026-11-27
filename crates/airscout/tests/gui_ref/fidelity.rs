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
use airscout::ui::forms::geometry::{self, Dock, Orientation};
use airscout::ui::forms::trace::{Entry, Kind};
use eframe::egui::{pos2, vec2, Vec2};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
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
pub const CHECKED_KINDS: &[&str] = &[
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

/// The effective dock side of a control: its explicit `dock`, else the WinForms
/// default for its type. WinForms docks a `StatusStrip` `Bottom` and a
/// `MenuStrip`/`ToolStrip` `Top` by default, so the Designer omits `Dock` and
/// the reference records `"dock": null` for those controls. Every dock read in
/// `compare` and `trace_from_reference` goes through this helper.
pub fn effective_dock(control: &Control) -> Option<&str> {
    if let Some(dock) = control.dock.as_deref() {
        return Some(dock);
    }
    match control.kind.as_str() {
        "StatusStrip" => Some("Bottom"),
        "MenuStrip" | "ToolStrip" => Some("Top"),
        _ => None,
    }
}

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
        match entry_by_name(trace, &control.name) {
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
                }
                // Independent of the container check: a moved/allowlisted
                // control's text is still verified.
                if normalize_trace(&entry.text) != text {
                    out.push(Mismatch::WrongText {
                        name: control.name.clone(),
                        expected: control.text.clone().unwrap_or_default(),
                        got: entry.text.clone(),
                    });
                }
            }
        }
    }

    // Order and extras, per reference container.
    let mut containers = reference
        .controls
        .iter()
        .map(|c| c.parent.clone())
        .collect::<Vec<_>>();
    containers.push("$this".to_string());
    containers.sort();
    containers.dedup();
    let covered: HashSet<Vec<String>> = containers.iter().map(|c| chain(reference, c)).collect();
    for container in &containers {
        out.extend(container_mismatches(reference, trace, container));
    }

    // Rust-only extras inside reference containers with no reference children
    // (e.g. an empty `TabPage`) or inside Rust-only containers (e.g. a menu
    // strip): every distinct trace path not already covered by a reference
    // container is scanned for extras under its innermost segment.
    for e in trace {
        if covered.contains(&e.path) {
            continue;
        }
        if is_extra(reference, &e.name) {
            let container = e
                .path
                .last()
                .cloned()
                .unwrap_or_else(|| "$this".to_string());
            out.push(Mismatch::Extra {
                name: e.name.clone(),
                container,
            });
        }
    }

    // Dock sides, grouped per parent so stacked docking is verified in reverse
    // index order against a shrinking `remaining` rect (mirroring WinForms
    // reverse z-order docking in `geometry::dock_layout`).
    let mut docked: BTreeMap<&str, Vec<&Control>> = BTreeMap::new();
    for control in &reference.controls {
        if !control.visible {
            continue;
        }
        let Some(dock) = effective_dock(control) else {
            continue;
        };
        if !DOCK_SIDES.contains(&dock) {
            continue;
        }
        docked
            .entry(control.parent.as_str())
            .or_default()
            .push(control);
    }
    for (parent, mut children) in docked {
        let Some(mut remaining) = parent_rect(reference, trace, parent) else {
            continue;
        };
        children.sort_by_key(|c| std::cmp::Reverse(c.index));
        for child in children {
            let Some(entry) = entry_by_name(trace, &child.name) else {
                continue;
            };
            let dock = effective_dock(child).unwrap();
            if !touches(entry.rect, remaining, dock) {
                out.push(Mismatch::WrongDockSide {
                    name: child.name.clone(),
                    dock: dock.to_string(),
                });
            }
            remaining = shrink(remaining, entry.rect, dock);
        }
    }

    // Split ratios.
    for control in &reference.controls {
        if control.kind != "SplitContainer" || !control.visible {
            continue;
        }
        let orientation = control.orientation.as_deref().unwrap_or("Vertical");
        let (Some(distance), Some(size)) = (control.splitter_distance, control.size) else {
            continue;
        };
        let Some(expected) = split_ratio(distance, size, orientation) else {
            continue;
        };
        let sc = entry_by_name(trace, &control.name);
        let panel1 = entry_by_name(trace, &format!("{}.Panel1", control.name));
        let (Some(sc), Some(panel1)) = (sc, panel1) else {
            continue;
        };
        let extent = sc.rect.extent(orientation);
        let got = panel1.rect.extent(orientation) / extent;
        if extent <= 0.0 || (expected - got).abs() > SPLIT_TOLERANCE {
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

    // Reference children of this container, in geometry order (checked only),
    // with ties broken by `index` so `TabPage`s (identical location) and
    // null-location items order by declaration index.
    let mut expected = Vec::new();
    for c in &reference.controls {
        if c.parent == container && is_checked(c) {
            expected.push(c.name.clone());
        }
    }
    expected.sort_by(|a, b| {
        let (ca, cb) = (control_by_name(reference, a), control_by_name(reference, b));
        cmp_geom(geometry_key(ca), geometry_key(cb)).then_with(|| ca.index.cmp(&cb.index))
    });
    let expected_set: HashSet<&str> = expected.iter().map(|s| s.as_str()).collect();

    // Drawn children of this container, deduplicated so a control recorded
    // both as its header (`Kind::Text`) and body (`Kind::Container`) is counted
    // once, then sorted by geometry.
    let container_path = chain(reference, container);
    // Reference `location`s are parent-relative, while trace rects are
    // screen-absolute, so the order key must measure both from the container's
    // own origin or a parent offset (e.g. `pa_Buttons` at y 285) shifts the
    // 10 pt rounding cliffs between the two sides.
    let origin_y = parent_rect(reference, trace, container)
        .map(|r| r.min.y)
        .unwrap_or(0.0);
    let mut got: Vec<&Entry> = dedup_entries(trace.iter().filter(|e| {
        e.path == container_path
            && (expected_set.contains(e.name.as_str()) || is_extra(reference, &e.name))
    }));
    got.sort_by(|a, b| {
        cmp_geom(
            (((a.rect.min.y - origin_y) / 10.0).round(), a.rect.min.x),
            (((b.rect.min.y - origin_y) / 10.0).round(), b.rect.min.x),
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
///
/// A parent recorded both as its header and its body (a `TabPage`) resolves to
/// the `Container` body rect, so docked controls are measured against the page
/// body rather than the 24 pt tab header.
fn parent_rect(reference: &Window, trace: &[Entry], parent: &str) -> Option<Rect> {
    if parent == "$this" {
        // A recorded window rect (offset by a menu bar, say) takes precedence
        // over the reference client size, which assumes a (0,0) origin.
        if let Some(entry) = trace
            .iter()
            .find(|e| e.name == "$this" && e.kind == Kind::Container)
        {
            return Some(entry.rect);
        }
        if let Some([w, h]) = reference.client_size {
            return Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(w as f32, h as f32),
            ));
        }
        return bounding_rect(trace);
    }
    trace
        .iter()
        .filter(|e| e.name == parent)
        .max_by_key(|e| e.kind == Kind::Container)
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

/// Shrink `remaining` along `dock` to the inner edge of a docked `child`,
/// mirroring the reverse z-order stacking of `geometry::dock_layout`.
fn shrink(mut remaining: Rect, child: Rect, dock: &str) -> Rect {
    match dock {
        "Top" => remaining.min.y = child.max.y,
        "Bottom" => remaining.max.y = child.min.y,
        "Left" => remaining.min.x = child.max.x,
        "Right" => remaining.max.x = child.min.x,
        _ => {}
    }
    remaining
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
pub fn is_checked(control: &Control) -> bool {
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
    name != "$this" && !is_pseudo_panel(name) && !reference.controls.iter().any(|c| c.name == name)
}

/// Whether `name` is a split-container pseudo-panel (`sc.Panel1` / `sc.Panel2`).
fn is_pseudo_panel(name: &str) -> bool {
    name.ends_with(".Panel1") || name.ends_with(".Panel2")
}

/// The trace entry that represents `name`'s drawn control, preferring the
/// `Kind::Text` entry (its header/title) over a `Kind::Container` body: a
/// `TabPage` is recorded once as its tab header (`Kind::Text`) and once as its
/// scoped page body (`Kind::Container`).
fn entry_by_name<'a>(trace: &'a [Entry], name: &str) -> Option<&'a Entry> {
    trace
        .iter()
        .filter(|e| e.name == name)
        .max_by_key(|e| e.kind == Kind::Text)
}

/// Keep one entry per `(path, name)`, preferring a `Kind::Text` entry (the
/// drawn header/title) over a `Kind::Container` body, so a `TabPage` recorded
/// both as its tab header and its scoped body is counted once.
fn dedup_entries<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<&'a Entry> {
    let mut out: Vec<&'a Entry> = Vec::new();
    for e in entries {
        match out
            .iter_mut()
            .find(|x| x.name == e.name && x.path == e.path)
        {
            Some(existing) => {
                let prefer = (e.kind == Kind::Text && existing.kind != Kind::Text)
                    || (!e.text.is_empty() && existing.text.is_empty());
                if prefer {
                    *existing = e;
                }
            }
            None => out.push(e),
        }
    }
    out
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

/// Strip the WinForms mnemonic (`&&` is a literal `&`, a lone `&` is dropped),
/// normalise `\r\n` to `\n`, and trim leading/trailing whitespace.
pub fn normalize(text: &str) -> String {
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
    out.trim().to_string()
}

/// Normalise a recorded trace string. The widget has already stripped the
/// mnemonic, so only newlines and ends are normalised here (re-stripping would
/// corrupt a literal `&` produced from a reference `&&`).
fn normalize_trace(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
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
        if id
            .strip_prefix("D-")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
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

/// Build a faithful trace for `reference` — the entries a correct renderer of
/// this window would record — using only the reference's own geometry:
/// [`geometry::dock_layout`] for docked children, [`geometry::split`] for split
/// panels, and [`geometry::designer_rect`] for the rest. Phase 2 reuses this to
/// cross-check its own rendering against a real reference such as `MapDlg`.
pub fn trace_from_reference(reference: &Window) -> Vec<Entry> {
    let client = match reference.client_size {
        Some([w, h]) => Rect::from_min_size(pos2(0.0, 0.0), vec2(w as f32, h as f32)),
        None => Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)),
    };
    let mut out = Vec::new();
    layout(reference, "$this", client, &mut out);
    out
}

/// Height of a `TabControl` header strip.
const TAB_HEIGHT: f32 = 24.0;

/// Width given to each tab header in the synthetic trace; only the left-to-right
/// order matters, so a fixed width is enough.
const TAB_HEADER_WIDTH: f32 = 80.0;

/// Container kinds whose children without a `Location` flow left-to-right in
/// `index` order (status-strip labels, list-view column headers).
const FLOW_KINDS: &[&str] = &["StatusStrip", "ToolStrip", "ListView"];

/// Width given to a flow item (status label, column header) whose designer
/// `Size` is absent; only the left-to-right order matters, so a fixed width is
/// enough.
const FLOW_ITEM_WIDTH: i32 = 80;

/// Lay out and emit every control whose parent is `container`, inside `area`.
fn layout(reference: &Window, container: &str, area: Rect, out: &mut Vec<Entry>) {
    let mut children: Vec<&Control> = reference
        .controls
        .iter()
        .filter(|c| c.parent == container)
        .collect();
    children.sort_by_key(|c| c.index);

    // Docked children are laid out in one `dock_layout` pass (reverse index
    // stacking); non-docked children use their designer location.
    let docked: Vec<&Control> = children
        .iter()
        .copied()
        .filter(|c| effective_dock(c).is_some())
        .collect();
    let specs: Vec<(Dock, Vec2)> = docked
        .iter()
        .map(|c| (dock_of(c), control_size(c)))
        .collect();
    let mut dock_rects = geometry::dock_layout(area, &specs).into_iter();

    // Children of a StatusStrip/ToolStrip/ListView without a `Location` (status
    // labels, column headers) flow left-to-right in index order instead of all
    // stacking at (0, 0).
    let flow = reference
        .controls
        .iter()
        .find(|c| c.name == container)
        .map(|c| FLOW_KINDS.contains(&c.kind.as_str()))
        .unwrap_or(false);
    let mut flow_x = area.min.x;

    for c in children {
        let rect = if effective_dock(c).is_some() {
            dock_rects.next().unwrap_or(Rect::NOTHING)
        } else if flow && c.location.is_none() {
            let size = c.size.unwrap_or([FLOW_ITEM_WIDTH, 0]);
            let rect = Rect::from_min_size(
                pos2(flow_x, area.min.y),
                vec2(size[0] as f32, size[1] as f32),
            );
            flow_x += size[0] as f32;
            rect
        } else {
            geometry::designer_rect(area, c.location.unwrap_or([0, 0]), c.size.unwrap_or([0, 0]))
        };
        emit(reference, c, rect, out);
    }
}

/// Emit the entries for a single control `c` at `rect` (and its children).
fn emit(reference: &Window, c: &Control, rect: Rect, out: &mut Vec<Entry>) {
    match c.kind.as_str() {
        "SplitContainer" => emit_split_container(reference, c, rect, out),
        "TabControl" => emit_tab_control(reference, c, rect, out),
        "TabPage" => {
            // The body is scoped around the page's children (the tab header was
            // already emitted by the owning `TabControl`).
            out.push(Entry {
                path: entry_path(reference, c),
                name: c.name.clone(),
                kind: Kind::Container,
                text: String::new(),
                rect,
            });
            layout(reference, &c.name, rect, out);
        }
        "GroupBox" => {
            // A group box records a single `Container` entry carrying its title.
            out.push(Entry {
                path: entry_path(reference, c),
                name: c.name.clone(),
                kind: Kind::Container,
                text: normalize(c.text.as_deref().unwrap_or("")),
                rect,
            });
            layout(reference, &c.name, rect, out);
        }
        _ => {
            if has_children(reference, &c.name) {
                out.push(Entry {
                    path: entry_path(reference, c),
                    name: c.name.clone(),
                    kind: Kind::Container,
                    text: normalize(c.text.as_deref().unwrap_or("")),
                    rect,
                });
                layout(reference, &c.name, rect, out);
            } else if is_checked(c) {
                out.push(Entry {
                    path: entry_path(reference, c),
                    name: c.name.clone(),
                    kind: Kind::Text,
                    text: normalize(c.text.as_deref().unwrap_or("")),
                    rect,
                });
            }
        }
    }
}

fn emit_split_container(reference: &Window, c: &Control, rect: Rect, out: &mut Vec<Entry>) {
    out.push(Entry {
        path: entry_path(reference, c),
        name: c.name.clone(),
        kind: Kind::Container,
        text: String::new(),
        rect,
    });
    let orientation = orientation_of(c);
    let distance = c.splitter_distance.unwrap_or(0) as f32;
    let (panel1, panel2) = geometry::split(rect, orientation, distance);
    let panel1_name = format!("{}.Panel1", c.name);
    let panel2_name = format!("{}.Panel2", c.name);
    out.push(Entry {
        path: chain(reference, &c.name),
        name: panel1_name.clone(),
        kind: Kind::Container,
        text: String::new(),
        rect: panel1,
    });
    layout(reference, &panel1_name, panel1, out);
    out.push(Entry {
        path: chain(reference, &c.name),
        name: panel2_name.clone(),
        kind: Kind::Container,
        text: String::new(),
        rect: panel2,
    });
    layout(reference, &panel2_name, panel2, out);
}

fn emit_tab_control(reference: &Window, c: &Control, rect: Rect, out: &mut Vec<Entry>) {
    out.push(Entry {
        path: entry_path(reference, c),
        name: c.name.clone(),
        kind: Kind::Container,
        text: String::new(),
        rect,
    });
    let content_top = (rect.min.y + TAB_HEIGHT).min(rect.max.y);
    let content_rect = Rect::from_min_max(pos2(rect.min.x, content_top), rect.max);
    let mut pages: Vec<&Control> = reference
        .controls
        .iter()
        .filter(|p| p.parent == c.name)
        .collect();
    pages.sort_by_key(|p| p.index);
    let mut x = rect.min.x;
    for page in pages {
        let header = Rect::from_min_size(pos2(x, rect.min.y), vec2(TAB_HEADER_WIDTH, TAB_HEIGHT));
        out.push(Entry {
            path: entry_path(reference, page),
            name: page.name.clone(),
            kind: Kind::Text,
            text: normalize(page.text.as_deref().unwrap_or("")),
            rect: header,
        });
        x += TAB_HEADER_WIDTH;
        emit(reference, page, content_rect, out);
    }
}

/// The trace path of a control: its parent's ancestor chain.
fn entry_path(reference: &Window, control: &Control) -> Vec<String> {
    chain(reference, &control.parent)
}

fn dock_of(control: &Control) -> Dock {
    match effective_dock(control) {
        Some("Top") => Dock::Top,
        Some("Bottom") => Dock::Bottom,
        Some("Left") => Dock::Left,
        Some("Right") => Dock::Right,
        Some("Fill") => Dock::Fill,
        _ => Dock::None,
    }
}

fn orientation_of(control: &Control) -> Orientation {
    match control.orientation.as_deref() {
        Some("Horizontal") => Orientation::Horizontal,
        _ => Orientation::Vertical,
    }
}

fn control_size(control: &Control) -> Vec2 {
    let [w, h] = control.size.unwrap_or([0, 0]);
    vec2(w as f32, h as f32)
}

fn has_children(reference: &Window, name: &str) -> bool {
    reference.controls.iter().any(|c| c.parent == name)
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
