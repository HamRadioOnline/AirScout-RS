//! GUI 1:1 reference: Task 1 parses WinForms `*.Designer.cs` into a reference
//! model; Task 2 runs the parser over every real upstream window and commits the
//! generated `docs/gui-reference/*.json` files.
//!
//! The Task 1 tests parse small synthetic Designer snippets only (no upstream
//! text is copied here). Task 2 tests read the upstream checkout and the
//! committed reference files.

mod gui_ref;

use gui_ref::designer::parse;
use gui_ref::model::{Control, Event, Window};
use gui_ref::resx::lookup;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn parse_src(src: &str) -> Window {
    parse("Window", src, &|_| None)
}

fn control<'a>(w: &'a Window, name: &str) -> &'a Control {
    w.controls
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("control `{name}` not found"))
}

#[test]
fn parses_kind_parent_and_order() {
    let src = r#"
        this.gb = new System.Windows.Forms.GroupBox();
        this.b1 = new System.Windows.Forms.Button();
        this.b2 = new System.Windows.Forms.Button();
        this.Controls.Add(this.gb);
        this.gb.Controls.Add(this.b2);
        this.gb.Controls.Add(this.b1);
    "#;
    let w = parse_src(src);
    assert_eq!(control(&w, "gb").parent, "$this");
    assert_eq!(control(&w, "b2").index, 0);
    assert_eq!(control(&w, "b1").index, 1);
    assert_eq!(control(&w, "b1").kind, "Button");
}

#[test]
fn parses_geometry_dock_anchor() {
    let src = r#"
        this.x = new System.Windows.Forms.SplitContainer();
        this.x.Location = new System.Drawing.Point(8, 76);
        this.x.Size = new System.Drawing.Size(64, 52);
        this.x.Dock = System.Windows.Forms.DockStyle.Right;
        this.x.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
        this.x.SplitterDistance = 342;
        this.x.Orientation = System.Windows.Forms.Orientation.Horizontal;
        this.Controls.Add(this.x);
    "#;
    let w = parse_src(src);
    let x = control(&w, "x");
    assert_eq!(x.location, Some([8, 76]));
    assert_eq!(x.size, Some([64, 52]));
    assert_eq!(x.dock.as_deref(), Some("Right"));
    assert_eq!(x.anchor, Some(vec!["Top".to_string(), "Right".to_string()]));
    assert_eq!(x.splitter_distance, Some(342));
    assert_eq!(x.orientation.as_deref(), Some("Horizontal"));
}

#[test]
fn joins_concatenated_strings_and_escapes() {
    // Multi-line concatenation with escape sequences.
    let w = parse_src(
        r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = "line 1\r\nline 2" +
            " ... line 3";
        this.Controls.Add(this.l);
        "#,
    );
    assert_eq!(
        control(&w, "l").text.as_deref(),
        Some("line 1\r\nline 2 ... line 3")
    );

    // Escaped quote and backslash.
    let w = parse_src(
        r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = "say \"hi\" \\ ok";
        this.Controls.Add(this.l);
        "#,
    );
    assert_eq!(control(&w, "l").text.as_deref(), Some("say \"hi\" \\ ok"));

    // Verbatim string (backslash is literal).
    let w = parse_src(
        r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = @"C:\x";
        this.Controls.Add(this.l);
        "#,
    );
    assert_eq!(control(&w, "l").text.as_deref(), Some("C:\\x"));
}

#[test]
fn resolves_resource_strings() {
    let resx =
        "<data name=\"l.Text\" xml:space=\"preserve\"><value>a &amp; b&#xD;\nc</value></data>";
    let src = r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = resources.GetString("l.Text");
        this.Controls.Add(this.l);
    "#;
    let w = parse("Window", src, &|k| lookup(resx, k));
    assert_eq!(control(&w, "l").text.as_deref(), Some("a & b\r\nc"));
}

