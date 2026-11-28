//! "Download data for the area" (`MapDlg.cs` has no equivalent; owner request): pick the map
//! zoom and whether to preload maps and elevation, see the tile/square counts, the size
//! estimates and the free disk space, then start the download. Map tiles come from the
//! provider's preload mirror only.

use crate::app::AppState;
use crate::download_area::{plan, AreaPlan};
use crate::jobs::JobEvent;
use crate::map_preloader::{map_mirror_ok, PRELOAD_MIN_ZOOM};
use as_geo::latlon::GRect;
use as_settings::{Settings, MAP_PRELOADER_MAX_ZOOM};
use eframe::egui;

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadAreaResult {
    Open,
    Start,
    Cancel,
    Close,
}

/// The state of the dialog.
pub struct DownloadAreaDlg {
    pub maps: bool,
    pub zoom: u8,
    pub elevation: bool,
    pub error: Option<String>,
    /// Id of the running download job, while it runs.
    pub running: Option<u64>,
    /// Last progress text of the running job.
    pub progress: String,
    /// The summary of the finished job.
    pub done: Option<String>,
}

impl DownloadAreaDlg {
    pub fn new(s: &Settings) -> Self {
        Self {
            maps: true,
            zoom: s.gui.map_preloader_max_zoom.min(MAP_PRELOADER_MAX_ZOOM),
            elevation: true,
            error: None,
            running: None,
            progress: String::new(),
            done: None,
        }
    }

    /// The plan for the current selections and settings, and the map-part notice (no mirror /
    /// provider mirror) when the Maps box is checked but the map part is unavailable.
    pub fn plan_for(&self, s: &Settings) -> (AreaPlan, Option<String>) {
        let (maps, notice) = match self.maps {
            true => match map_mirror_ok(&s.gui.map_provider, &s.background.map_mirror_url) {
                Ok(()) => (Some(self.zoom), None),
                Err(e) => (None, Some(e)),
            },
            false => (None, None),
        };
        let a = &s.area;
        let area = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
        (
            plan(&area, maps, self.elevation, &s.elevation_enabled),
            notice,
        )
    }

    /// Handle a job event of the running download; returns the status-line text, if any.
    pub fn on_event(&mut self, event: JobEvent) -> Option<String> {
        match event {
            JobEvent::Progress(s) => {
                self.progress = s.clone();
                Some(s)
            }
            JobEvent::Done(s) => {
                self.done = Some(s.clone());
                self.running = None;
                Some(s)
            }
            JobEvent::Failed(s) => {
                self.error = Some(s.clone());
                self.running = None;
                Some(format!("error: {s}"))
            }
            _ => None,
        }
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        p: &AreaPlan,
        notice: Option<&str>,
        free: Option<u64>,
    ) -> DownloadAreaResult {
        let mut result = DownloadAreaResult::Open;
        let running = self.running.is_some();
        egui::Modal::new(egui::Id::unique("download_area_dlg")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading("Download data for the area");
            ui.label("Preload everything AirScout needs for the configured area, for offline use.");
            ui.separator();
            ui.checkbox(&mut self.maps, "Maps");
            ui.add_enabled_ui(self.maps, |ui| {
                ui.horizontal(|ui| {
                    ui.label("up to zoom");
                    ui.add(
                        egui::DragValue::new(&mut self.zoom)
                            .range(PRELOAD_MIN_ZOOM..=MAP_PRELOADER_MAX_ZOOM),
                    );
                });
            });
            ui.checkbox(&mut self.elevation, "Elevation");
            ui.separator();

            if let Some(m) = &p.maps {
                ui.label(format!(
                    "Maps: {} tiles (about {})",
                    m.tiles,
                    human(m.bytes)
                ));
            }
            for e in &p.elevation {
                ui.label(format!(
                    "{}: {} squares (about {})",
                    e.model.dir_name(),
                    e.squares,
                    human(e.bytes)
                ));
            }
            if p.total_bytes() == 0 {
                ui.label("Nothing to download.");
            } else {
                ui.label(format!("Total: about {}", human(p.total_bytes())));
            }
            if let Some(n) = notice {
                ui.colored_label(ui.visuals().warn_fg_color, n);
            }
            match free {
                Some(f) if !p.enough(f) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!(
                            "Not enough free disk space: {} free, about {} needed.",
                            human(f),
                            human(p.total_bytes())
                        ),
                    );
                }
                Some(f) => {
                    ui.label(format!("Free disk space: {}", human(f)));
                }
                None => {
                    ui.label("Free disk space: unknown");
                }
            }

            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            if let Some(d) = &self.done {
                ui.colored_label(ui.visuals().warn_fg_color, d);
            }
            if running && !self.progress.is_empty() {
                ui.label(&self.progress);
            }

            ui.separator();
            let can_start = !running && p.total_bytes() > 0 && free.is_some_and(|f| p.enough(f));
            ui.horizontal(|ui| {
                if running {
                    if ui.button("Cancel").clicked() {
                        result = DownloadAreaResult::Cancel;
                    }
                } else {
                    if ui
                        .add_enabled(can_start, egui::Button::new("Start"))
                        .on_disabled_hover_text(
                            "Select Maps or Elevation, and free enough disk space",
                        )
                        .clicked()
                    {
                        result = DownloadAreaResult::Start;
                    }
                    if ui.button("Close").clicked() {
                        result = DownloadAreaResult::Close;
                    }
                }
            });
        });
        result
    }
}

/// Draw the dialog and apply the user's action through the app state.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.download_area_dlg.take() else {
        return;
    };
    let (p, notice) = dlg.plan_for(&state.settings);
    let free = state.free_disk_bytes();
    let result = dlg.show(ctx, &p, notice.as_deref(), free);
    match result {
        DownloadAreaResult::Open => state.download_area_dlg = Some(dlg),
        DownloadAreaResult::Start => {
            if let Err(e) = state.start_download_area(dlg.maps, dlg.zoom, dlg.elevation) {
                dlg.error = Some(e);
                state.download_area_dlg = Some(dlg);
            } else {
                dlg.running = state.maint.running;
                dlg.error = None;
                dlg.done = None;
                dlg.progress.clear();
                state.download_area_dlg = Some(dlg);
            }
        }
        DownloadAreaResult::Cancel => {
            state.cancel_download_area(dlg.running);
            state.download_area_dlg = Some(dlg);
        }
        DownloadAreaResult::Close => {
            state.cancel_download_area(dlg.running);
        }
    }
}

fn human(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{bytes} B")
    }
}
