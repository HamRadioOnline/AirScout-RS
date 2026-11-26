//! Background database updaters (`StartAllBackgroundWorkers`, MapDlg.cs:2166-2455) and the
//! status LEDs that show their state (`DatabaseStatus.cs`).
//!
//! One thread per job: stations, aircraft, position maintainer, one elevation updater per
//! enabled model (which only checks the stored squares when `gui.elevation_download` is off) and
//! the map preloader (`crate::map_preloader`, no LED). A job runs once on startup (`update_on_startup`) and then every
//! `update_period_min` (`update_periodically`). A blocking HTTP request cannot be interrupted:
//! [`BackgroundService::stop`] sets the cancel flag and detaches; a thread inside a request ends
//! when that request returns, and from the moment of the cancel it reports nothing any more.

use crate::calculator::{Calculator, IdleSet, Timing};
use crate::map_preloader::{
    finished_text, no_mirror_text, preload_gate, preload_run, preloader_client,
    provider_mirror_text, PreloadGate, PRELOAD_DELAY, PRELOAD_MIN_ZOOM,
};
use crate::services::{client, sea_file, Paths, ServiceEvent};
use crate::tiles::store_path;
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::{maintain, MaintainerConfig, PositionDb};
use as_aircraft::updater::update_aircraft_db;
use as_elevation::{check_area, download_area, ElevationError, ElevationModel};
use as_geo::latlon::GRect;
use as_maps::{preload_tiles, TileStore};
use as_settings::Settings;
use as_stations::{update_stations, StationDb};
use eframe::egui::Color32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// One status LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Led {
    Aircraft,
    Stations,
    Elevation(ElevationModel),
}

/// The LEDs in status bar order.
pub const LEDS: [Led; 7] = [
    Led::Aircraft,
    Led::Stations,
    Led::Elevation(ElevationModel::Globe),
    Led::Elevation(ElevationModel::Srtm3),
    Led::Elevation(ElevationModel::Srtm1),
    Led::Elevation(ElevationModel::Aster3),
    Led::Elevation(ElevationModel::Aster1),
];

impl Led {
    pub fn label(self) -> &'static str {
        match self {
            Led::Aircraft => "Aircraft",
            Led::Stations => "Stations",
            Led::Elevation(m) => m.dir_name(),
        }
    }
}

/// `DATABASESTATUS` as far as the LEDs need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedState {
    Empty,
    Updating,
    Complete,
    UpToDate,
    Error,
    #[default]
    Unknown,
}

/// `DatabaseStatus.GetDatabaseStatusColor`: ERROR red, UPTODATE green, COMPLETE blue, UPDATING
/// gold, EMPTY black, otherwise plum.
pub fn led_color(s: LedState) -> Color32 {
    match s {
        LedState::Error => Color32::from_rgb(255, 0, 0),
        LedState::UpToDate => Color32::from_rgb(0, 128, 0),
        LedState::Complete => Color32::from_rgb(0, 0, 255),
        LedState::Updating => Color32::from_rgb(255, 215, 0),
        LedState::Empty => Color32::BLACK,
        LedState::Unknown => Color32::from_rgb(221, 160, 221),
    }
}

/// `DatabaseStatus.GetDatabaseStatusText`.
pub fn led_text(s: LedState) -> &'static str {
    match s {
        LedState::Unknown => "Database status is unknown.",
        LedState::Error => "Database has errors.",
        LedState::UpToDate => "Database is up to date.",
        LedState::Complete => "Database is complete.",
        LedState::Updating => "Database is updating.",
        LedState::Empty => "Database is empty.",
    }
}

/// Remove `user:pass@` / `user@` after every `://`.
///
/// A password may hold an unescaped `/`, `?`, `#`, `'` or `)`, so the authority cannot be cut at
/// the first of them: everything up to the last `@` of the URL-like word goes (the word ends at
/// whitespace or a character never allowed unescaped in a URL). An `@` later in the path or
/// query drops the host as well; that is the safe side.
fn strip_userinfo(text: &str) -> String {
    let end_of_word = |c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let word_len = tail.find(end_of_word).unwrap_or(tail.len());
        let skip = tail[..word_len].rfind('@').map_or(0, |p| p + 1);
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out
}