#[test]
fn reads_items_columns_events_visibility() {
    let src = r#"
        this.cb = new System.Windows.Forms.ComboBox();
        this.cb.Items.AddRange(new object[] { "OFF", "ON" });
        this.lv = new System.Windows.Forms.ListView();
        this.c1 = new System.Windows.Forms.ColumnHeader();
        this.c2 = new System.Windows.Forms.ColumnHeader();
        this.lv.Columns.AddRange(new System.Windows.Forms.ColumnHeader[] { this.c1, this.c2 });
        this.b = new System.Windows.Forms.Button();
        this.b.Click += new System.EventHandler(this.b_Click);
        this.b.Visible = false;
        this.Controls.Add(this.cb);
        this.Controls.Add(this.lv);
        this.Controls.Add(this.b);
    "#;
    let w = parse_src(src);
    assert_eq!(
        control(&w, "cb").items,
        vec!["OFF".to_string(), "ON".to_string()]
    );
    assert_eq!(control(&w, "c2").parent, "lv");
    assert_eq!(control(&w, "c2").index, 1);
    assert_eq!(control(&w, "c1").index, 0);
    assert_eq!(
        control(&w, "b").events,
        vec![Event {
            event: "Click".to_string(),
            handler: "b_Click".to_string(),
        }]
    );
    assert!(!control(&w, "b").visible);
}

#[test]
fn reports_unknown_forms() {
    let w = parse_src("this.x.Size = SomeCall();");
    assert_eq!(w.unparsed.len(), 1);
    assert!(w.unparsed[0].contains("SomeCall"));
}

// --- Fix round 1: one test per Important review finding. ---

// Finding 1: form-level value-type `new` assignments must not become controls.
#[test]
fn form_level_value_types_are_not_controls() {
    let src = r#"
        this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
        this.components = new System.ComponentModel.Container();
        this.MinimumSize = new System.Drawing.Size(0, 0);
        this.gb = new System.Windows.Forms.GroupBox();
        this.Controls.Add(this.gb);
    "#;
    let w = parse_src(src);
    for name in ["AutoScaleDimensions", "components", "MinimumSize"] {
        assert!(
            !w.controls.iter().any(|c| c.name == name),
            "value-type assignment `{name}` must not create a control"
        );
    }
    assert_eq!(control(&w, "gb").kind, "GroupBox");
}

// Finding 2: unknown `.AddRange(` forms must be reported in `unparsed`.
#[test]
fn reports_unknown_addrange_forms() {
    let w = parse_src("this.p.Controls.AddRange(new System.Windows.Forms.Control[] { this.a });");
    assert_eq!(w.unparsed.len(), 1);
    assert!(w.unparsed[0].contains("AddRange"));
}

// Finding 3: `Settings.Default.*` Text/Enabled bindings are runtime-bound and
// must be skipped (no static text, enabled stays default `true`, no `unparsed`).
#[test]
fn skips_runtime_settings_bindings() {
    let src = r#"
        this.x = new System.Windows.Forms.TextBox();
        this.y = new System.Windows.Forms.CheckBox();
        this.x.Text = global::AirScout.Properties.Settings.Default.Foo;
        this.y.Enabled = global::AirScout.Properties.Settings.Default.Bar;
        this.Controls.Add(this.x);
        this.Controls.Add(this.y);
    "#;
    let w = parse_src(src);
    assert!(w.unparsed.is_empty());
    assert_eq!(control(&w, "x").text, None);
    assert!(control(&w, "y").enabled);
}

// Finding 4: `=` inside a string literal in `Items.AddRange(new object[])` must
// not be mistaken for an assignment.
#[test]
fn items_addrange_with_equals_in_string() {
    let src = r#"
        this.cb = new System.Windows.Forms.ComboBox();
        this.cb.Items.AddRange(new object[] { "A = B", "C" });
        this.Controls.Add(this.cb);
    "#;
    let w = parse_src(src);
    assert_eq!(
        control(&w, "cb").items,
        vec!["A = B".to_string(), "C".to_string()]
    );
    assert!(w.unparsed.is_empty());
}

