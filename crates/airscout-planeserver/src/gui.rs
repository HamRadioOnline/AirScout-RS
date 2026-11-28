//! The planeserver window (C# `MainDlg`): tabs Status, Feeds, Area, Server and General, with
//! Apply and Revert, and the tray icon of [`crate::tray`].
//!
//! The window edits the values of the config file. Command line overrides (`--bind`,
//! `--port`, `--data`) stay in force for this run and are never saved.

use crate::config::{feed_needs_disclaimer, PlaneServerConfig, DEFAULT_PORT, MAX_FEEDS};
use crate::service::{make_feeds, Service, Status};
use crate::tray::{icon_rgba, Tray, TrayCommand, TrayHandle, ICON_SIZE};
use chrono::{DateTime, Utc};
use eframe::egui::{
    self, CentralPanel, ComboBox, DragValue, Grid, Panel, ScrollArea, TextEdit, ViewportCommand,
};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use toml::{Table, Value};

/// Window title (`MainDlg.Designer.cs:1136`).
pub const TITLE: &str = "AirScout Plane Server";
/// Window size (`MainDlg.Designer.cs:1129`).
const SIZE: [f32; 2] = [634.0, 452.0];
/// The label of the port field. The C# spinner allows 1024..65535 (`MainDlg.Designer.cs:663`);
/// the config allows any port but 0.
pub const PORT_LABEL: &str = "Port (1...65535, default 9872):";
/// The disclaimer of a restricted feed that has no text of its own (BinCraft).
pub const FALLBACK_DISCLAIMER: &str = "This feed uses an undocumented or restricted data \
     source. Use it only if you are allowed to. Accept to enable it.";
/// The text of an empty feed slot.
const NONE: &str = "[none]";
/// Status and tray refresh (`ti_Main_Tick`, `MainDlg.cs:652`).
const REFRESH: Duration = Duration::from_secs(1);
/// The window polls the service and the tray at least this often.
const POLL: Duration = Duration::from_millis(250);
const LAN_WARNING: &str = "With bind address 0.0.0.0 the server listens on all network \
     interfaces without authentication or encryption. Anyone on your network can read the \
     plane list.";

/// The running service as the window sees it; tests use a fake.
pub trait ServiceHandle {
    fn status(&self) -> Status;
    fn stop(self: Box<Self>);
}

impl ServiceHandle for Service {
    fn status(&self) -> Status {
        Service::status(self)
    }
    fn stop(self: Box<Self>) {
        Service::stop(*self)
    }
}

/// A service that could not start: its status names the error.
struct StartFailed {
    started: DateTime<Utc>,
    error: String,
}

impl ServiceHandle for StartFailed {
    fn status(&self) -> Status {
        Status {
            started: self.started,
            planes: 0,
            feeds: Vec::new(),
            server: self.error.clone(),
            db_error: None,
        }
    }
    fn stop(self: Box<Self>) {}
}

/// Makes a service from a config.
pub type ServiceFactory = Box<dyn Fn(&PlaneServerConfig) -> Box<dyn ServiceHandle>>;

