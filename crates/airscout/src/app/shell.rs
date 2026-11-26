//! The eframe app shell.

use super::*;

pub struct AirScoutApp {
    pub state: AppState,
    pub map: crate::ui::map::MapView,
    rx: Receiver<ServiceEvent>,
}

impl AirScoutApp {
    /// Load the settings, open the data directory, wire the services.
    pub fn new(settings_path: PathBuf, data_dir: &Path) -> Self {
        let (settings, settings_error) = load_settings(&settings_path);
        let mut app = Self::with_settings(settings, settings_error, settings_path, data_dir);
        app.state.startup();
        app
    }

    pub fn with_settings(
        settings: Settings,
        settings_error: Option<String>,
        settings_path: PathBuf,
        data_dir: &Path,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (services, open_error): (Box<dyn Services>, Option<String>) = match open_data(data_dir)
        {
            Ok(paths) => (Box::new(RealServices::new(paths, tx)), None),
            Err(e) => {
                let msg = format!("data directory {}: {e}", data_dir.display());
                (Box::new(FailedServices(msg.clone())), Some(msg))
            }
        };
        let mut state = AppState::new(
            settings,
            settings_error,
            services,
            Arc::new(PlaneInfoCache::new()),
        );
        if let Some(e) = open_error {
            state.status = e;
        }
        let (beeper, note) = crate::sound::default_beeper();
        state.set_beeper(beeper);
        if let Some(n) = note {
            state.status = n;
        }
        state.settings_path = Some(settings_path.clone());
        state.tile_dir = Some(crate::tiles::tiles_cache_dir(data_dir));
        state.data_dir = Some(data_dir.to_path_buf());
        // Without the store the map loads every tile over HTTP, as before.
        let (map_store, store_error) = crate::tiles::open_store(data_dir);
        state.map_store = map_store;
        if let Some(msg) = store_error {
            state.status = if state.status.is_empty() {
                msg
            } else {
                format!("{}; {msg}", state.status)
            };
        }
        Self {
            state,
            map: crate::ui::map::MapView::new(data_dir),
            rx,
        }
    }

    /// Window geometry for the next start, if saved.
    pub fn window_geometry(&self) -> (Option<[f32; 2]>, Option<[f32; 2]>) {
        let g = &self.state.settings.gui;
        (g.window_size, g.window_pos)
    }
}

impl eframe::App for AirScoutApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.state.wizard.is_some() {
            // Only the wizard is drawn: no map (no tile fetches), no ticks, no events.
            use crate::ui::dialogs::wizard::WizardAction;
            let action = match self.state.wizard.as_mut() {
                Some(w) => w.show(ui),
                None => WizardAction::None,
            };
            match action {
                WizardAction::None => {}
                WizardAction::Finish => self.state.finish_wizard(),
                WizardAction::Cancel => ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close),
            }
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        while let Ok(e) = self.rx.try_recv() {
            self.state.on_event(e);
        }
        if self.state.tick_due(Instant::now()) {
            let cache = self.state.cache.clone();
            self.state.on_tick(Utc::now(), &cache);
        }
        self.state.track_sync();
        if std::mem::take(&mut self.state.alarm_focus) {
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Focus);
        }
        crate::ui::show(ui, &mut self.state, &mut self.map);
        self.state.sync_my();
        self.state.sync_integrations();
        if self.state.settings.track.activate {
            // display only: the Track worker steps on its own clock, also while the window is
            // minimised; this repaint shows its values once a second
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        ctx.input(|i| {
            let vp = i.viewport();
            let g = &mut self.state.settings.gui;
            if let Some(r) = vp.inner_rect {
                g.window_size = Some([r.width(), r.height()]);
            }
            if let Some(r) = vp.outer_rect {
                g.window_pos = Some([r.min.x, r.min.y]);
            }
        });
        // The services cannot wake the UI: poll their events a few times per second.
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn on_exit(&mut self) {
        self.state.on_exit();
    }
}

/// The window root: with `--clean` the cleanup dialog comes first, and the app (which opens the
/// databases) is built only after the dialog ended with "Continue" (`MapDlg.cs:632-646`).
pub struct RootApp {
    cleanup: Option<crate::cleanup::CleanupDlg>,
    app: Option<AirScoutApp>,
    settings_path: PathBuf,
    data_dir: PathBuf,
}

impl RootApp {
    pub fn new(app: AirScoutApp) -> Self {
        Self {
            cleanup: None,
            app: Some(app),
            settings_path: PathBuf::new(),
            data_dir: PathBuf::new(),
        }
    }

    /// Show the cleanup dialog first; no database is opened until it ends.
    pub fn with_cleanup(settings_path: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            cleanup: Some(crate::cleanup::CleanupDlg::new(
                settings_path.clone(),
                data_dir.clone(),
            )),
            app: None,
            settings_path,
            data_dir,
        }
    }
}

impl eframe::App for RootApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        use crate::cleanup::CleanupOutcome;
        if let Some(dlg) = self.cleanup.as_mut() {
            let ctx = ui.ctx().clone();
            if ctx.input(|i| i.viewport().close_requested()) {
                if dlg.busy() {
                    // never leave a half-done delete: keep the window until it ends
                    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::CancelClose);
                } else {
                    dlg.close();
                }
            }
            crate::ui::dialogs::cleanup::show(ui, dlg);
            match dlg.outcome {
                // C# `Abort`: exit immediately, nothing was deleted by this action.
                Some(CleanupOutcome::Abort) => std::process::exit(-1),
                Some(CleanupOutcome::Proceed) => {
                    self.cleanup = None;
                    let app = AirScoutApp::new(self.settings_path.clone(), &self.data_dir);
                    let size = app.window_geometry().0.unwrap_or([1280.0, 800.0]);
                    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(size.into()));
                    self.app = Some(app);
                }
                None => {}
            }
            return;
        }
        if let Some(app) = self.app.as_mut() {
            app.ui(ui, frame);
        }
    }

    fn on_exit(&mut self) {
        if let Some(app) = self.app.as_mut() {
            app.on_exit();
        }
    }
}
