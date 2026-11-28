//! Reference model of a parsed WinForms `*.Designer.cs` file.
//!
//! These types mirror the upstream C# control tree so the Rust UI can be checked
//! 1:1 against it. The schema is stable across tasks; the generator (Task 2)
//! serialises it with serde.

use serde::{Deserialize, Serialize};

/// One parsed window (one `*.Designer.cs` file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// Window / form name, e.g. `MapDlg`.
    pub window: String,
    /// Upstream commit the Designer file was extracted from.
    pub upstream_sha: String,
    /// `this.Text` set on the form itself.
    pub title: Option<String>,
    /// `this.ClientSize` set on the form itself.
    pub client_size: Option<[i32; 2]>,
    /// Every control declared in `InitializeComponent`, in declaration order.
    pub controls: Vec<Control>,
    /// Statements the parser recognised but could not read.
    pub unparsed: Vec<String>,
}

impl Window {
    /// An empty window with the given name.
    pub fn new(window: &str) -> Window {
        Window {
            window: window.to_string(),
            upstream_sha: String::new(),
            title: None,
            client_size: None,
            controls: Vec::new(),
            unparsed: Vec::new(),
        }
    }
}

/// A single control: a label, group box, column header, custom type, …
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Control {
    /// Designer field name (`this.<name>`).
    pub name: String,
    /// WinForms type name (last path segment of `new A.B.Type(...)`).
    pub kind: String,
    /// `"$this"`, a control name, or e.g. `"sc_Main.Panel1"`.
    pub parent: String,
    /// Order within the parent's `Controls`/`Items`/`Columns`.
    pub index: usize,
    /// Visible `Text` after string joins and resource lookup.
    pub text: Option<String>,
    /// `Location`.
    pub location: Option<[i32; 2]>,
    /// `Size`.
    pub size: Option<[i32; 2]>,
    /// `"Top" | "Bottom" | "Left" | "Right" | "Fill"`.
    pub dock: Option<String>,
    /// Set of `AnchorStyles` names.
    pub anchor: Option<Vec<String>>,
    /// `TabIndex`.
    pub tab_index: Option<i32>,
    /// `Visible` (defaults to `true`).
    pub visible: bool,
    /// `Enabled` (defaults to `true`).
    pub enabled: bool,
    /// `SplitContainer.SplitterDistance`.
    pub splitter_distance: Option<i32>,
    /// `"Horizontal" | "Vertical"`.
    pub orientation: Option<String>,
    /// `ComboBox`/`ListBox` string items set in the Designer.
    pub items: Vec<String>,
    /// Subscribed event handlers.
    pub events: Vec<Event>,
}

impl Control {
    /// A control with the given name and all Designer defaults applied.
    pub fn new(name: &str) -> Control {
        Control {
            name: name.to_string(),
            kind: String::new(),
            parent: "$this".to_string(),
            index: 0,
            text: None,
            location: None,
            size: None,
            dock: None,
            anchor: None,
            tab_index: None,
            visible: true,
            enabled: true,
            splitter_distance: None,
            orientation: None,
            items: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// One subscribed event on a control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Event name (`Click`, `Paint`, …).
    pub event: String,
    /// Handler method name (`b_Click`, …).
    pub handler: String,
}