// Finding 5: resx `lookup` must stay within the matched `<data>` element.
#[test]
fn resx_lookup_bounds_to_data_element() {
    // Self-closing `<value />` yields the empty string.
    let resx = "<data name=\"k\" xml:space=\"preserve\"><value /></data>";
    assert_eq!(lookup(resx, "k"), Some(String::new()));

    // A key with no `<value>` must not steal the next key's value.
    let resx = "<data name=\"empty\"></data><data name=\"next\"><value>leak</value></data>";
    assert_eq!(lookup(resx, "empty"), None);
    assert_eq!(lookup(resx, "next"), Some("leak".to_string()));
}

// Finding 6: `resources.GetString("k" )` (whitespace before `)`) must parse.
#[test]
fn resource_string_allows_space_before_paren() {
    let resx = "<data name=\"k\" xml:space=\"preserve\"><value>v</value></data>";
    let w = parse(
        "Window",
        r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = resources.GetString("k" );
        this.Controls.Add(this.l);
        "#,
        &|key| lookup(resx, key),
    );
    assert_eq!(control(&w, "l").text.as_deref(), Some("v"));
    assert!(w.unparsed.is_empty());
}

// Finding 7: concatenated strings inside `Items.AddRange(new object[])` join.
#[test]
fn items_addrange_concatenates_strings() {
    let src = r#"
        this.cb = new System.Windows.Forms.ComboBox();
        this.cb.Items.AddRange(new object[] { "a" + "b", "c" });
        this.Controls.Add(this.cb);
    "#;
    let w = parse_src(src);
    assert_eq!(
        control(&w, "cb").items,
        vec!["ab".to_string(), "c".to_string()]
    );
    assert!(w.unparsed.is_empty());
}

// --- Task 2: parser rules added for the real upstream windows. ---

// Components that are never attached to a parent (BackgroundWorker, Timer,
// ImageList, ToolTip, …) are not controls and must be dropped.
#[test]
fn drops_unparented_components() {
    let src = r#"
        this.bw = new System.ComponentModel.BackgroundWorker();
        this.bw.DoWork += new System.ComponentModel.DoWorkEventHandler(this.bw_DoWork);
        this.tt = new System.Windows.Forms.ToolTip(this.components);
        this.b = new System.Windows.Forms.Button();
        this.Controls.Add(this.b);
    "#;
    let w = parse_src(src);
    assert!(w.controls.iter().any(|c| c.name == "b"));
    assert!(
        !w.controls.iter().any(|c| c.name == "bw"),
        "BackgroundWorker must be dropped from controls"
    );
    assert!(
        !w.controls.iter().any(|c| c.name == "tt"),
        "ToolTip must be dropped from controls"
    );
}

// `Pages.Add` attaches a page to its wizard, exactly like `Controls.Add`.
#[test]
fn parses_pages_add_parent() {
    let src = r#"
        this.sw = new AeroWizard.StepWizardControl();
        this.wp1 = new AeroWizard.WizardPage();
        this.wp2 = new AeroWizard.WizardPage();
        this.sw.Pages.Add(this.wp1);
        this.sw.Pages.Add(this.wp2);
        this.Controls.Add(this.sw);
    "#;
    let w = parse_src(src);
    assert_eq!(control(&w, "wp1").parent, "sw");
    assert_eq!(control(&w, "wp2").parent, "sw");
    assert_eq!(control(&w, "wp2").index, 1);
    assert_eq!(control(&w, "sw").parent, "$this");
}

// Text-entry controls (`TextBox` and subclasses) carry user-input default values
// (callsigns, locators, coordinates) that must not be copied into the reference.
#[test]
fn drops_text_of_user_input_controls() {
    let src = r#"
        this.l = new System.Windows.Forms.Label();
        this.l.Text = "A label";
        this.tb = new System.Windows.Forms.TextBox();
        this.tb.Text = "12.345";
        this.mtb = new System.Windows.Forms.MaskedTextBox();
        this.mtb.Text = "12.345";
        this.dtb = new ScoutBase.Core.DoubleTextBox();
        this.dtb.Text = "12.345";
        this.Controls.Add(this.l);
        this.Controls.Add(this.tb);
        this.Controls.Add(this.mtb);
        this.Controls.Add(this.dtb);
    "#;
    let w = parse_src(src);
    assert_eq!(control(&w, "l").text.as_deref(), Some("A label"));
    assert_eq!(control(&w, "tb").text, None);
    assert_eq!(control(&w, "mtb").text, None);
    assert_eq!(control(&w, "dtb").text, None);
    assert!(w.unparsed.is_empty());
}

