//! WinForms Designer parser: turns an `InitializeComponent` body into the
//! reference [`model::Window`].
//!
//! It understands only the statement shapes the Designer emits and reports the
//! shapes it recognises but cannot read in [`model::Window::unparsed`].

use super::model::{Control, Event, Window};
use std::collections::{HashMap, HashSet};

/// Parse a Designer source into a reference window.
///
/// `src` may be the full `*.Designer.cs` text or just the body of
/// `InitializeComponent`. `resx` resolves `resources.GetString("…")` keys.
///
/// Components that are never attached to a parent (`BackgroundWorker`, `Timer`,
/// `ImageList`, `ToolTip`, …) are not controls and are dropped from
/// [`model::Window::controls`].
pub fn parse(window: &str, src: &str, resx: &dyn Fn(&str) -> Option<String>) -> Window {
    let body = initialize_component_body(src);
    let statements = split_statements(body);
    let mut parser = Parser::new(window, resx);
    for stmt in &statements {
        parser.handle_statement(stmt);
    }
    let mut window = parser.window;
    window
        .controls
        .retain(|c| parser.parented.contains(&c.name));
    window
}

struct Parser<'a> {
    window: Window,
    by_name: HashMap<String, usize>,
    counters: HashMap<String, usize>,
    /// Control names that were explicitly attached to a parent via `Controls.Add`,
    /// `Pages.Add`, `Items.AddRange` or `Columns.AddRange`.
    parented: HashSet<String>,
    resx: &'a dyn Fn(&str) -> Option<String>,
}

impl<'a> Parser<'a> {
    fn new(window: &str, resx: &'a dyn Fn(&str) -> Option<String>) -> Self {
        Parser {
            window: Window::new(window),
            by_name: HashMap::new(),
            counters: HashMap::new(),
            parented: HashSet::new(),
            resx,
        }
    }

    /// Fetch the control named `name`, creating it with defaults if absent.
    fn control_mut(&mut self, name: &str) -> &mut Control {
        if let Some(&idx) = self.by_name.get(name) {
            return &mut self.window.controls[idx];
        }
        let idx = self.window.controls.len();
        self.window.controls.push(Control::new(name));
        self.by_name.insert(name.to_string(), idx);
        &mut self.window.controls[idx]
    }

    fn handle_statement(&mut self, stmt: &str) {
        if !stmt.starts_with("this.") {
            return; // `System.ComponentModel…`, `((…)).BeginInit()`, …
        }
        let rest = &stmt["this.".len()..];
        if let Some(eq) = find_assignment_eq(rest) {
            let is_plus = eq > 0 && rest.as_bytes()[eq - 1] == b'+';
            let left = rest[..if is_plus { eq - 1 } else { eq }].trim();
            let right = rest[eq + 1..].trim();
            if is_plus {
                self.handle_event(left, right);
            } else {
                self.handle_assign(stmt, left, right);
            }
            return;
        }
        self.handle_method_call(stmt, rest);
    }

    fn handle_assign(&mut self, stmt: &str, left: &str, right: &str) {
        let mut parts = left.split('.');
        match (parts.next(), parts.next()) {
            // Form-level: `this.Text`, `this.ClientSize`, or a control declaration.
            (Some(name), None) => match name {
                "Text" => match self.parse_string_expr(right) {
                    Some(s) => self.window.title = Some(s),
                    None => self.unparsed(stmt),
                },
                "ClientSize" => {
                    if let Some(size) = parse_size(right) {
                        self.window.client_size = Some(size);
                    }
                }
                _ => {
                    if let Some(kind) = kind_from_new(right) {
                        if !is_non_control_kind(&kind) {
                            self.control_mut(name).kind = kind;
                        }
                    }
                }
            },
            // Control property: `this.x.Prop = value`.
            (Some(name), Some(prop)) => {
                if parts.next().is_some() {
                    return; // nested path (e.g. `sc_Main.Panel1.BackColor`) — ignored
                }
                self.handle_control_prop(stmt, name, prop, right);
            }
            (None, _) => {}
        }
    }