/// The real service with the feeds of `cfg`.
pub fn real_service(cfg: &PlaneServerConfig) -> Box<dyn ServiceHandle> {
    match Service::start(cfg, make_feeds(cfg)) {
        Ok(s) => Box::new(s),
        Err(error) => Box::new(StartFailed {
            started: Utc::now(),
            error,
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Status,
    Feeds,
    Area,
    Server,
    General,
}

const TABS: [(Tab, &str); 5] = [
    (Tab::Status, "Status"),
    (Tab::Feeds, "Feeds"),
    (Tab::Area, "Area"),
    (Tab::Server, "Server"),
    (Tab::General, "General"),
];

/// The command line values that differ from the file, kept for this run.
#[derive(Default)]
struct Overrides {
    bind: Option<IpAddr>,
    port: Option<u16>,
    data_dir: Option<Option<PathBuf>>,
}

impl Overrides {
    fn between(file: &PlaneServerConfig, run: &PlaneServerConfig) -> Overrides {
        Overrides {
            bind: (file.bind != run.bind).then_some(run.bind),
            port: (file.port != run.port).then_some(run.port),
            data_dir: (file.data_dir != run.data_dir).then(|| run.data_dir.clone()),
        }
    }

    fn apply(&self, cfg: &PlaneServerConfig) -> PlaneServerConfig {
        let mut c = cfg.clone();
        if let Some(b) = self.bind {
            c.bind = b;
        }
        if let Some(p) = self.port {
            c.port = p;
        }
        if let Some(d) = &self.data_dir {
            c.data_dir = d.clone();
        }
        c
    }

    /// One line per override, for the Server tab.
    fn lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(b) = self.bind {
            v.push(format!("bind address {b}"));
        }
        if let Some(p) = self.port {
            v.push(format!("port {p}"));
        }
        if let Some(d) = &self.data_dir {
            match d {
                Some(d) => v.push(format!("data dir {}", d.display())),
                None => v.push("default data dir".to_owned()),
            }
        }
        v
    }
}

/// What the Feeds tab needs of one feed.
struct FeedInfo {
    name: &'static str,
    disclaimer: Option<&'static str>,
    needs_disclaimer: bool,
    secret_keys: &'static [&'static str],
    defaults: Table,
}

/// The disclaimer dialog of the feed just picked in `slot`.
struct Disclaimer {
    slot: usize,
    feed: String,
    text: String,
}

pub struct PlaneServerApp {
    path: PathBuf,
    draft: PlaneServerConfig,
    bind_text: String,
    overrides: Overrides,
    feeds: Vec<FeedInfo>,
    factory: ServiceFactory,
    service: Option<Box<dyn ServiceHandle>>,
    status: Option<Status>,
    next_refresh: Instant,
    tab: Tab,
    disclaimer: Option<Disclaimer>,
    messages: Vec<String>,
    tray: Option<Box<dyn TrayHandle>>,
    wayland: bool,
    tray_error: Option<String>,
    start_hidden: bool,
    frames: u64,
    quitting: bool,
}

/// `cfg` with exactly [`MAX_FEEDS`] slots; empty slots read `[none]`.
fn with_slots(mut cfg: PlaneServerConfig) -> PlaneServerConfig {
    cfg.feeds.resize(MAX_FEEDS, NONE.to_owned());
    for f in &mut cfg.feeds {
        if f.is_empty() {
            *f = NONE.to_owned();
        }
    }
    cfg
}

/// The file values, validated; the error text if the file cannot be read.
fn load(path: &std::path::Path) -> Result<PlaneServerConfig, String> {
    let mut c = PlaneServerConfig::load_or_default(path).map_err(|e| e.to_string())?;
    c.validate();
    Ok(c)
}

impl PlaneServerApp {
    /// `cfg` is the config of this run (file values plus command line overrides); `path` is
    /// the file. Starts the service with `factory(&cfg)`.
    pub fn new(cfg: PlaneServerConfig, path: PathBuf, factory: ServiceFactory) -> Self {
        let (draft, overrides) = match load(&path) {
            Ok(file) => {
                let o = Overrides::between(&file, &cfg);
                (file, o)
            }
            Err(_) => (cfg.clone(), Overrides::default()),
        };
        let draft = with_slots(draft);
        let feeds = as_feeds::registry::all_feeds()
            .into_iter()
            .map(|f| {
                let mut defaults = f.settings();
                if f.name() == "AirScoutServer" {
                    defaults.insert("url".into(), "".into());
                }
                FeedInfo {
                    name: f.name(),
                    disclaimer: f.disclaimer(),
                    needs_disclaimer: feed_needs_disclaimer(f.as_ref()),
                    secret_keys: f.secret_keys(),
                    defaults,
                }
            })
            .collect();
        let service = factory(&cfg);
        let mut app = PlaneServerApp {
            path,
            bind_text: draft.bind.to_string(),
            draft,
            overrides,
            feeds,
            factory,
            service: Some(service),
            status: None,
            next_refresh: Instant::now(),
            tab: Tab::Status,
            disclaimer: None,
            messages: Vec::new(),
            tray: None,
            wayland: false,
            tray_error: None,
            start_hidden: cfg.start_in_tray,
            frames: 0,
            quitting: false,
        };
        app.refresh();
        app
    }

    pub fn set_tray(&mut self, tray: Box<dyn TrayHandle>) {
        self.tray = Some(tray);
        self.tray_error = None;
    }

    /// The window runs on Wayland, where winit cannot hide it.
    pub fn set_wayland(&mut self, on: bool) {
        self.wayland = on;
    }

    /// The tray could not be made: the Status tab shows `tray: <error>`.
    pub fn set_tray_error(&mut self, e: String) {
        self.tray_error = Some(e);
    }

    /// The values the tabs edit.
    pub fn draft(&self) -> &PlaneServerConfig {
        &self.draft
    }

    /// Puts `s` in place of the running service (tests: a new status from a fake). The old
    /// one is stopped; the status is read at once.
    #[doc(hidden)]
    pub fn replace_service_for_test(&mut self, s: Box<dyn ServiceHandle>) {
        if let Some(old) = self.service.replace(s) {
            old.stop();
        }
        self.refresh();
    }

    pub fn draft_mut(&mut self) -> &mut PlaneServerConfig {
        &mut self.draft
    }

    fn refresh(&mut self) {
        self.next_refresh = Instant::now() + REFRESH;
        let Some(s) = self.service.as_ref().map(|s| s.status()) else {
            return;
        };
        if let Some(t) = &mut self.tray {
            t.set_planes(s.planes);
        }
        self.status = Some(s);
    }

    /// The non-drawing work of a frame; runs also while the window is hidden.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        let cmds = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for c in cmds {
            match c {
                TrayCommand::Show => {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                TrayCommand::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        // eframe shows the window on frame 1 in any case. Wayland cannot hide a window
        // (winit ignores `set_visible` there): start minimised instead.
        if self.frames == 2 && self.start_hidden {
            if self.can_hide() {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            } else if self.wayland {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
        }
        if Instant::now() >= self.next_refresh {
            self.refresh();
        }
        let close = ctx.input(|i| i.viewport().close_requested());
        // Without a way to hide the window, closing it quits.
        if close && !self.quitting && self.can_hide() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        ctx.request_repaint_after(POLL);
    }

    /// The window can go to the tray: a tray exists and the window system can hide it.
    fn can_hide(&self) -> bool {
        self.tray.is_some() && !self.wayland
    }

    /// Stops the service; the tray goes with the app.
    pub fn shutdown(&mut self) {
        if let Some(s) = self.service.take() {
            s.stop();
        }
        self.tray = None;
    }

    /// Apply: validate, save, restart the service.
    fn apply(&mut self) {
        self.messages.clear();
        match self.bind_text.trim().parse::<IpAddr>() {
            Ok(b) => self.draft.bind = b,
            Err(_) => {
                self.messages
                    .push(format!("invalid bind address: {}", self.bind_text.trim()));
                return;
            }
        }
        self.messages.extend(self.draft.validate());
        self.draft = with_slots(std::mem::take(&mut self.draft));
        self.bind_text = self.draft.bind.to_string();
        if let Err(e) = self.draft.save(&self.path) {
            self.messages.push(e.to_string());
            return;
        }
        if let Some(s) = self.service.take() {
            s.stop();
        }
        let run = self.overrides.apply(&self.draft);
        self.service = Some((self.factory)(&run));
        self.refresh();
        self.messages
            .push("Saved; the service was restarted.".to_owned());
    }

    /// Revert: the file values again.
    fn revert(&mut self) {
        self.messages.clear();
        match load(&self.path) {
            Ok(c) => {
                self.draft = with_slots(c);
                self.bind_text = self.draft.bind.to_string();
                self.disclaimer = None;
            }
            Err(e) => self.messages.push(e),
        }
    }

    /// Feed `name` was picked in `slot`. A restricted feed whose disclaimer is not accepted
    /// opens the dialog; Cancel there empties the slot again.
    fn select(&mut self, slot: usize, name: &str) {
        self.draft.feeds[slot] = name.to_owned();
        let Some(info) = self.feeds.iter().find(|f| f.name == name) else {
            return;
        };
        let accepted = self
            .draft
            .feeds_disclaimer_accepted
            .iter()
            .any(|a| a == name);
        if info.needs_disclaimer && !accepted {
            self.disclaimer = Some(Disclaimer {
                slot,
                feed: name.to_owned(),
                text: info.disclaimer.unwrap_or(FALLBACK_DISCLAIMER).to_owned(),
            });
        }
    }

    /// Draws the window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        Panel::top("ps_tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (t, name) in TABS {
                    ui.selectable_value(&mut self.tab, t, name);
                }
            });
        });
        Panel::bottom("ps_buttons").show(ui, |ui| {
            for m in &self.messages {
                ui.label(m);
            }
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    self.apply();
                }
                if ui.button("Revert").clicked() {
                    self.revert();
                }
            });
        });
        CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| match self.tab {
                    Tab::Status => self.status_tab(ui),
                    Tab::Feeds => self.feeds_tab(ui),
                    Tab::Area => self.area_tab(ui),
                    Tab::Server => self.server_tab(ui),
                    Tab::General => self.general_tab(ui),
                });
        });
        self.disclaimer_dialog(ui.ctx());
    }

    fn status_tab(&self, ui: &mut egui::Ui) {
        let Some(s) = &self.status else {
            ui.label("no service");
            return;
        };
        Grid::new("ps_status").num_columns(2).show(ui, |ui| {
            ui.label("Uptime:");
            ui.label(uptime(Utc::now() - s.started));
            ui.end_row();
            ui.label("Planes:");
            ui.label(s.planes.to_string());
            ui.end_row();
            ui.label("Server:");
            ui.label(&s.server);
            ui.end_row();
        });
        ui.separator();
        Grid::new("ps_status_feeds")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                for h in ["Feed", "State", "Last ok", "Last error"] {
                    ui.strong(h);
                }
                ui.end_row();
                for f in &s.feeds {
                    ui.label(&f.name);
                    ui.label(&f.state);
                    ui.label(
                        f.last_ok
                            .map(|t| t.format("%H:%M:%S UTC").to_string())
                            .unwrap_or_else(|| "never".to_owned()),
                    );
                    // `last_error` is never cleared: show it only while it is current.
                    let current = match (f.last_error_at, f.last_ok) {
                        (_, None) => true,
                        (Some(err), Some(ok)) => err > ok,
                        (None, Some(_)) => false,
                    };
                    match &f.last_error {
                        Some(e) if current => ui.label(e),
                        _ => ui.label(""),
                    };
                    ui.end_row();
                }
            });
        if s.feeds.is_empty() {
            ui.label("No feed configured: pick one in the Feeds tab.");
        }
        if let Some(e) = &s.db_error {
            ui.label(format!("database: {e}"));
        }
        if let Some(e) = &self.tray_error {
            ui.label(format!("tray: {e}"));
        }
    }

    fn feeds_tab(&mut self, ui: &mut egui::Ui) {
        for slot in 0..MAX_FEEDS {
            let current = self.draft.feeds[slot].clone();
            let mut picked = None;
            ui.group(|ui| {
                ComboBox::from_label(format!("Feed {}", slot + 1))
                    .selected_text(current.as_str())
                    // all feeds at once, no scrolling in the list
                    .height(400.0)
                    .show_ui(ui, |ui| {
                        let names = std::iter::once(NONE).chain(self.feeds.iter().map(|f| f.name));
                        for n in names {
                            if ui.selectable_label(current == n, n).clicked() && current != n {
                                picked = Some(n);
                            }
                        }
                    });
                if current != NONE {
                    self.feed_settings(ui, slot, &current);
                }
            });
            if let Some(n) = picked {
                self.select(slot, n);
            }
        }
    }

    /// The settings of feed `name` in `slot`: its defaults with the stored values on top.
    /// Strings are text fields (secret keys masked), numbers spinners, booleans checkboxes.
    fn feed_settings(&mut self, ui: &mut egui::Ui, slot: usize, name: &str) {
        let Some(info) = self.feeds.iter().find(|f| f.name == name) else {
            ui.label(format!("unknown feed ({name})"));
            return;
        };
        let mut table = info.defaults.clone();
        if let Some(Value::Table(t)) = self.draft.feed_settings.get(name) {
            for (k, v) in t {
                table.insert(k.clone(), v.clone());
            }
        }
        let before = table.clone();
        Grid::new(("ps_feed_settings", slot))
            .num_columns(2)
            .show(ui, |ui| {
                for (key, value) in table.iter_mut() {
                    let l = ui.label(key.as_str());
                    let secret = info.secret_keys.contains(&key.as_str());
                    let r = match value {
                        Value::String(s) => ui.add(
                            TextEdit::singleline(s)
                                .password(secret)
                                .desired_width(300.0),
                        ),
                        Value::Integer(i) => ui.add(DragValue::new(i)),
                        Value::Float(x) => ui.add(DragValue::new(x)),
                        Value::Boolean(b) => ui.checkbox(b, ""),
                        _ => ui.label("(not editable)"),
                    };
                    r.labelled_by(l.id);
                    ui.end_row();
                }
            });
        if table != before {
            as_settings::canonicalize_feed_table(name, &mut table);
            self.draft
                .feed_settings
                .insert(name.to_owned(), Value::Table(table));
        }
    }

    fn area_tab(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        ui.strong("Covered Area");
        Grid::new("ps_area").num_columns(2).show(ui, |ui| {
            spin(
                ui,
                "Min. Lat [-90 ... 90 deg]:",
                &mut d.min_lat,
                -90.0..=90.0,
            );
            spin(
                ui,
                "Max. Lat [-90 ... 90 deg]:",
                &mut d.max_lat,
                -90.0..=90.0,
            );
            spin(
                ui,
                "Min. Lon [-180 ... 180 deg]:",
                &mut d.min_lon,
                -180.0..=180.0,
            );
            spin(
                ui,
                "Max. Lon [-180 ... 180 deg]:",
                &mut d.max_lon,
                -180.0..=180.0,
            );
            spin(ui, "Min. Altitude [m]:", &mut d.min_alt_m, -1000..=30000);
            spin(ui, "Max. Altitude [m]:", &mut d.max_alt_m, -1000..=30000);
            spin(ui, "Lifetime [min]:", &mut d.lifetime_min, 1..=1440);
            spin(ui, "Interval [s]:", &mut d.interval_s, 1..=86400);
        });
    }

    fn server_tab(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.draft.server_active, "Activate Server");
        Grid::new("ps_server").num_columns(2).show(ui, |ui| {
            let l = ui.label("Bind address:");
            ui.text_edit_singleline(&mut self.bind_text)
                .labelled_by(l.id);
            ui.end_row();
            spin(ui, PORT_LABEL, &mut self.draft.port, 1..=65535);
        });
        ui.label(LAN_WARNING);
        ui.label(format!(
            "Planes are served at http://localhost:{}/planes.json (default port {DEFAULT_PORT}).",
            self.draft.port
        ));
        let o = self.overrides.lines();
        if !o.is_empty() {
            ui.label(format!(
                "This run uses from the command line (not saved): {}",
                o.join(", ")
            ));
        }
    }

    fn general_tab(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.draft.start_in_tray, "Start in tray");
        ui.label("Starts with the window hidden; needs a tray icon.");
        if self.wayland {
            ui.label(
                "Wayland: the window cannot hide in the tray. Closing it quits the plane \
                 server, and Start in tray starts it minimised.",
            );
        }
        ui.label(format!("Config file: {}", self.path.display()));
    }

    fn disclaimer_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.disclaimer else { return };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("ps_feed_disclaimer")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(format!("Disclaimer: {}", d.feed));
            ui.label(&d.text);
            ui.horizontal(|ui| {
                if ui.button("Accept").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if !self.draft.feeds_disclaimer_accepted.contains(&d.feed) {
                    self.draft.feeds_disclaimer_accepted.push(d.feed.clone());
                }
                self.disclaimer = None;
            }
            Some(false) => {
                self.draft.feeds[d.slot] = NONE.to_owned();
                self.disclaimer = None;
            }
            None => {}
        }
    }
}