// --- Task 2: generator over every upstream window. ---

/// Upstream commit the Designer files are extracted from (`tools/fetch-upstream.sh`).
const PINNED_SHA: &str = "91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5";

fn upstream_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/upstream/AirScout")
}

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui-reference")
}

/// `HEAD` of the upstream checkout, or `None` when it has not been fetched.
fn upstream_sha() -> Option<String> {
    let out = Command::new("git")
        .args(["-C"])
        .arg(upstream_dir())
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Window names of every `*.Designer.cs` in the upstream checkout, except the
/// never-instantiated `ReflectionDlg` (25 windows).
fn designer_windows() -> Vec<String> {
    let dir = upstream_dir().join("AirScout");
    let mut windows = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("upstream AirScout directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("cs") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let Some(name) = stem.strip_suffix(".Designer") else {
            continue;
        };
        if name == "ReflectionDlg" {
            continue;
        }
        windows.push(name.to_string());
    }
    windows.sort();
    windows
}

/// Window names of the committed `docs/gui-reference/*.json` files.
fn committed_windows() -> Vec<String> {
    let dir = reference_dir();
    let mut windows = Vec::new();
    if !dir.is_dir() {
        return windows;
    }
    for entry in std::fs::read_dir(&dir).expect("docs/gui-reference directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        windows.push(stem.to_string());
    }
    windows.sort();
    windows
}

/// Parse one upstream window into a reference model with the pinned SHA filled in.
fn read_upstream_window(window: &str) -> Window {
    let dir = upstream_dir().join("AirScout");
    let designer = std::fs::read_to_string(dir.join(format!("{window}.Designer.cs")))
        .expect("upstream Designer file");
    let resx = std::fs::read_to_string(dir.join(format!("{window}.resx"))).unwrap_or_default();
    let mut w = parse(window, &designer, &|key| lookup(&resx, key));
    w.upstream_sha = PINNED_SHA.to_string();
    w
}

/// Writes `docs/gui-reference/*.json` for every upstream window. Ignored by
/// default; run explicitly to (re)generate the committed reference files.
#[test]
#[ignore]
fn generate_gui_reference() {
    if !upstream_dir().is_dir() {
        panic!("run tools/fetch-upstream.sh");
    }
    let dir = reference_dir();
    std::fs::create_dir_all(&dir).expect("create docs/gui-reference");
    for window in designer_windows() {
        let w = read_upstream_window(&window);
        let json = serde_json::to_string_pretty(&w).expect("serialize reference");
        std::fs::write(dir.join(format!("{window}.json")), format!("{json}\n"))
            .expect("write reference");
    }
}

/// Asserts the committed reference files match a fresh parse of the upstream
/// checkout at the pinned SHA (and that no file is extra or missing). Skips when
/// the upstream checkout is absent.
#[test]
fn gui_reference_is_current() {
    let Some(sha) = upstream_sha() else {
        eprintln!("skipped: upstream not fetched");
        return;
    };
    if sha != PINNED_SHA {
        eprintln!("skipped: upstream not fetched");
        return;
    }

    let windows = designer_windows();
    let generated: std::collections::BTreeMap<String, Window> = windows
        .iter()
        .map(|w| (w.clone(), read_upstream_window(w)))
        .collect();

    let mut committed = committed_windows();
    committed.sort();
    let mut expected = windows.clone();
    expected.sort();
    assert_eq!(
        committed, expected,
        "committed reference files differ from the upstream window set"
    );

    for window in &windows {
        let on_disk = gui_ref::load(window);
        assert_eq!(
            on_disk, generated[window],
            "committed reference for {window} is out of date; regenerate it"
        );
    }
}

