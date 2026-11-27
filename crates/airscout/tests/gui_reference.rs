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

/// Path of the gap inventory `docs/gui-reference/gaps.md`.
fn gaps_path() -> PathBuf {
    reference_dir().join("gaps.md")
}

/// Whether `control` gets a row in the gap inventory: a checked control
/// (Task 5 rule), a control with a subscribed event, or a settings field.
fn gap_row(control: &Control) -> bool {
    gui_ref::fidelity::is_checked(control)
        || !control.events.is_empty()
        || SETTINGS_KINDS.contains(&control.kind.as_str())
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

/// One markdown row for `control` (Rust column `?`, Notes empty).
fn gap_row_md(control: &Control) -> String {
    let text = gui_ref::fidelity::normalize(control.text.as_deref().unwrap_or(""));
    format!(
        "| {} | {} | {} | {} | ? |  |",
        md_cell(&control.name),
        md_cell(&control.kind),
        md_cell(&text),
        md_cell(&handlers_cell(control)),
    )
}

/// Generates `docs/gui-reference/gaps.md`: one `## <Window>` section per
/// reference window with a row per gap-inventory control, Rust column set to
/// `?`. Refuses to overwrite a file whose Rust column is already filled.
#[test]
#[ignore]
fn generate_gaps_skeleton() {
    let path = gaps_path();
    if path.exists() {
        let existing = std::fs::read_to_string(&path).expect("read gaps.md");
        if !has_unfilled_rust(&existing) {
            panic!("gaps.md already has filled Rust values; refusing to overwrite");
        }
    }

    let mut out = String::new();
    out.push_str("# GUI 1:1 gap inventory\n\n");
    out.push_str(&format!(
        "Upstream pinned at `{PINNED_SHA}`. For every control of every upstream window:\n\
         is there Rust logic behind it?\n\n\
         Rows: every checked control (visible, non-empty text, kind in the fidelity list),\n\
         every control with a subscribed C# event, and every settings field\n\
         (`ComboBox`/`TextBox`/`NumericUpDown`/`CheckBox`). The `Rust` column is\n\
         `ok <path>:<line>` where the Rust code has the same effect, `missing` where it\n\
         does not, or `not ported: <reason>` for the spec's non-goals only.\n\n"
    ));

    for window in committed_windows() {
        let w = gui_ref::load(&window);
        out.push_str(&format!("## {window}\n\n"));
        out.push_str("| Control | Kind | Text | C# handlers | Rust | Notes |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for control in &w.controls {
            if gap_row(control) {
                out.push_str(&gap_row_md(control));
                out.push('\n');
            }
        }
        out.push('\n');
    }

    std::fs::write(&path, out).expect("write gaps.md");
}

/// Whether a `gaps.md` body still has an unfilled Rust column (a `?` or empty
/// cell). Used by the generator to avoid clobbering filled work.
fn has_unfilled_rust(body: &str) -> bool {
    for row in table_rows(body) {
        let rust = rust_cell(&row);
        if rust.is_empty() || rust == "?" {
            return true;
        }
    }
    false
}

/// The `## <Window>` sections present in a `gaps.md` body.
fn section_windows(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|l| l.strip_prefix("## ").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && s != "Summary")
        .collect()
}

/// The markdown table rows (excluding the header and separator rows) of `body`.
fn table_rows(body: &str) -> Vec<String> {
    body.lines()
        .filter(|l| l.starts_with('|'))
        .filter(|l| !l.starts_with("| Control ") && !l.trim_start().starts_with("|---"))
        .map(|l| l.to_string())
        .collect()
}

/// The Rust cell (6th column) of a markdown table row.
fn rust_cell(row: &str) -> String {
    let cells: Vec<&str> = row.split('|').collect();
    cells
        .get(5)
        .map(|c| c.trim().to_string())
        .unwrap_or_default()
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
