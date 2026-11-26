//! Task 1 — parse WinForms `*.Designer.cs` files into a reference model.
//!
//! Each test parses a small synthetic Designer snippet and checks the fields the
//! brief specifies. Synthetic snippets only (no upstream text is copied here).

mod gui_ref;

use gui_ref::designer::parse;
use gui_ref::model::{Control, Event, Window};
use gui_ref::resx::lookup;

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
        this.l.Text = "A 1 arcsec\r\nCoverage: 58°S" +
            " ... 60°N";
        "#,
    );
    assert_eq!(
        control(&w, "l").text.as_deref(),
        Some("A 1 arcsec\r\nCoverage: 58°S ... 60°N")
    );

    // Escaped quote and backslash.
    let w = parse_src(r#"this.l.Text = "say \"hi\" \\ ok";"#);
    assert_eq!(control(&w, "l").text.as_deref(), Some("say \"hi\" \\ ok"));

    // Verbatim string (backslash is literal).
    let w = parse_src(r#"this.l.Text = @"C:\x";"#);
    assert_eq!(control(&w, "l").text.as_deref(), Some("C:\\x"));
}

#[test]
fn resolves_resource_strings() {
    let resx =
        "<data name=\"l.Text\" xml:space=\"preserve\"><value>a &amp; b&#xD;\nc</value></data>";
    let src = r#"this.l.Text = resources.GetString("l.Text");"#;
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
        r#"this.l.Text = resources.GetString("k" );"#,
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
    "#;
    let w = parse_src(src);
    assert_eq!(
        control(&w, "cb").items,
        vec!["ab".to_string(), "c".to_string()]
    );
    assert!(w.unparsed.is_empty());
}