/// Every committed reference has no unparsed Designer statements, and `MapDlg`
/// carries the expected split-container geometry.
#[test]
fn reference_has_no_unparsed_statements() {
    let windows = committed_windows();
    assert!(
        !windows.is_empty(),
        "no committed reference files under docs/gui-reference"
    );
    for window in &windows {
        let w = gui_ref::load(window);
        assert!(
            w.unparsed.is_empty(),
            "{} has unparsed Designer statements: {:?}",
            window,
            w.unparsed
        );
    }

    let map = gui_ref::load("MapDlg");
    let sc_map = map
        .controls
        .iter()
        .find(|c| c.name == "sc_Map")
        .expect("MapDlg has sc_Map");
    assert_eq!(sc_map.splitter_distance, Some(342));
    assert_eq!(sc_map.orientation.as_deref(), Some("Horizontal"));
    let sc_main = map
        .controls
        .iter()
        .find(|c| c.name == "sc_Main")
        .expect("MapDlg has sc_Main");
    assert_eq!(sc_main.splitter_distance, Some(852));
}

// --- Task 6: gap inventory skeleton. ---

/// Control kinds that are settings fields: their Rust counterpart is an
/// `as-settings` field edited from some UI (see the task brief).
const SETTINGS_KINDS: &[&str] = &["ComboBox", "TextBox", "NumericUpDown", "CheckBox"];

/// AirScout's C# settings-input subclasses (in addition to the four base kinds
/// above). These appear in the Designer files as `ScoutBase.Core.*` controls and
/// are settings fields even when the Designer subscribes no event (R7). `RichTextBox`
/// is *not* a settings field: it is a standard read-only display control.
const SUBCLASS_KINDS: &[&str] = &[
    "Int32TextBox",
    "DoubleTextBox",
    "LongTextBox",
    "CallsignTextBox",
    "LocatorTextBox",
    "CallsignComboBox",
    "LocatorComboBox",
    "PercentageControl",
];

/// Path of the gap inventory `docs/gui-reference/gaps.md`.
fn gaps_path() -> PathBuf {
    reference_dir().join("gaps.md")
}

/// Whether `kind` is a settings-input control: one of the brief's four base
/// kinds or an AirScout custom subclass used in the Designer files.
fn is_settings_kind(kind: &str) -> bool {
    SETTINGS_KINDS.contains(&kind) || SUBCLASS_KINDS.contains(&kind)
}

/// Whether `control` gets a row in the gap inventory: a checked control
/// (Task 5 rule), a control with a subscribed event, or a settings field.
fn gap_row(control: &Control) -> bool {
    gui_ref::fidelity::is_checked(control)
        || !control.events.is_empty()
        || is_settings_kind(&control.kind)
}

/// Escape a cell value for a markdown table: pipes are escaped and newlines
/// become spaces (a cell cannot contain a literal newline).
fn md_cell(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\n', " ")
        .replace('|', "\\|")
        .trim()
        .to_string()
}

