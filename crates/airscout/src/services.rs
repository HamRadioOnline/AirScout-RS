//! GUI-free background services: path computation thread and feed worker wrapper. The app shell
//! drives them through channels and polls [`ServiceEvent`]s.

use crate::best_case::{effective_position, MODEL_ORDER};
use as_elevation::{
    prepare_area, ElevationError, ElevationModel, ElevationStore, ELV_MISSING, TILE_MISSING,
};
use as_feeds::worker::FeedEvent;
use as_geo::latlon::{self, GPoint, GRect};
use as_geo::maidenhead;
use as_propagation::{
    HorizonPoint, PathParams, PropagationEngine, PropagationHorizon, PropagationPath,
};
use as_settings::{ElevationEnabled, Settings};
use as_stations::{default_antenna_gain, default_antenna_height, StationDb};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;

pub use crate::feed_service::FeedService;

/// Errors of the services.
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("elevation: {0}")]
    Elevation(#[from] ElevationError),
    #[error("propagation: {0}")]
    Propagation(#[from] as_propagation::PropagationError),
    #[error("station database: {0}")]
    Station(#[from] as_stations::StationError),
    #[error("aircraft database: {0}")]
    Aircraft(#[from] as_aircraft::AircraftError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// The databases of one data directory (elevation, propagation, stations; the aircraft and
/// position databases are opened by [`FeedService`]).
#[derive(Clone)]
pub struct Paths {
    pub store: Arc<ElevationStore>,
    pub engine: Arc<PropagationEngine>,
    pub stations: Arc<StationDb>,
    pub data_dir: PathBuf,
    /// Set while the path service computes; the background calculator waits then
    /// (`Background_Calculations_ThreadWait`).
    pub ui_busy: Arc<AtomicBool>,
    /// My station as the app currently has it; the calculator reads it per path, like the
    /// original reads `MyCall`/`MyLat`/`MyLon` from the settings each time.
    pub my: Arc<RwLock<MyStation>>,
}

/// Call sign and position of My.
#[derive(Clone, Debug, PartialEq)]
pub struct MyStation {
    pub call: String,
    pub lat: f64,
    pub lon: f64,
}

impl Default for MyStation {
    /// Unknown: the position is invalid until the app has set it, so nothing is calculated.
    fn default() -> Self {
        Self {
            call: String::new(),
            lat: f64::NAN,
            lon: f64::NAN,
        }
    }
}

impl Paths {
    pub fn my_station(&self) -> MyStation {
        self.my.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_my_station(&self, my: MyStation) {
        *self.my.write().unwrap_or_else(|e| e.into_inner()) = my;
    }
}

/// Open all databases in `dir` (created if missing), with the file layout of the CLI.
pub fn open_data(dir: &Path) -> Result<Paths, AppError> {
    std::fs::create_dir_all(dir)?;
    let store = Arc::new(ElevationStore::open(dir)?);
    let engine = Arc::new(PropagationEngine::open(store.clone(), dir)?);
    let stations = Arc::new(StationDb::open(&dir.join("stations.db3"))?);
    Ok(Paths {
        store,
        engine,
        stations,
        data_dir: dir.to_path_buf(),
        ui_busy: Arc::new(AtomicBool::new(false)),
        my: Arc::new(RwLock::new(MyStation::default())),
    })
}

/// What the services report to the app.
#[derive(Debug, Clone)]
pub enum ServiceEvent {
    /// Progress or a non-fatal condition worth showing in the status line.
    Status(String),
    Error(String),
    /// Elevation data note for the path request `id` (missing squares, download error). It stays
    /// visible after the path result arrives, until the next Play.
    PathNote {
        id: u64,
        text: String,
    },
    /// `terrain`: one ground elevation [m] per entry of `path.info_points()` (same order; empty if
    /// there are none), sampled in the selected model; missing values count as 0. The gains are
    /// the QRV antenna gains (stored value, or the band default if that is 0).
    PathReady {
        /// `PathRequest::id` of the request this answers.
        id: u64,
        /// Position of the target in `PathRequest::targets`.
        index: usize,
        call: String,
        path: PropagationPath,
        terrain: Vec<f64>,
        my_gain_dbi: f64,
        dx_gain_dbi: f64,
    },
    PathFailed {
        id: u64,
        index: usize,
        error: String,
    },
    /// Ground elevation [m] of My and DX for the tracking heights of Play `id`
    /// (`Services::request_ground_elevations`).
    TrackHeights {
        id: u64,
        my_m: f64,
        dx_m: f64,
    },
    /// All targets of request `id` have been answered (`PathReady` or `PathFailed`).
    PathsDone {
        id: u64,
    },
    /// Event of the feed started with id `feed_id` (see `Services::start_feed`); the app drops
    /// events of feeds it has replaced.
    Feed(u64, FeedEvent),
    /// One Spectrum Lab sample: the time stamp of the FFT and its maximum level [dB].
    Spectrum {
        at: DateTime<Utc>,
        level: f64,
    },
    /// A Spectrum Lab poll failed; the poller keeps going.
    SpectrumError(String),
    /// Answer of `Services::load_stations`: `(call, locator)` ordered by call.
    Stations(Vec<(String, String)>),
    /// One azimuth of the horizon request `id` is done. The azimuths run in parallel: they
    /// arrive in no particular order.
    HorizonPoint {
        id: u64,
        az: usize,
        point: HorizonPoint,
    },
    /// The horizon request `id` is complete (also when it was found stored: then no
    /// `HorizonPoint` came before).
    HorizonDone {
        id: u64,
        horizon: Box<PropagationHorizon>,
    },
    HorizonFailed {
        id: u64,
        error: String,
    },
    /// Event of the analysis job `id` (`Services::run_analysis`); the app drops those of
    /// superseded jobs.
    Job {
        id: u64,
        event: crate::jobs::JobEvent,
    },
    /// State of a status LED; `text` is its tooltip (credential-free).
    Led {
        /// Generation of the background service that sent it (`AppState::background_gen`).
        generation: u64,
        led: crate::background::Led,
        state: crate::background::LedState,
        text: String,
    },
    /// Progress of a background path calculator (`tsl_Calculations`).
    Calculations {
        generation: u64,
        text: String,
    },
    /// A background job finished a run (or has none to run): the PathCalculator waits for these.
    BackgroundIdle {
        generation: u64,
        led: crate::background::Led,
    },
    /// Event of the Track worker started with `generation` (`TrackService::start`).
    Track {
        generation: u64,
        event: crate::track_service::TrackEvent,
    },
    /// Command received by the Win-Test receiver started with `generation`
    /// (`WinTestService::start`).
    WinTest {
        generation: u64,
        cmd: crate::wintest_service::WinTestCommand,
    },
    /// Event of the CAT worker started with `generation` (`CatService::start`).
    Cat {
        generation: u64,
        event: as_cat::CatEvent,
    },
}

/// One DX station of a path request.
#[derive(Debug, Clone)]
pub struct PathTarget {
    pub call: String,
    pub pos: GPoint,
}

/// Path computations from My to each target: the SINGLE (one target) and MULTI branches of
/// `MapDlg.UpdatePaths`.
#[derive(Debug, Clone)]
pub struct PathRequest {
    /// Echoed in the answers, so the app can drop results of superseded requests.
    pub id: u64,
    pub my: GPoint,
    pub my_call: String,
    pub targets: Vec<PathTarget>,
    pub settings: Settings,
}

#[derive(Default)]
struct Queue {
    pending: Option<PathRequest>,
    stop: bool,
    paused: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Thread computing propagation paths. Only the newest request is kept while one is computing.
pub struct PathService {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl PathService {
    pub fn spawn(paths: Paths, tx: Sender<ServiceEvent>) -> Self {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let q = queue.clone();
        let thread = std::thread::Builder::new()
            .name("path-service".into())
            .spawn(move || run_paths(&paths, &q, &tx))
            .ok();
        Self { queue, thread }
    }

    /// Queue a computation; an older request that has not started yet is dropped.
    pub fn request(&self, req: PathRequest) {
        lock(&self.queue.0).pending = Some(req);
        self.queue.1.notify_one();
    }

    /// Test hook: keep the worker from starting new computations until [`Self::resume`].
    #[doc(hidden)]
    pub fn pause(&self) {
        lock(&self.queue.0).paused = true;
    }

    /// Test hook: undo [`Self::pause`].
    #[doc(hidden)]
    pub fn resume(&self) {
        lock(&self.queue.0).paused = false;
        self.queue.1.notify_one();
    }

    /// Stop the thread after the computation in progress and wait for it.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        {
            let mut q = lock(&self.queue.0);
            q.stop = true;
            q.pending = None;
        }
        self.queue.1.notify_one();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PathService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Marks the UI as busy for its lifetime, also when the computation panics.
struct BusyGuard<'a>(&'a AtomicBool);

impl<'a> BusyGuard<'a> {
    fn set(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn run_paths(paths: &Paths, queue: &(Mutex<Queue>, Condvar), tx: &Sender<ServiceEvent>) {
    loop {
        let req = {
            let mut q = lock(&queue.0);
            loop {
                if q.stop {
                    return;
                }
                if !q.paused {
                    if let Some(r) = q.pending.take() {
                        break r;
                    }
                }
                q = queue.1.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _busy = BusyGuard::set(&paths.ui_busy);
            compute_all(paths, &req, tx)
        }));
        if outcome.is_err() {
            for index in 0..req.targets.len() {
                let error = "internal error while computing the path".to_string();
                let _ = tx.send(ServiceEvent::PathFailed {
                    id: req.id,
                    index,
                    error,
                });
            }
        }
        if tx.send(ServiceEvent::PathsDone { id: req.id }).is_err() {
            return;
        }
    }
}

/// `GetElevation` (MapDlg.cs:3649): the models of `order` (the GUI passes `MODEL_ORDER`: ASTER1,
/// ASTER3, SRTM1, SRTM3, GLOBE; the web server its own order), skipping models whose
/// `Elevation_*_Enabled` flag is off (`logic-findings.md` #87). The walk moves on to the next
/// model only while the value is `ELV_MISSING`; `TILE_MISSING` stops it and yields 0. A model
/// that cannot answer (no database, bad lookup) counts as `ELV_MISSING`.
pub(crate) fn ground_elevation_with(
    enabled: &ElevationEnabled,
    order: &[ElevationModel],
    mut lookup: impl FnMut(ElevationModel) -> Result<i16, ElevationError>,
) -> f64 {
    let mut elv = ELV_MISSING;
    for &m in order {
        if elv != ELV_MISSING {
            break;
        }
        if !enabled.is_enabled(m) {
            continue;
        }
        elv = lookup(m).unwrap_or(ELV_MISSING);
    }
    if elv <= TILE_MISSING {
        0.0
    } else {
        f64::from(elv)
    }
}

pub(crate) fn ground_elevation(
    store: &ElevationStore,
    enabled: &ElevationEnabled,
    lat: f64,
    lon: f64,
) -> f64 {
    ground_elevation_with(enabled, &MODEL_ORDER, |m| {
        store.elevation(m, lat, lon, false)
    })
}

/// Bounding box of the path points (one per km), padded by 0.01 degree. The antimeridian is not
/// supported (`logic-findings.md` #15).
fn path_bbox(a: GPoint, b: GPoint) -> Result<GRect, String> {
    let d = latlon::distance(a.lat, a.lon, b.lat, b.lon);
    let brg = latlon::bearing(a.lat, a.lon, b.lat, b.lon);
    let (mut min_lat, mut min_lon, mut max_lat, mut max_lon) =
        (90.0f64, 180.0f64, -90.0f64, -180.0f64);
    let samples = (0..=d as usize).map(|k| latlon::destination_point(a.lat, a.lon, brg, k as f64));
    for p in samples.chain([a, b]) {
        min_lat = min_lat.min(p.lat);
        min_lon = min_lon.min(p.lon);
        max_lat = max_lat.max(p.lat);
        max_lon = max_lon.max(p.lon);
    }
    if min_lon < -180.0 || max_lon > 180.0 || max_lon - min_lon > 180.0 {
        return Err("path crosses the antimeridian (±180°), not supported yet".into());
    }
    Ok(GRect::new(
        (min_lat - 0.01).max(-90.0),
        (min_lon - 0.01).max(-180.0),
        (max_lat + 0.01).min(90.0),
        (max_lon + 0.01).min(180.0),
    ))
}

pub(crate) fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(as_net::user_agent())
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("cannot create HTTP client: {e}"))
}

/// Station antenna height [m] and gain [dBd]: stored values, or the band defaults if a stored one
/// is 0 (`UpdatePaths`).
fn antenna(
    db: &StationDb,
    call: &str,
    p: GPoint,
    band: as_geo::band::Band,
) -> Result<(f64, f64), String> {
    let loc = maidenhead::loc_from_lat_lon(p.lat, p.lon, false, 3, true)
        .ok()
        .flatten()
        .ok_or_else(|| format!("invalid position {}, {}", p.lat, p.lon))?;
    antenna_of_loc(db, call, &loc, band)
}

/// [`antenna`] for a station given by its locator.
pub(crate) fn antenna_of_loc(
    db: &StationDb,
    call: &str,
    loc: &str,
    band: as_geo::band::Band,
) -> Result<(f64, f64), String> {
    let q = db
        .qrv_find_or_create_default(call, loc, band)
        .map_err(|e| format!("station database: {e}"))?;
    let height = if q.antenna_height == 0.0 {
        default_antenna_height(band)
    } else {
        q.antenna_height
    };
    let gain = if q.antenna_gain == 0.0 {
        default_antenna_gain(band)
    } else {
        q.antenna_gain
    };
    Ok((height, gain))
}

/// Compute every target of `req` and send `PathReady`/`PathFailed` for each.
fn compute_all(paths: &Paths, req: &PathRequest, tx: &Sender<ServiceEvent>) {
    let model = req.settings.elevation_enabled.effective_model();
    let band = req.settings.band;
    let my = if model == ElevationModel::None {
        Err("no elevation model enabled".to_string())
    } else {
        // the antenna comes from the locator of the station, the path from its (best case) position
        antenna(&paths.stations, &req.my_call, req.my, band).and_then(|a| {
            effective_position(&paths.store, &req.settings, req.my).map(|pos| (a, pos))
        })
    };
    for (index, target) in req.targets.iter().enumerate() {
        let result = my
            .clone()
            .and_then(|(my, my_pos)| compute(paths, req, model, (my, my_pos), target, tx));
        let event = match result {
            Ok((path, terrain, my_gain_dbi, dx_gain_dbi)) => ServiceEvent::PathReady {
                id: req.id,
                index,
                call: target.call.clone(),
                path,
                terrain,
                my_gain_dbi,
                dx_gain_dbi,
            },
            Err(error) => ServiceEvent::PathFailed {
                id: req.id,
                index,
                error,
            },
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

/// Local obstruction at `from` (`UpdatePaths`, `PathCalculator`): the stored one of `model` gives
/// the elevation angle [rad] for the bearing `from` -> `to`, seen from the antenna height above
/// ground (`o.GetObstruction(antennaheight, bearing)`: the ground elevation is not added).
/// `f64::MIN` if there is none.
/// The squares of `model` known to have no file on the server (sea), shared by the path
/// service, the background updater and the calculator.
pub(crate) fn sea_file(data_dir: &Path, model: ElevationModel) -> PathBuf {
    data_dir.join(format!(
        "{}-sea-squares.txt",
        model.dir_name().to_lowercase()
    ))
}

pub(crate) fn local_obstruction(
    store: &ElevationStore,
    model: ElevationModel,
    from: GPoint,
    to: GPoint,
    antenna_height: f64,
) -> Result<f64, String> {
    let bearing = latlon::bearing(from.lat, from.lon, to.lat, to.lon);
    Ok(store
        .local_obstruction_find(model, from.lat, from.lon)
        .map_err(|e| format!("local obstruction: {e}"))?
        .map_or(f64::MIN, |o| o.obstruction(antenna_height, bearing)))
}

type Computed = (PropagationPath, Vec<f64>, f64, f64);

fn compute(
    paths: &Paths,
    req: &PathRequest,
    model: ElevationModel,
    ((h1, my_gain), my_pos): ((f64, f64), GPoint),
    target: &PathTarget,
    tx: &Sender<ServiceEvent>,
) -> Result<Computed, String> {
    let s = &req.settings;
    let status = |m: String| {
        let _ = tx.send(ServiceEvent::Status(m));
    };
    let note = |text: String| {
        let _ = tx.send(ServiceEvent::PathNote { id: req.id, text });
    };
    let band = s.band;
    let (h2, dx_gain) = antenna(&paths.stations, &target.call, target.pos, band)?;
    let dx_pos = effective_position(&paths.store, s, target.pos)?;

    // `prepare_area` runs without download too: it decides whether the stored data is complete
    let rect = path_bbox(my_pos, dx_pos)?;
    let sea_file = sea_file(&paths.data_dir, model);
    let prepared = client().and_then(|c| {
        prepare_area(
            &paths.store,
            &c,
            &s.gui.elevation_base_url,
            model,
            rect,
            &sea_file,
            s.gui.elevation_download,
            &mut |m| status(m),
        )
        .map_err(|e| format!("elevation data: {e}"))
    });
    match prepared {
        Ok(a) if !a.complete => note(format!(
            "elevation data incomplete for this path, missing squares: {}",
            a.missing.join(", ")
        )),
        Ok(_) => {}
        // the path is computed anyway and comes out invalid
        Err(e) => note(e),
    }

    let local_obstruction = local_obstruction(&paths.store, model, my_pos, dx_pos, h1)?;

    let en = &s.elevation_enabled;
    let bs = s.current_band_settings();
    let p = PathParams {
        lat1: my_pos.lat,
        lon1: my_pos.lon,
        h1: ground_elevation(&paths.store, en, my_pos.lat, my_pos.lon) + h1,
        lat2: dx_pos.lat,
        lon2: dx_pos.lon,
        h2: ground_elevation(&paths.store, en, dx_pos.lat, dx_pos.lon) + h2,
        qrg_ghz: band.to_ghz(),
        radius_km: s.radius_km(),
        f1_clearance: bs.f1_clearance,
        step_width_m: model.default_step_width_m(),
        local_obstruction,
    };
    // The near-field suppression is set by the app (`Services::set_near_field_suppression`), never
    // per computation: it is not part of the stored path's key.
    let path = paths
        .engine
        .find_or_create_path(model, &p, true, None)
        .map_err(|e| format!("path calculation: {e}"))?
        .ok_or_else(|| "path calculation cancelled".to_string())?;
    let terrain = path
        .info_points()
        .unwrap_or_default()
        .iter()
        .map(|i| {
            paths
                .store
                .elevation(model, i.lat, i.lon, true)
                .map_or(0.0, f64::from)
        })
        .collect();
    Ok((path, terrain, my_gain, dx_gain))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> ElevationEnabled {
        ElevationEnabled {
            globe: true,
            srtm3: true,
            srtm1: true,
            aster3: true,
            aster1: true,
        }
    }

    #[test]
    fn busy_guard_clears_the_flag_also_on_panic() {
        let flag = AtomicBool::new(false);
        {
            let _g = BusyGuard::set(&flag);
            assert!(flag.load(Ordering::SeqCst));
        }
        assert!(!flag.load(Ordering::SeqCst));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = BusyGuard::set(&flag);
            panic!("computation failed");
        }));
        assert!(r.is_err());
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn elv_missing_falls_through_to_next_model() {
        let e = ground_elevation_with(&all(), &MODEL_ORDER, |m| {
            Ok(if m == ElevationModel::Srtm3 {
                123
            } else {
                ELV_MISSING
            })
        });
        assert_eq!(e, 123.0);
    }

    #[test]
    fn tile_missing_stops_the_walk_with_zero() {
        let mut asked = Vec::new();
        let e = ground_elevation_with(&all(), &MODEL_ORDER, |m| {
            asked.push(m);
            Ok(match m {
                ElevationModel::Aster1 => ELV_MISSING,
                ElevationModel::Aster3 => TILE_MISSING,
                _ => 500,
            })
        });
        assert_eq!(e, 0.0);
        assert_eq!(asked, [ElevationModel::Aster1, ElevationModel::Aster3]);
    }

    #[test]
    fn lookup_error_falls_through() {
        let e = ground_elevation_with(&all(), &MODEL_ORDER, |m| match m {
            ElevationModel::Globe => Ok(77),
            _ => Err(ElevationError::InvalidArgument("no database".into())),
        });
        assert_eq!(e, 77.0);
    }

    #[test]
    fn disabled_model_is_skipped_in_ground_elevation() {
        // GLOBE off, SRTM3 on: GLOBE has data but must not be asked
        let en = ElevationEnabled {
            globe: false,
            srtm3: true,
            srtm1: false,
            aster3: false,
            aster1: false,
        };
        let mut asked = Vec::new();
        let e = ground_elevation_with(&en, &MODEL_ORDER, |m| {
            asked.push(m);
            Ok(if m == ElevationModel::Srtm3 { 321 } else { 999 })
        });
        assert_eq!(e, 321.0);
        assert_eq!(asked, [ElevationModel::Srtm3]);
        // SRTM3 has no data: the walk ends without asking GLOBE
        let e = ground_elevation_with(&en, &MODEL_ORDER, |m| {
            Ok(if m == ElevationModel::Srtm3 {
                ELV_MISSING
            } else {
                999
            })
        });
        assert_eq!(e, 0.0);
    }
}