    fn handle_control_prop(&mut self, stmt: &str, name: &str, prop: &str, value: &str) {
        match prop {
            "Text" => {
                if is_settings_binding(value) {
                    return; // runtime-bound at startup — no static text
                }
                if is_user_input_kind(&self.control_mut(name).kind) {
                    return; // user-input default values are not part of the reference
                }
                match self.parse_string_expr(value) {
                    Some(s) => self.control_mut(name).text = Some(s),
                    None => self.unparsed(stmt),
                }
            }
            "Location" => match parse_point(value) {
                Some(v) => self.control_mut(name).location = Some(v),
                None => self.unparsed(stmt),
            },
            "Size" => match parse_size(value) {
                Some(v) => self.control_mut(name).size = Some(v),
                None => self.unparsed(stmt),
            },
            "Dock" => match parse_dock(value) {
                Some(v) => self.control_mut(name).dock = Some(v),
                None => self.unparsed(stmt),
            },
            "Anchor" => match parse_anchor(value) {
                Some(v) => self.control_mut(name).anchor = Some(v),
                None => self.unparsed(stmt),
            },
            "Visible" => match parse_bool(value) {
                Some(v) => self.control_mut(name).visible = v,
                None => self.unparsed(stmt),
            },
            "Enabled" => {
                if is_settings_binding(value) {
                    return; // runtime-bound at startup — keeps default `true`
                }
                match parse_bool(value) {
                    Some(v) => self.control_mut(name).enabled = v,
                    None => self.unparsed(stmt),
                }
            }
            "TabIndex" => match parse_i32(value) {
                Some(v) => self.control_mut(name).tab_index = Some(v),
                None => self.unparsed(stmt),
            },
            "SplitterDistance" => match parse_i32(value) {
                Some(v) => self.control_mut(name).splitter_distance = Some(v),
                None => self.unparsed(stmt),
            },
            "Orientation" => match parse_orientation(value) {
                Some(v) => self.control_mut(name).orientation = Some(v),
                None => self.unparsed(stmt),
            },
            _ => {} // any other property — ignored silently
        }
    }

    fn handle_event(&mut self, left: &str, right: &str) {
        let Some((name, event)) = left.rsplit_once('.') else {
            return; // form-level event (`this.Load`, `this.FormClosing`) — ignored
        };
        let Some(handler) = extract_handler(right) else {
            return;
        };
        self.control_mut(name).events.push(Event {
            event: event.to_string(),
            handler,
        });
    }

    fn handle_method_call(&mut self, stmt: &str, rest: &str) {
        if let Some(pos) = rest.find("Controls.Add(") {
            self.handle_controls_add(stmt, rest, pos, "Controls.Add(");
        } else if let Some(pos) = rest.find(".Pages.Add(") {
            self.handle_controls_add(stmt, rest, pos, ".Pages.Add(");
        } else if let Some(pos) = rest.find(".Items.AddRange(") {
            self.handle_addrange_at(stmt, rest, pos, ".Items.AddRange(", false);
        } else if let Some(pos) = rest.find(".Columns.AddRange(") {
            self.handle_addrange_at(stmt, rest, pos, ".Columns.AddRange(", true);
        } else if find_outside_strings(rest, ".AddRange(").is_some() {
            self.unparsed(stmt);
        }
        // `SuspendLayout` / `ResumeLayout` / `PerformLayout` / `DataBindings.Add` — ignored
    }

    fn handle_controls_add(&mut self, stmt: &str, rest: &str, pos: usize, needle: &str) {
        let parent_path = rest[..pos].trim().trim_end_matches('.');
        let parent = if parent_path.is_empty() {
            "$this".to_string()
        } else {
            parent_path.to_string()
        };
        let open = pos + needle.len() - 1;
        let Some(close) = matching_paren(rest, open) else {
            self.unparsed(stmt);
            return;
        };
        let arg = rest[open + 1..close].trim();
        let Some(child) = arg.strip_prefix("this.") else {
            self.unparsed(stmt);
            return;
        };
        let child = child.trim();
        if !is_identifier(child) {
            self.unparsed(stmt);
            return;
        }
        let index = self.next_index(&parent);
        let c = self.control_mut(child);
        c.parent = parent;
        c.index = index;
        self.parented.insert(child.to_string());
    }

    fn handle_addrange_at(
        &mut self,
        stmt: &str,
        rest: &str,
        pos: usize,
        needle: &str,
        is_columns: bool,
    ) {
        let parent = rest[..pos].to_string();
        let open = pos + needle.len() - 1;
        let Some(content) = paren_content(rest, open) else {
            self.unparsed(stmt);
            return;
        };
        self.handle_addrange(stmt, &parent, &content, is_columns);
    }