/// The C# handlers column: `event: handler` pairs, comma-separated.
fn handlers_cell(control: &Control) -> String {
    control
        .events
        .iter()
        .map(|e| format!("{}: {}", e.event, e.handler))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One markdown row for `control` with the given Rust and Notes cells.
fn gap_row_md_with(control: &Control, rust: &str, notes: &str) -> String {
    let text = gui_ref::fidelity::normalize(control.text.as_deref().unwrap_or(""));
    format!(
        "| {} | {} | {} | {} | {} | {} |",
        md_cell(&control.name),
        md_cell(&control.kind),
        md_cell(&text),
        md_cell(&handlers_cell(control)),
        md_cell(rust),
        md_cell(notes),
    )
}

/// The markdown table header for every window section.
const GAP_HEADER: &str =
    "| Control | Kind | Text | C# handlers | Rust | Notes |\n|---|---|---|---|---|---|\n";

/// The `gaps.md` header and path-convention preamble.
fn render_header(out: &mut String) {
    out.push_str("# GUI 1:1 gap inventory\n\n");
    out.push_str(&format!(
        "Upstream pinned at `{PINNED_SHA}`. For every control of every upstream window:\n\
         is there Rust logic behind it?\n\n\
         Rows: every checked control (visible, non-empty text, kind in the fidelity list),\n\
         every control with a subscribed C# event, and every settings field\n\
         (`ComboBox`/`TextBox`/`NumericUpDown`/`CheckBox` and AirScout's `*TextBox`/\n\
         `*UpDown`/`*ComboBox`/`PercentageControl` subclasses). The `Rust` column is\n\
         `ok <path>:<line>` where the Rust code has the same effect, `missing` where it\n\
         does not, or `not ported: <reason>` for the spec's non-goals only.\n\n\
         Paths: `airscout` paths are relative to `crates/airscout/src/` (e.g.\n\
         `ui/options/cat.rs:41`); settings fields are under `as-settings/`.\n\n"
    ));
}

/// The `## Summary` bucket a Rust cell contributes to (`""` when unfilled).
fn rust_status(rust: &str) -> &'static str {
    if rust.starts_with("ok ") {
        "ok"
    } else if rust == "missing" {
        "missing"
    } else if rust.starts_with("not ported") {
        "not ported"
    } else {
        ""
    }
}

/// Render the full `gaps.md` body from the committed reference windows, merging
/// filled Rust/Notes cells from `existing_body` (keyed by window + control) and
/// recomputing the `## Summary` table from the emitted rows. Rows are never
/// dropped: every gap-inventory control is emitted, and filled cells survive.
fn render_gaps(existing_body: &str) -> String {
    let filled = existing_filled(existing_body);

    let mut out = String::new();
    render_header(&mut out);

    let mut summary: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    for window in committed_windows() {
        let w = gui_ref::load(&window);
        out.push_str(&format!("## {window}\n\n"));
        out.push_str(GAP_HEADER);
        let mut counts = (0usize, 0usize, 0usize, 0usize);
        for control in &w.controls {
            if gap_row(control) {
                let (rust, notes) = filled
                    .get(&(window.clone(), control.name.clone()))
                    .cloned()
                    .unwrap_or_else(|| ("?".to_string(), String::new()));
                out.push_str(&gap_row_md_with(control, &rust, &notes));
                out.push('\n');
                counts.0 += 1;
                match rust_status(&rust) {
                    "ok" => counts.1 += 1,
                    "missing" => counts.2 += 1,
                    "not ported" => counts.3 += 1,
                    _ => {}
                }
            }
        }
        out.push('\n');
        summary.insert(window.clone(), counts);
    }

    out.push_str("## Summary\n\n");
    out.push_str("| Window | rows | ok | missing | not ported |\n");
    out.push_str("|---|---|---|---|---|\n");
    for (window, (rows, ok, missing, not_ported)) in &summary {
        out.push_str(&format!(
            "| {window} | {rows} | {ok} | {missing} | {not_ported} |\n"
        ));
    }
    out
}

/// Generates `docs/gui-reference/gaps.md` by merging filled Rust/Notes cells
/// from the existing file (if any) into a fresh render that recomputes the
/// Summary table. Never clobbers filled work: existing non-`?` Rust cells (and
/// their Notes) are kept, no row is dropped, and the `## Summary` table is
/// recomputed from the rows.
#[test]
#[ignore]
fn generate_gaps_skeleton() {
    let path = gaps_path();
    let existing = if path.exists() {
        std::fs::read_to_string(&path).expect("read gaps.md")
    } else {
        String::new()
    };
    let out = render_gaps(&existing);
    std::fs::write(&path, out).expect("write gaps.md");
}

