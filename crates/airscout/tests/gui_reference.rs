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