/// Drop the user info and everything from the first `?` of a URL-like word: both can carry
/// credentials.
pub fn strip_query(text: &str) -> String {
    let text = strip_userinfo(text);
    let mut out = String::with_capacity(text.len());
    let mut skipping = false;
    for c in text.chars() {
        if skipping {
            if c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '>') {
                skipping = false;
                out.push(c);
            }
        } else if c == '?' {
            skipping = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// Sends events unless the service was cancelled: a detached thread stays silent.
#[derive(Clone)]
struct Emitter {
    tx: Sender<ServiceEvent>,
    cancel: Arc<AtomicBool>,
    /// Stamped on every LED event, so the app can drop those of a replaced service.
    generation: u64,
    /// Marked by `idle`; the calculators wait on it.
    idle_set: Arc<IdleSet>,
}

impl Emitter {
    fn send(&self, e: ServiceEvent) {
        if !self.cancel.load(Ordering::Relaxed) {
            let _ = self.tx.send(e);
        }
    }

    fn led(&self, led: Led, state: LedState, text: String) {
        self.send(ServiceEvent::Led {
            generation: self.generation,
            led,
            state,
            text,
        });
    }

    fn idle(&self, led: Led) {
        self.idle_set.mark(led);
        self.send(ServiceEvent::BackgroundIdle {
            generation: self.generation,
            led,
        });
    }
}

#[derive(Clone, Copy)]
struct Schedule {
    /// Run at all (`RUNONCE` or `RUNPERIODICALLY`).
    run: bool,
    /// Run again after `period` (`RUNPERIODICALLY`).
    periodic: bool,
    period: Duration,
}

/// Sleep `total` in slices of at most 200 ms; true if cancelled meanwhile.
pub(crate) fn sleep_cancellable(total: Duration, cancel: &AtomicBool) -> bool {
    let slice = Duration::from_millis(200);
    let mut left = total;
    while !left.is_zero() {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        let d = left.min(slice);
        std::thread::sleep(d);
        left -= d;
    }
    cancel.load(Ordering::Relaxed)
}

struct RunningGuard(Arc<AtomicUsize>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Result of one run of a job: LED state and tooltip detail, or `None` if cancelled.
type Outcome = Option<(LedState, String)>;

/// The running updater threads. Dropping it cancels them too.
pub struct BackgroundService {
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicUsize>,
}

impl BackgroundService {
    /// Start the jobs for `settings`. All events go to `tx`, tagged with `generation`, and stop
    /// when `cancel` is set.
    ///
    /// Schedule as in `StartAllBackgroundWorkers` (MapDlg.cs:2186, 2291): `update_on_startup`
    /// runs the updaters once; only if that is off, `update_periodically` runs them at once and
    /// then every `update_period_min`. With both set the updaters run once (`else if`). With
    /// neither they do not run. The position maintainer runs in every case.
    pub fn start(
        paths: Paths,
        settings: &Settings,
        tx: Sender<ServiceEvent>,
        cancel: Arc<AtomicBool>,
        generation: u64,
    ) -> Self {
        let svc = Self {
            cancel: cancel.clone(),
            running: Arc::new(AtomicUsize::new(0)),
        };
        let bg = &settings.background;
        let sched = Schedule {
            run: bg.update_on_startup || bg.update_periodically,
            periodic: !bg.update_on_startup && bg.update_periodically,
            period: Duration::from_secs(bg.update_period_min.max(1) * 60),
        };
        let em = Emitter {
            tx,
            cancel,
            generation,
            idle_set: Arc::new(IdleSet::default()),
        };
        let a = &settings.area;
        let rect = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);

        let (p, url) = (paths.clone(), bg.stations_url.clone());
        let area = settings.location_restrict_to_area.then_some(rect);
        let cancel = em.cancel.clone();
        svc.spawn_job(
            "bg-stations",
            sched,
            em.clone(),
            Some(Led::Stations),
            move |progress| {
                let r = client().and_then(|c| {
                    let cache = p.data_dir.join("tmp");
                    // Own connection (WAL): the import transaction must not hold the mutex of
                    // the shared handle, which the UI thread queries.
                    let db = StationDb::open(&p.data_dir.join("stations.db3"))
                        .map_err(|e| e.to_string())?;
                    update_stations(&c, &url, &cache, &db, area, Some(&cancel), &mut |m| {
                        progress(m.to_string())
                    })
                    .map_err(|e| e.to_string())
                });
                finish(r.map(|_| ()), cancel_err(&cancel))
            },
        );

        let (p, url) = (paths.clone(), bg.aircraft_url.clone());
        let cancel = em.cancel.clone();
        svc.spawn_job(
            "bg-aircraft",
            sched,
            em.clone(),
            Some(Led::Aircraft),
            move |progress| {
                let r = client().and_then(|c| {
                    let db = AircraftDb::open(&p.data_dir).map_err(|e| e.to_string())?;
                    update_aircraft_db(
                        &c,
                        &url,
                        &p.data_dir.join("tmp"),
                        &db,
                        Some(&cancel),
                        &mut |m| progress(m.to_string()),
                    )
                    .map_err(|e| e.to_string())
                });
                finish(r.map(|_| ()), cancel_err(&cancel))
            },
        );

        {
            let p = paths.clone();
            let cancel = em.cancel.clone();
            let em2 = em.clone();
            let mut last_max_size = 0.0;
            let every = Schedule {
                run: true,
                periodic: true,
                ..sched
            };
            svc.spawn_job("bg-positions", every, em.clone(), None, move |_| {
                let r = PositionDb::open(&p.data_dir)
                    .map_err(|e| e.to_string())
                    .and_then(|db| {
                        maintain(&db, &MaintainerConfig::default(), &mut last_max_size)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    });
                if let Err(e) = r {
                    if !cancel_err(&cancel) {
                        em2.send(ServiceEvent::Error(format!(
                            "position database: {}",
                            strip_query(&e)
                        )));
                    }
                }
                Some((LedState::UpToDate, String::new()))
            });
        }

        for model in ElevationModel::ALL {
            if !settings.elevation_enabled.is_enabled(model) {
                continue;
            }
            let p = paths.clone();
            let base = settings.gui.elevation_base_url.clone();
            // "Download missing elevation data" off: only check what is stored
            let download = settings.gui.elevation_download;
            let cancel = em.cancel.clone();
            let sea = sea_file(&p.data_dir, model);
            let name = format!("bg-{}", model.dir_name().to_lowercase());
            svc.spawn_job(
                &name,
                sched,
                em.clone(),
                Some(Led::Elevation(model)),
                move |progress| {
                    // Only downloads: the shared bounds and COMPLETE belong to the path
                    // service's `prepare_area`; the calculator checks the area itself.
                    let r = if download {
                        client().map_err(ElevationError::Http).and_then(|c| {
                            download_area(
                                &p.store,
                                &c,
                                &base,
                                model,
                                rect,
                                &sea,
                                Some(&cancel),
                                &mut |m| progress(m),
                            )
                        })
                    } else {
                        check_area(&p.store, model, rect, &sea, Some(&cancel), &mut |m| {
                            progress(m)
                        })
                    };
                    Some(match r {
                        Err(ElevationError::Cancelled) => return None,
                        Ok(a) if !a.errors.is_empty() => (LedState::Error, a.errors.join("\n")),
                        Ok(a) if a.missing.is_empty() => (LedState::UpToDate, String::new()),
                        Ok(a) => (
                            LedState::Empty,
                            format!("missing squares: {}", a.missing.join(", ")),
                        ),
                        Err(e) => (LedState::Error, e.to_string()),
                    })
                },
            );
            // C# starts the calculators only in the OnStartup / Periodically branches (MapDlg.cs:2278, 2383)
            if bg.calculations_enable && sched.run {
                svc.spawn_calculator(
                    Calculator {
                        paths: paths.clone(),
                        settings: settings.clone(),
                        model,
                        cancel: em.cancel.clone(),
                        timing: Timing::default(),
                    },
                    &em,
                    sched.periodic.then_some(sched.period),
                );
            }
        }

        // Started with the updaters (MapDlg.cs:2288, :2393), after the calculators there.
        if sched.run {
            svc.spawn_map_preloader(
                preload_gate(settings),
                paths.data_dir.clone(),
                bg.map_mirror_url.clone(),
                rect,
                sched,
                &em,
            );
        }
        svc
    }

    /// The map preloader: its texts go to the calculations status line (`SayCalculations`,
    /// MapDlg.cs:9184-9187), a text already shown is skipped. It opens its own connection to
    /// the tile store, so the map view never waits for its writes.
    fn spawn_map_preloader(
        &self,
        gate: PreloadGate,
        data_dir: PathBuf,
        mirror_url: String,
        area: GRect,
        sched: Schedule,
        em: &Emitter,
    ) {
        if gate == PreloadGate::Skip {
            return;
        }
        self.running.fetch_add(1, Ordering::SeqCst);
        let guard = RunningGuard(self.running.clone());
        let em = em.clone();
        let r = std::thread::Builder::new()
            .name("bg-mappreloader".into())
            .spawn(move || {
                let _guard = guard;
                let mut last = String::new();
                let mut say = |m: String| {
                    let m = strip_query(&m);
                    if m != last {
                        em.send(ServiceEvent::Calculations {
                            generation: em.generation,
                            text: m.clone(),
                        });
                        last = m;
                    }
                };
                let max_zoom = match gate {
                    PreloadGate::Run { max_zoom } => max_zoom,
                    PreloadGate::NoMirror(provider) => return say(no_mirror_text(&provider)),
                    PreloadGate::ProviderMirror(host) => return say(provider_mirror_text(&host)),
                    PreloadGate::Skip => return,
                };
                say("MapPreloader started.".into());
                loop {
                    if em.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let ready = preloader_client().and_then(|c| {
                        TileStore::open(&store_path(&data_dir))
                            .map(|s| (c, s))
                            .map_err(|e| e.to_string())
                    });
                    match ready {
                        Ok((client, store)) => {
                            let list = preload_tiles(&area, PRELOAD_MIN_ZOOM, max_zoom);
                            let Some(o) = preload_run(
                                &store,
                                &client,
                                &mirror_url,
                                list,
                                PRELOAD_DELAY,
                                &em.cancel,
                                &mut say,
                            ) else {
                                return;
                            };
                            say(finished_text(&o));
                        }
                        Err(e) => say(format!("MapPreloader: {e}")),
                    }
                    if !sched.periodic || sleep_cancellable(sched.period, &em.cancel) {
                        return;
                    }
                }
            });
        if r.is_err() {
            self.running.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// The `PathCalculator` of one model: starts when the stations and the model's updater have
    /// finished a run (`BackgroundIdle`), repeats like the updaters.
    fn spawn_calculator(&self, calc: Calculator, em: &Emitter, period: Option<Duration>) {
        self.running.fetch_add(1, Ordering::SeqCst);
        let guard = RunningGuard(self.running.clone());
        let name = format!("calc-{}", calc.model.dir_name().to_lowercase());
        let (em, idle) = (em.clone(), em.idle_set.clone());
        let r = std::thread::Builder::new().name(name).spawn(move || {
            let _guard = guard;
            let mut last = String::new();
            calc.run_loop(&idle, period, &mut |m| {
                // `SayCalculations` skips a text that is already shown
                if m != last {
                    em.send(ServiceEvent::Calculations {
                        generation: em.generation,
                        text: m.clone(),
                    });
                    last = m;
                }
            });
        });
        if r.is_err() {
            self.running.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn spawn_job(
        &self,
        name: &str,
        sched: Schedule,
        em: Emitter,
        led: Option<Led>,
        mut run: impl FnMut(&mut dyn FnMut(String)) -> Outcome + Send + 'static,
    ) {
        self.running.fetch_add(1, Ordering::SeqCst);
        let guard = RunningGuard(self.running.clone());
        let r = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _guard = guard;
                if !sched.run {
                    // nothing to run: tell the PathCalculator not to wait for this job
                    if let Some(l) = led {
                        em.idle(l);
                    }
                    return;
                }
                loop {
                    if em.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(l) = led {
                        em.led(l, LedState::Updating, led_text(LedState::Updating).into());
                    }
                    let mut progress = |m: String| {
                        if let Some(l) = led {
                            em.led(
                                l,
                                LedState::Updating,
                                format!("{}\n{}", led_text(LedState::Updating), strip_query(&m)),
                            );
                        }
                    };
                    let Some((state, detail)) = run(&mut progress) else {
                        return;
                    };
                    if let Some(l) = led {
                        let mut text = led_text(state).to_string();
                        if !detail.is_empty() {
                            text = format!("{text}\n{}", strip_query(&detail));
                        }
                        em.led(l, state, text);
                        em.idle(l);
                    }
                    if !sched.periodic || sleep_cancellable(sched.period, &em.cancel) {
                        return;
                    }
                }
            });
        if r.is_err() {
            self.running.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Number of job threads still alive.
    pub fn running_jobs(&self) -> Arc<AtomicUsize> {
        self.running.clone()
    }

    /// Set the cancel flag and detach: threads end after the request in flight; none reports
    /// anything after this call.
    pub fn stop(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for BackgroundService {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn cancel_err(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn finish(r: Result<(), String>, cancelled: bool) -> Outcome {
    if cancelled {
        return None;
    }
    Some(match r {
        Ok(()) => (LedState::UpToDate, String::new()),
        Err(e) => (LedState::Error, e),
    })
}