/// The filled Rust cells of a `gaps.md` body, keyed by `(window, control name)`:
/// the Rust and Notes cells of every row whose Rust cell is already filled.
fn existing_filled(body: &str) -> BTreeMap<(String, String), (String, String)> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for line in body.lines() {
        if line == "## Summary" {
            break;
        }
        if let Some(name) = line.strip_prefix("## ") {
            section = name.trim().to_string();
            continue;
        }
        if !line.starts_with('|') {
            continue;
        }
        if line.starts_with("| Control ") || line.trim_start().starts_with("|---") {
            continue;
        }
        let cells = split_cells(line);
        let control = cells
            .get(1)
            .map(|c| c.trim().to_string())
            .unwrap_or_default();
        let rust = cells
            .get(5)
            .map(|c| c.trim().to_string())
            .unwrap_or_default();
        let notes = cells
            .get(6)
            .map(|c| c.trim().to_string())
            .unwrap_or_default();
        if rust.is_empty() || rust == "?" {
            continue;
        }
        out.insert((section.clone(), control), (rust, notes));
    }
    out
}

/// The `## <Window>` sections present in a `gaps.md` body.
fn section_windows(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|l| l.strip_prefix("## ").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && s != "Summary")
        .collect()
}

/// The markdown table rows (excluding the header and separator rows) of `body`,
/// stopping at the `## Summary` table (which is not a window section).
fn table_rows(body: &str) -> Vec<String> {
    let mut rows = Vec::new();
    for line in body.lines() {
        if line == "## Summary" {
            break;
        }
        if !line.starts_with('|') {
            continue;
        }
        if line.starts_with("| Control ") || line.trim_start().starts_with("|---") {
            continue;
        }
        rows.push(line.to_string());
    }
    rows
}

/// Split a markdown table row into cells on unescaped `|` only (`\|` is a
/// literal pipe, kept as part of its cell).
fn split_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('|') => cur.push('|'),
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                }
                None => cur.push('\\'),
            },
            '|' => {
                cells.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// The Rust cell (6th column) of a markdown table row.
fn rust_cell(row: &str) -> String {
    split_cells(row).get(5).cloned().unwrap_or_default()
}

/// Every window in `docs/gui-reference/*.json` has a `## <Window>` section and
/// no Rust cell is empty or `?`.
#[test]
fn gaps_covers_every_reference_window() {
    let body =
        std::fs::read_to_string(gaps_path()).expect("gaps.md exists (run generate_gaps_skeleton)");

    let sections = section_windows(&body);
    let windows = committed_windows();
    for window in &windows {
        assert!(
            sections.iter().any(|s| s == window),
            "gaps.md has no `## {window}` section"
        );
    }

    for row in table_rows(&body) {
        let rust = rust_cell(&row);
        assert!(
            !rust.is_empty() && rust != "?",
            "unfilled Rust cell in row: {row}"
        );
    }
}

// --- Fix round 1: one test per review finding. ---

/// Finding (R9): filled cells survive regeneration (merge, not clobber).
#[test]
fn merge_preserves_filled_cells() {
    // A filled Rust cell for a real reference control must survive regeneration.
    let existing = format!(
        "## MapDlg\n\n{GAP_HEADER}| cb_Band | ComboBox |  |  | ok ui/mod.rs:107 | band note |\n"
    );
    let out = render_gaps(&existing);
    let row = table_rows(&out)
        .into_iter()
        .find(|r| split_cells(r).get(1).map(|c| c.as_str()) == Some("cb_Band"))
        .expect("cb_Band row present");
    assert_eq!(rust_cell(&row), "ok ui/mod.rs:107");
    assert_eq!(
        split_cells(&row).get(6).map(|c| c.as_str()),
        Some("band note")
    );
}

/// Finding (R9): the `## Summary` table is recomputed from the rows, not
/// preserved verbatim, so stale counts are replaced.
#[test]
fn summary_recomputed_after_merge() {
    let existing = format!(
        "## MapDlg\n\n{GAP_HEADER}| cb_Band | ComboBox |  |  | ok ui/mod.rs:107 |  |\n\n\
         ## Summary\n\n\
         | Window | rows | ok | missing | not ported |\n\
         |---|---|---|---|---|\n\
         | MapDlg | 999 | 999 | 0 | 0 |\n"
    );
    let out = render_gaps(&existing);

    let mapdlg_rows = {
        let mut in_map = false;
        let mut n = 0;
        for line in out.lines() {
            if line == "## MapDlg" {
                in_map = true;
                continue;
            }
            if in_map && line.starts_with("## ") {
                break;
            }
            if in_map
                && line.starts_with('|')
                && !line.starts_with("| Control ")
                && !line.trim_start().starts_with("|---")
            {
                n += 1;
            }
        }
        n
    };

    let summary = out[out.find("## Summary").expect("summary present")..].to_string();
    assert!(
        !summary.contains("999"),
        "stale summary count must be recomputed: {summary}"
    );
    // Only cb_Band is filled, so MapDlg's ok count is 1 and the rest are `?`.
    assert!(
        summary.contains(&format!("| MapDlg | {mapdlg_rows} | 1 | 0 | 0 |")),
        "summary must reflect merged rows: {summary}"
    );
}

/// Finding (Important #1): filled cells are read per `(window, control)` and `?`
/// cells are ignored, so regeneration keeps existing work and only adds rows.
#[test]
fn existing_filled_reads_only_filled_cells() {
    let body = format!(
        "## MapDlg\n\n{GAP_HEADER}| cb_Band | ComboBox |  |  | ok ui/mod.rs:107 | a note |\n| tb_X | TextBox |  |  | ? |  |\n"
    );
    let m = existing_filled(&body);
    assert_eq!(
        m.get(&("MapDlg".to_string(), "cb_Band".to_string())),
        Some(&("ok ui/mod.rs:107".to_string(), "a note".to_string()))
    );
    assert!(!m.contains_key(&("MapDlg".to_string(), "tb_X".to_string())));
}

/// Finding (Important #2): the Summary table is not a window section and must be
/// excluded from `table_rows`.
#[test]
fn table_rows_stops_at_summary() {
    let body = format!(
        "## MapDlg\n\n{GAP_HEADER}| cb_Band | ComboBox |  |  | ok ui/mod.rs:107 |  |\n\n## Summary\n\n| Window | rows | ok | missing | not ported |\n|---|---|---|---|---|\n| MapDlg | 1 | 1 | 0 | 0 |\n"
    );
    let rows = table_rows(&body);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("cb_Band"));
}