    fn handle_addrange(&mut self, stmt: &str, parent: &str, content: &str, is_columns: bool) {
        match parse_addrange(content, is_columns, self.resx) {
            Some(AddRange::Items(items)) => self.control_mut(parent).items = items,
            Some(AddRange::Children(names)) => {
                let start = self.counters.get(parent).copied().unwrap_or(0);
                for (i, name) in names.iter().enumerate() {
                    let c = self.control_mut(name);
                    c.parent = parent.to_string();
                    c.index = start + i;
                    self.parented.insert(name.clone());
                }
                self.counters
                    .insert(parent.to_string(), start + names.len());
            }
            None => self.unparsed(stmt),
        }
    }

    fn next_index(&mut self, parent: &str) -> usize {
        let entry = self.counters.entry(parent.to_string()).or_insert(0);
        let index = *entry;
        *entry += 1;
        index
    }

    fn parse_string_expr(&self, expr: &str) -> Option<String> {
        parse_string_expr(expr, self.resx)
    }

    fn unparsed(&mut self, stmt: &str) {
        self.window.unparsed.push(stmt.to_string());
    }
}

enum AddRange {
    Items(Vec<String>),
    Children(Vec<String>),
}

/// Extract only the body of `InitializeComponent` (the statements between its
/// braces). Falls back to `src` unchanged when the method is not present, so
/// callers may pass a bare snippet.
fn initialize_component_body(src: &str) -> &str {
    let Some(pos) = src.find("InitializeComponent") else {
        return src;
    };
    let Some(open_rel) = src[pos..].find('{') else {
        return src;
    };
    let open = pos + open_rel;
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    src
}

