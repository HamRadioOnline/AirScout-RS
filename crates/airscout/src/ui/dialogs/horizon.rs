//! `HorizonDlg`: radio horizon of a station out to 500 km, as elevation angle (Eps_Min) and
//! distance over the bearing, polar or cartesian. The plots fill while the calculation runs. The
//! original's third view (the horizon as a route on a map) is not ported.

use crate::horizon_service::HorizonRequest;
use as_geo::latlon::GPoint;
use as_propagation::{HorizonPoint, PropagationHorizon};
use as_settings::Settings;
use eframe::egui::{self, Color32, Grid, Ui};
use egui_plot::{Line, Plot, PlotPoint, PlotPoints, Text};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `SupportFunctions.GetCSVSeparator` is ";" for a locale with a decimal comma and "," else.
/// The port does not read the locale: numbers are always written with a decimal point, so the
/// separator is a constant ";" (never ambiguous).
pub const CSV_SEPARATOR: &str = ";";

const BLUE: Color32 = Color32::from_rgb(0, 0, 255);
const RED: Color32 = Color32::from_rgb(255, 0, 0);

/// `(bearing, value)` pairs.
type Series = Vec<(f64, f64)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizonState {
    Idle,
    Running,
    Done,
    Failed,
}

/// What the user asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizonAction {
    None,
    Calculate,
    Export,
    Close,
}

pub struct HorizonDlg {
    pub call: String,
    pub pos: GPoint,
    /// Antenna height above ground [m].
    pub height_m: f64,
    pub with_obstruction: bool,
    /// Settings the calculation uses (a copy; the dialog edits nothing of them).
    pub settings: Settings,
    /// Request the answers are for.
    pub id: u64,
    pub state: HorizonState,
    /// Polar or cartesian plots (`Horizon_Plot_Polar`).
    pub polar: bool,
    /// Status line: result, export path, or the error.
    pub status: String,
    slots: Vec<Option<HorizonPoint>>,
    horizon: Option<PropagationHorizon>,
    /// Why nothing can be calculated.
    error: Option<String>,
    started: Option<Instant>,
}

impl HorizonDlg {
    pub fn new(
        call: &str,
        pos: GPoint,
        height_m: f64,
        with_obstruction: bool,
        settings: &Settings,
    ) -> Self {
        let none =
            settings.elevation_enabled.effective_model() == as_elevation::ElevationModel::None;
        Self {
            call: call.to_string(),
            pos,
            height_m,
            with_obstruction,
            settings: settings.clone(),
            id: 0,
            state: HorizonState::Idle,
            polar: settings.gui.horizon_plot_polar,
            status: String::new(),
            slots: vec![None; 360],
            horizon: None,
            error: none.then(|| "No elevation model enabled: enable one in Options".to_string()),
            started: None,
        }
    }

    /// The calculation cannot run (no elevation model).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The request to start, answered with events carrying `id`.
    pub fn begin(&mut self, id: u64) -> HorizonRequest {
        self.id = id;
        self.state = HorizonState::Running;
        self.slots = vec![None; 360];
        self.horizon = None;
        self.started = Some(Instant::now());
        self.status = format!("Calculating the radio horizon of {} ...", self.call);
        HorizonRequest {
            id,
            call: self.call.clone(),
            pos: self.pos,
            height_m: self.height_m,
            with_obstruction: self.with_obstruction,
            settings: self.settings.clone(),
        }
    }

    /// `DrawHorizonPoint` while the calculation runs. The azimuths arrive in any order.
    pub fn on_point(&mut self, id: u64, az: usize, p: HorizonPoint) {
        if id == self.id && self.state == HorizonState::Running && az < 360 {
            self.slots[az] = Some(p);
        }
    }

    /// The complete horizon replaces what was drawn (`bw_Horizon_Calculate_RunWorkerCompleted`).
    pub fn on_done(&mut self, id: u64, h: PropagationHorizon) {
        if id != self.id || self.state != HorizonState::Running {
            return;
        }
        self.slots = h.horizon.iter().map(|p| Some(*p)).collect();
        let ms = self.started.map_or(0, |t| t.elapsed().as_millis());
        self.status = if h.valid {
            format!(
                "Calculation of radio horizon of {} finished, {ms}ms.",
                self.call
            )
        } else {
            format!(
                "Radio horizon of {} is incomplete: elevation data is missing.",
                self.call
            )
        };
        self.horizon = Some(h);
        self.state = HorizonState::Done;
    }