/// Finding (Important #2): an escaped `\|` in a cell must not shift the Rust
/// column when `rust_cell` splits the row.
#[test]
fn rust_cell_splits_on_unescaped_pipe_only() {
    let row = "| label94 | Label | km \\| mi |  | ok ui/options/planes.rs:27 |  |";
    assert_eq!(rust_cell(row), "ok ui/options/planes.rs:27");
}

/// Finding (Important #4, controller ruling R7): AirScout's custom
/// settings-input subclasses get a row even with no subscribed event.
#[test]
fn settings_input_subclasses_are_settings_rows() {
    let src = r#"
        this.a = new ScoutBase.Core.Int32TextBox();
        this.b = new ScoutBase.Core.DoubleTextBox();
        this.c = new ScoutBase.Core.CallsignTextBox();
        this.d = new ScoutBase.Core.PercentageControl();
        this.e = new ScoutBase.Core.LongTextBox();
        this.f = new ScoutBase.Core.LocatorComboBox();
        this.Controls.Add(this.a);
        this.Controls.Add(this.b);
        this.Controls.Add(this.c);
        this.Controls.Add(this.d);
        this.Controls.Add(this.e);
        this.Controls.Add(this.f);
    "#;
    let w = parse_src(src);
    for name in ["a", "b", "c", "d", "e", "f"] {
        assert!(gap_row(control(&w, name)), "{name} must be a settings row");
    }
    let w2 = parse_src("this.l = new System.Windows.Forms.Label(); this.Controls.Add(this.l);");
    assert!(!gap_row(control(&w2, "l")));
    // `RichTextBox` is a standard read-only display control, not a settings field.
    let w3 =
        parse_src("this.r = new System.Windows.Forms.RichTextBox(); this.Controls.Add(this.r);");
    assert!(
        !gap_row(control(&w3, "r")),
        "RichTextBox must not be a settings row"
    );
}