/// Strip `//` line comments (outside string literals), preserving newlines.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = skip_normal_string(bytes, i);
                out.push_str(&src[i..end]);
                i = end;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                let end = skip_verbatim_string(bytes, i);
                out.push_str(&src[i..end]);
                i = end;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    out.push('\n');
                    i += 1;
                }
            }
            _ => {
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

/// Split into statements on `;`, ignoring `;` inside string literals (normal and
/// verbatim `@"…"`, where `""` is an escaped quote).
fn split_statements(src: &str) -> Vec<String> {
    let stripped = strip_comments(src);
    let bytes = stripped.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            b';' => {
                let stmt = stripped[start..i].trim();
                if !stmt.is_empty() {
                    out.push(stmt.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let stmt = stripped[start..].trim();
    if !stmt.is_empty() {
        out.push(stmt.to_string());
    }
    out
}

/// Skip a normal `"…"` string literal; `start` is the opening quote. Returns the
/// index just past the closing quote (or `len` if unterminated).
fn skip_normal_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skip a verbatim `@"…"` string literal; `start` is the `@`. Returns the index
/// just past the closing quote (or `len` if unterminated).
fn skip_verbatim_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// Find the first byte index of `needle` in `s`, ignoring occurrences inside
/// normal and verbatim string literals.
fn find_outside_strings(s: &str, needle: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let needle = needle.as_bytes();
    let mut i = 0usize;
    while i + needle.len() <= bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            _ => {
                if &bytes[i..i + needle.len()] == needle {
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
}

/// Find the `=` (or `+=`) of an assignment/event, ignoring `=` inside string
/// literals and stopping at the first `(`. A Designer assignment/event LHS
/// (`this.<name>[.Prop]`) never contains `(`, so the first `(` marks a method
/// call such as `this.cb.Items.AddRange(new object[] { "A = B" })`.
fn find_assignment_eq(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            b'(' => return None,
            b'=' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Parse a normal `"…"` string literal (leading `"`). Returns the decoded text
/// and the number of bytes consumed.
fn parse_normal_string(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let mut i = 1usize;
    let mut out = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some((out, i + 1)),
            b'\\' => {
                if i + 1 >= bytes.len() {
                    return None;
                }
                let e = bytes[i + 1];
                match e {
                    b'0' => out.push('\0'),
                    b'a' => out.push('\u{7}'),
                    b'b' => out.push('\u{8}'),
                    b't' => out.push('\t'),
                    b'n' => out.push('\n'),
                    b'v' => out.push('\u{b}'),
                    b'f' => out.push('\u{c}'),
                    b'r' => out.push('\r'),
                    b'\\' => out.push('\\'),
                    b'\'' => out.push('\''),
                    b'"' => out.push('"'),
                    b'x' => {
                        let mut j = i + 2;
                        let mut value = 0u32;
                        let mut n = 0;
                        while j < bytes.len() && n < 4 && (bytes[j] as char).is_ascii_hexdigit() {
                            value = value * 16 + (bytes[j] as char).to_digit(16).unwrap();
                            j += 1;
                            n += 1;
                        }
                        if n == 0 {
                            return None;
                        }
                        out.push(char::from_u32(value)?);
                        i = j;
                        continue;
                    }
                    b'u' => {
                        let hex = s.get(i + 2..i + 6)?;
                        out.push(char::from_u32(u32::from_str_radix(hex, 16).ok()?)?);
                        i += 6;
                        continue;
                    }
                    b'U' => {
                        let hex = s.get(i + 2..i + 10)?;
                        out.push(char::from_u32(u32::from_str_radix(hex, 16).ok()?)?);
                        i += 10;
                        continue;
                    }
                    _ => out.push(e as char),
                }
                i += 2;
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    None
}

/// Parse a verbatim `@"…"` string literal (leading `@"`), where `""` is an
/// escaped quote. Returns the text and the number of bytes consumed.
fn parse_verbatim_string(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let mut i = 2usize;
    let mut out = String::new();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                out.push('"');
                i += 2;
            } else {
                return Some((out, i + 1));
            }
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    None
}

/// Parse a `Text` value expression: a run of string literals and
/// `resources.GetString("…")` calls joined by `+`.
fn parse_string_expr(expr: &str, resx: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = expr.trim();
    let mut found = false;
    loop {
        rest = rest.trim_start();
        if rest.starts_with('+') {
            rest = rest[1..].trim_start();
            continue;
        }
        if rest.is_empty() {
            break;
        }
        if rest.starts_with("@\"") {
            let (s, used) = parse_verbatim_string(rest)?;
            out.push_str(&s);
            rest = &rest[used..];
        } else if rest.starts_with('"') {
            let (s, used) = parse_normal_string(rest)?;
            out.push_str(&s);
            rest = &rest[used..];
        } else {
            let inner = rest.strip_prefix("resources.GetString(")?;
            let inner = inner.trim_start();
            if !inner.starts_with('"') {
                return None;
            }
            let (key, used) = parse_normal_string(inner)?;
            let after_key = inner[used..].trim_start();
            let after_paren = after_key.strip_prefix(')')?;
            out.push_str(&resx(&key)?);
            rest = after_paren;
        }
        found = true;
    }
    found.then_some(out)
}

/// Extract the type name (last path segment) from a `new A.B.Type(...)` RHS.
fn kind_from_new(rhs: &str) -> Option<String> {
    let idx = find_outside_strings(rhs, "new ")?;
    let after = &rhs[idx + "new ".len()..];
    let type_path: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let last = type_path.split('.').next_back()?;
    (!last.is_empty()).then(|| last.to_string())
}

/// Value types and other non-control types assigned on the form itself
/// (`this.AutoScaleDimensions = new …SizeF`, `this.components = new …Container`,
/// `this.MinimumSize = new …Size`, …). These are not UI controls and must not
/// become entries in [`model::Window::controls`].
fn is_non_control_kind(kind: &str) -> bool {
    matches!(
        kind,
        "SizeF" | "Size" | "Point" | "Font" | "Padding" | "Container"
    )
}

/// Text-entry controls (`TextBox` and its `ScoutBase.Core` subclasses). Their
/// Designer default values are user input (callsigns, locators, coordinates),
/// not labels, so the reference records no `Text` for them.
fn is_user_input_kind(kind: &str) -> bool {
    matches!(
        kind,
        "TextBox"
            | "MaskedTextBox"
            | "DoubleTextBox"
            | "Int32TextBox"
            | "LongTextBox"
            | "LocatorTextBox"
            | "CallsignTextBox"
    )
}

/// Parse `new <…>.Point(a, b)` into `[a, b]`.
fn parse_point(expr: &str) -> Option<[i32; 2]> {
    parse_new_int_pair(expr, "Point")
}

/// Parse `new <…>.Size(a, b)` into `[a, b]`.
fn parse_size(expr: &str) -> Option<[i32; 2]> {
    parse_new_int_pair(expr, "Size")
}

fn parse_new_int_pair(expr: &str, type_name: &str) -> Option<[i32; 2]> {
    let idx = expr.find("new ")?;
    let after = &expr[idx + "new ".len()..];
    let type_path: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    if type_path.split('.').next_back() != Some(type_name) {
        return None;
    }
    let open = expr.find('(')?;
    let close = expr.rfind(')')?;
    if close < open {
        return None;
    }
    let inner = &expr[open + 1..close];
    let mut it = inner.split(',');
    let a: i32 = it.next()?.trim().parse().ok()?;
    let b: i32 = it.next()?.trim().parse().ok()?;
    Some([a, b])
}

/// Parse `…DockStyle.<Name>` into `"<Name>"`.
fn parse_dock(expr: &str) -> Option<String> {
    let idx = expr.find("DockStyle.")?;
    let after = &expr[idx + "DockStyle.".len()..];
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Parse `…Orientation.<Name>` into `"<Name>"`.
fn parse_orientation(expr: &str) -> Option<String> {
    let idx = expr.find("Orientation.")?;
    let after = &expr[idx + "Orientation.".len()..];
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Parse every `AnchorStyles.<Name>` in order into a name list.
fn parse_anchor(expr: &str) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut rest = expr;
    while let Some(idx) = rest.find("AnchorStyles.") {
        let after = &rest[idx + "AnchorStyles.".len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            break;
        }
        names.push(name);
        rest = after;
    }
    (!names.is_empty()).then_some(names)
}

fn parse_bool(expr: &str) -> Option<bool> {
    match expr.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_i32(expr: &str) -> Option<i32> {
    expr.trim().parse().ok()
}

/// True when `value` is a runtime-bound application setting
/// (`global::AirScout.Properties.Settings.Default.<Key>` or
/// `Properties.Settings.Default.<Key>`). These are resolved at startup, so the
/// parser records no static value instead of flagging them as unreadable.
fn is_settings_binding(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("global::AirScout.Properties.Settings.Default.")
        || v.starts_with("Properties.Settings.Default.")
}

/// Parse an `AddRange` argument: `new <type>[] { … }`.
fn parse_addrange(
    content: &str,
    is_columns: bool,
    resx: &dyn Fn(&str) -> Option<String>,
) -> Option<AddRange> {
    let idx = content.find("new ")?;
    let after = &content[idx + "new ".len()..];
    let array_type: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let open = content.find('{')?;
    let close = content.rfind('}')?;
    if close < open {
        return None;
    }
    let inner = &content[open + 1..close];
    let elements = split_top_level_commas(inner);
    let is_items = !is_columns && array_type.split('.').next_back() == Some("object");
    if is_items {
        let mut strings = Vec::with_capacity(elements.len());
        for el in &elements {
            strings.push(parse_string_expr(el, resx)?);
        }
        Some(AddRange::Items(strings))
    } else {
        let mut names = Vec::with_capacity(elements.len());
        for el in &elements {
            let name = el.strip_prefix("this.")?.trim();
            if !is_identifier(name) {
                return None;
            }
            names.push(name.to_string());
        }
        Some(AddRange::Children(names))
    }
}

/// Split on top-level commas (outside string literals).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            b',' => {
                let el = s[start..i].trim();
                if !el.is_empty() {
                    out.push(el.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let el = s[start..].trim();
    if !el.is_empty() {
        out.push(el.to_string());
    }
    out
}

/// Find the `)` matching the `(` at `open`, skipping string literals.
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_normal_string(bytes, i);
                continue;
            }
            b'@' if i + 1 < bytes.len() && bytes[i + 1] == b'"' => {
                i = skip_verbatim_string(bytes, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Return the text between the `(` at `open` and its matching `)`.
fn paren_content(s: &str, open: usize) -> Option<String> {
    let close = matching_paren(s, open)?;
    Some(s[open + 1..close].to_string())
}

/// Extract the handler name from `new …(this.<handler>)`.
fn extract_handler(right: &str) -> Option<String> {
    let idx = right.find("this.")?;
    let after = &right[idx + "this.".len()..];
    let ident: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!ident.is_empty()).then_some(ident)
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