/// A labelled spinner row of a grid.
fn spin<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut N,
    range: std::ops::RangeInclusive<N>,
) {
    let l = ui.label(label);
    ui.add(DragValue::new(v).range(range)).labelled_by(l.id);
    ui.end_row();
}

/// `1d 02:03:04`.
fn uptime(d: chrono::Duration) -> String {
    let s = d.num_seconds().max(0);
    format!(
        "{}d {:02}:{:02}:{:02}",
        s / 86400,
        s / 3600 % 24,
        s / 60 % 60,
        s % 60
    )
}

impl eframe::App for PlaneServerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}

/// Opens the window; returns when it is closed (or Quit in the tray).
pub fn run(cfg: PlaneServerConfig, path: PathBuf) -> Result<(), String> {
    // eframe picks Wayland when it is there; winit cannot hide a window on Wayland.
    let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size(SIZE)
            .with_icon(egui::IconData {
                rgba: icon_rgba(),
                width: ICON_SIZE,
                height: ICON_SIZE,
            }),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            let mut app = PlaneServerApp::new(cfg, path, Box::new(real_service));
            app.set_wayland(wayland);
            // Here: on the main thread, after the event loop is set up (macOS, Windows).
            match Tray::new(&cc.egui_ctx) {
                Ok(t) => app.set_tray(Box::new(t)),
                Err(e) => {
                    eprintln!("airscout-planeserver: tray: {e}");
                    app.set_tray_error(e);
                }
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())
}