    pub fn on_failed(&mut self, id: u64, error: String) {
        if id == self.id && self.state == HorizonState::Running {
            self.status = error;
            self.state = HorizonState::Failed;
        }
    }

    /// The finished horizon, for the export.
    pub fn horizon(&self) -> Option<&PropagationHorizon> {
        self.horizon.as_ref()
    }

    /// Number of azimuths drawn so far.
    pub fn points_drawn(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Write the CSV into `dir` (created if missing); the status line gets the path.
    pub fn export(&mut self, dir: &Path) {
        let Some(h) = &self.horizon else {
            return;
        };
        self.status = match export_csv(dir, &self.call, &h.horizon) {
            Ok(p) => format!("Exported to {}", p.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    /// Series of the filled azimuths, in bearing order: `(bearing, eps_min [deg])` and
    /// `(bearing, distance [km])`.
    fn series(&self) -> (Series, Series) {
        let mut eps = Vec::new();
        let mut dist = Vec::new();
        for (az, p) in self.slots.iter().enumerate() {
            if let Some(p) = p {
                eps.push((az as f64, p.eps_min.to_degrees()));
                dist.push((az as f64, p.dist));
            }
        }
        (eps, dist)
    }

    pub fn show(&mut self, ctx: &egui::Context) -> HorizonAction {
        let mut action = HorizonAction::None;
        egui::Modal::new(egui::Id::unique("horizon_dlg")).show(ctx, |ui| {
            ui.set_width(760.0);
            ui.heading(format!("Radio Horizon of {}", self.call));
            self.parameters(ui);
            ui.horizontal(|ui| {
                ui.label("Plot");
                ui.selectable_value(&mut self.polar, true, "Polar");
                ui.selectable_value(&mut self.polar, false, "Cartesian");
            });
            self.plots(ui);
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.label(&self.status);
            ui.horizontal(|ui| {
                let calc = self.error.is_none() && self.state != HorizonState::Running;
                if ui
                    .add_enabled(calc, egui::Button::new("Calculate"))
                    .clicked()
                {
                    action = HorizonAction::Calculate;
                }
                let export = self.state == HorizonState::Done;
                if ui
                    .add_enabled(export, egui::Button::new("Export CSV"))
                    .clicked()
                {
                    action = HorizonAction::Export;
                }
                if ui.button("Close").clicked() {
                    action = HorizonAction::Close;
                }
            });
        });
        action
    }

    fn parameters(&self, ui: &mut Ui) {
        let s = &self.settings;
        let model = s.elevation_enabled.effective_model();
        let ground = (self.state == HorizonState::Done)
            .then(|| self.horizon.as_ref().map(|h| h.params.h - self.height_m))
            .flatten();
        let bs = s.current_band_settings();
        Grid::new("horizon_params").num_columns(4).show(ui, |ui| {
            ui.label("Latitude");
            ui.label(format!("{:.8}", self.pos.lat));
            ui.label("Longitude");
            ui.label(format!("{:.8}", self.pos.lon));
            ui.end_row();
            ui.label("Elevation [m]");
            ui.label(ground.map_or("-".to_string(), |g| format!("{g:.0}")));
            ui.label("Antenna height [m]");
            ui.label(format!("{:.0}", self.height_m));
            ui.end_row();
            ui.label("K factor");
            ui.label(format!("{:.2}", bs.k_factor));
            ui.label("QRG");
            ui.label(s.band.name());
            ui.end_row();
            ui.label("F1 clearance");
            ui.label(format!("{:.2}", bs.f1_clearance));
            ui.label("Elevation model");
            ui.label(model.dir_name());
            ui.end_row();
        });
    }

    fn plots(&self, ui: &mut Ui) {
        let (eps, dist) = self.series();
        // the polar plots are closed to a circle once complete (`DrawHorizonPoint(0, .., true)`)
        let closed = eps.len() == 360;
        ui.columns(2, |cols| {
            if self.polar {
                polar_plot(
                    &mut cols[0],
                    "horizon_eps_polar",
                    "Elevation angle [deg]",
                    &eps,
                    BLUE,
                    closed,
                );
                polar_plot(
                    &mut cols[1],
                    "horizon_dist_polar",
                    "Distance [km]",
                    &dist,
                    RED,
                    closed,
                );
            } else {
                cartesian_plot(
                    &mut cols[0],
                    "horizon_eps_cart",
                    "Elevation angle [deg]",
                    &eps,
                    BLUE,
                );
                cartesian_plot(
                    &mut cols[1],
                    "horizon_dist_cart",
                    "Distance [km]",
                    &dist,
                    RED,
                );
            }
        });
    }
}

const PLOT_HEIGHT: f32 = 300.0;

fn cartesian_plot(ui: &mut Ui, id: &str, title: &str, pts: &[(f64, f64)], color: Color32) {
    ui.strong(title);
    let line: Vec<[f64; 2]> = pts.iter().map(|&(x, y)| [x, y]).collect();
    Plot::new(id)
        .height(PLOT_HEIGHT)
        .allow_scroll(false)
        .x_axis_label("Bearing [deg]")
        .include_x(0.0)
        .include_x(360.0)
        .show(ui, |pu| {
            pu.line(
                Line::new(title, PlotPoints::from(line))
                    .color(color)
                    .width(2.0),
            );
        });
}

/// A "nice" ring step (1, 2, 5 times a power of ten) giving about four rings over `span`.
fn ring_step(span: f64) -> f64 {
    let raw = (span / 4.0).max(1e-9);
    let pow = 10f64.powf(raw.log10().floor());
    let f = raw / pow;
    let m = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * pow
}

/// Polar chart: bearing clockwise from the top (north), the value as radius. The centre is the
/// smallest value (at most 0), so negative elevation angles stay visible.
fn polar_plot(
    ui: &mut Ui,
    id: &str,
    title: &str,
    pts: &[(f64, f64)],
    color: Color32,
    closed: bool,
) {
    ui.strong(title);
    let lo = pts.iter().map(|p| p.1).fold(0.0, f64::min);
    let hi = pts.iter().map(|p| p.1).fold(lo, f64::max);
    let step = ring_step((hi - lo).max(1e-6));
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).ceil() as i64;
    let outer = last as f64 * step - lo;
    let at = |az: f64, r: f64| {
        let a = az.to_radians();
        [r * a.sin(), r * a.cos()]
    };
    let mut curve: Vec<[f64; 2]> = pts.iter().map(|&(az, v)| at(az, v - lo)).collect();
    if closed {
        if let Some(&p) = curve.first() {
            curve.push(p);
        }
    }
    let grid = Color32::from_gray(150);
    Plot::new(id)
        .height(PLOT_HEIGHT)
        .data_aspect(1.0)
        .show_axes(false)
        .show_grid(false)
        .allow_scroll(false)
        .show(ui, |pu| {
            for k in first..=last {
                let v = k as f64 * step;
                let r = v - lo;
                if r <= 0.0 {
                    continue;
                }
                let ring: Vec<[f64; 2]> = (0..=72).map(|i| at(f64::from(i) * 5.0, r)).collect();
                pu.line(
                    Line::new("ring", PlotPoints::from(ring))
                        .color(grid)
                        .width(0.5),
                );
                let [x, y] = at(135.0, r);
                pu.text(Text::new(
                    "ring label",
                    PlotPoint::new(x, y),
                    format!("{v:.0}"),
                ));
            }
            for az in (0..360).step_by(30) {
                let a = f64::from(az);
                pu.line(
                    Line::new("spoke", PlotPoints::from(vec![[0.0, 0.0], at(a, outer)]))
                        .color(grid)
                        .width(0.5),
                );
                let [x, y] = at(a, outer * 1.1);
                pu.text(Text::new("bearing", PlotPoint::new(x, y), az.to_string()));
            }
            pu.line(
                Line::new(title, PlotPoints::from(curve))
                    .color(color)
                    .width(2.0),
            );
        });
}

/// CSV of the horizon as `HorizonDlg.btn_Horizon_Export_Click` writes it: bearing, Eps_Min [deg],
/// distance [km] and ground elevation [m] of the obstacle, 8 decimals.
pub fn horizon_csv(h: &[HorizonPoint; 360]) -> String {
    let sep = CSV_SEPARATOR;
    let mut s = format!("Bearing[deg]{sep}Eps_Min[deg]{sep}Distance[km]{sep}Elevation[m]\n");
    for (i, p) in h.iter().enumerate() {
        s.push_str(&format!(
            "{i}{sep}{:.8}{sep}{:.8}{sep}{:.8}\n",
            p.eps_min / std::f64::consts::PI * 180.0,
            p.dist,
            f64::from(p.elv)
        ));
    }
    s
}

/// Write the CSV to `<dir>/horizon_<call>.csv`; creates `dir`.
pub fn export_csv(dir: &Path, call: &str, h: &[HorizonPoint; 360]) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    // a call may contain '/': keep the file name flat
    let call: String = call
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!("horizon_{call}.csv"));
    std::fs::write(&path, horizon_csv(h))?;
    Ok(path)
}
