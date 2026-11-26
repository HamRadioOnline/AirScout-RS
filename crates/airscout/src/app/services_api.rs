//! The real and failed implementations of [`Services`].

use super::*;
use crate::analysis::AnalysisJob;
use crate::cat_service::CatIntegration;
use crate::jobs::JobRunner;
use crate::net_addrs::NetAddrs;
use crate::track_outputs::TrackConfig;
use crate::track_service::{TrackCtx, TrackService};
use crate::tracker::{TrackInput, TrackInputCell, TrackShown};
use crate::web_api::WebCtx;
use crate::web_service::{keeps_server, web_config, WebService, WebState};
use crate::wintest_nearest::NearestSettings;
use crate::wintest_service::{WinTestConfig, WinTestCtx, WinTestService};
use as_web::server::WebConfig;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

/// The real services: path thread plus feed worker, events into one channel.
pub struct RealServices {
    paths: Paths,
    path_service: Option<PathService>,
    feed: Option<FeedService>,
    horizon: Option<HorizonHandle>,
    background: Option<BackgroundService>,
    /// Running-worker counter shared by every `BackgroundService` generation: it only decreases
    /// when a worker actually exits, so a restart never loses a still-running older worker (#155).
    background_running: Arc<AtomicUsize>,
    speclab: Option<SpecLabService>,
    /// Cancel flag of the running analysis job.
    analysis: Option<Arc<AtomicBool>>,
    /// Cancel flags of the maintenance jobs that may still run (see `run_maintenance`).
    maintenance: Vec<(u64, Arc<AtomicBool>)>,
    /// Signal level history, opened on the first Spectrum Lab start.
    signals: Option<Arc<SignalDb>>,
    /// Enabled elevation models of the settings the background workers got last.
    elevation: as_settings::ElevationEnabled,
    /// Win-Test receiver, while `server.activate`.
    wintest: Option<WinTestService>,
    /// Track worker, while `track.activate`.
    track: Option<TrackService>,
    /// The app's tracking input; one cell for the whole lifetime, so a restarted Track worker
    /// steps from the current input at once.
    track_input: TrackInputCell,
    /// CAT worker (while `cat.activate`) and the Doppler cell the Track worker writes.
    cat: CatIntegration,
    /// What the Win-Test receiver answers ASSETPATH with; the app updates it live.
    nearest: Arc<RwLock<NearestSettings>>,
    /// Web server with the config it was started for, while `server.activate`.
    web: Option<(WebConfig, WebService)>,
    /// The settings the web routes read; one for the whole lifetime.
    web_live: Arc<RwLock<Settings>>,
    /// Interface addresses of the integration sockets.
    net: NetAddrs,
    tx: Sender<ServiceEvent>,
}

impl RealServices {
    pub fn new(paths: Paths, tx: Sender<ServiceEvent>) -> Self {
        let path_service = PathService::spawn(paths.clone(), tx.clone());
        Self {
            paths,
            path_service: Some(path_service),
            feed: None,
            horizon: None,
            background: None,
            background_running: Arc::new(AtomicUsize::new(0)),
            speclab: None,
            analysis: None,
            maintenance: Vec::new(),
            signals: None,
            elevation: Default::default(),
            wintest: None,
            track: None,
            track_input: TrackInputCell::default(),
            cat: CatIntegration::default(),
            nearest: Arc::new(RwLock::new(NearestSettings::from_settings(
                &Settings::default(),
            ))),
            web: None,
            web_live: Arc::new(RwLock::new(Settings::default())),
            net: NetAddrs::default(),
            tx,
        }
    }

    /// Other interface addresses for the integration sockets (tests: loopback).
    pub fn with_net(mut self, net: NetAddrs) -> Self {
        self.net = net;
        self
    }

    /// Stops the web server and starts one for the settings, unless its config (activation,
    /// port, bind) is unchanged and it is bound: then it keeps its connections (D3). A
    /// server that is not bound yet is restarted, so each Options close makes a fresh bind
    /// attempt and reports a lasting bind error again. Each server gets a fresh stop flag;
    /// stopping sets the old one, which cancels its path calculations.
    fn restart_web_if_changed(&mut self, s: &Settings, cache: &Arc<PlaneInfoCache>) {
        let want = web_config(s, self.net);
        let keep = match &self.web {
            Some((c, w)) => keeps_server(c, &w.state(), want.as_ref()),
            None => want.is_none(),
        };
        if keep {
            return;
        }
        if let Some((_, mut w)) = self.web.take() {
            w.stop();
        }
        if let Some(cfg) = want {
            let ctx = WebCtx {
                paths: self.paths.clone(),
                cache: cache.clone(),
                live: self.web_live.clone(),
                aircraft: None,
                tmp_dir: self.paths.data_dir.join("tmp"),
                stop: Arc::new(AtomicBool::new(false)),
            };
            let svc = WebService::start(cfg.clone(), ctx, self.tx.clone());
            self.web = Some((cfg, svc));
        }
    }
}

impl Services for RealServices {
    fn request_path(&mut self, req: PathRequest) {
        if let Some(p) = &self.path_service {
            p.request(req);
        }
    }

    fn start_feed(
        &mut self,
        feed_id: u64,
        settings: &Settings,
        cache: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        self.stop_feed();
        let listener = web_config(settings, self.net)
            .map(|config| self.web_local_addr().unwrap_or(config.bind));
        let f = FeedService::start_with_listener(
            &self.paths,
            settings,
            cache,
            self.tx.clone(),
            feed_id,
            listener,
        )
        .map_err(|e| e.to_string())?;
        self.feed = Some(f);
        Ok(())
    }

    fn stop_feed(&mut self) {
        if let Some(f) = self.feed.take() {
            f.stop();
        }
    }

    fn request_ground_elevations(&mut self, id: u64, my: GPoint, dx: GPoint) {
        let store = self.paths.store.clone();
        let en = self.elevation;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let at = |p: GPoint| crate::services::ground_elevation(&store, &en, p.lat, p.lon);
            let _ = tx.send(ServiceEvent::TrackHeights {
                id,
                my_m: at(my),
                dx_m: at(dx),
            });
        });
    }

    fn start_background(&mut self, settings: &Settings, generation: u64) {
        self.elevation = settings.elevation_enabled;
        // the old service is cancelled and detached; a fresh flag keeps it silent for good. The
        // new generation shares `background_running`, so a worker of the old one that is still
        // inside a request stays counted until it exits (#155).
        self.stop_background();
        let svc = BackgroundService::start(
            self.paths.clone(),
            settings,
            self.tx.clone(),
            Arc::new(AtomicBool::new(false)),
            generation,
            self.background_running.clone(),
        );
        self.background = Some(svc);
    }

    fn stop_background(&mut self) {
        if let Some(b) = self.background.take() {
            b.stop();
        }
    }

    fn background_jobs(&self) -> usize {
        self.background_running.load(Ordering::SeqCst)
    }

    fn restart_background(&mut self, settings: &Settings, generation: u64) {
        self.start_background(settings, generation);
    }

    fn start_speclab(&mut self, settings: &Settings, record: Arc<AtomicBool>) {
        self.stop_speclab();
        let signals = match &self.signals {
            Some(s) => s.clone(),
            None => match SignalDb::open(&self.paths.data_dir) {
                Ok(s) => {
                    let s = Arc::new(s);
                    self.signals = Some(s.clone());
                    s
                }
                Err(e) => {
                    let _ = self
                        .tx
                        .send(ServiceEvent::SpectrumError(format!("signal database: {e}")));
                    return;
                }
            },
        };
        self.speclab = Some(SpecLabService::start(
            &settings.speclab,
            signals,
            record,
            self.tx.clone(),
            Arc::new(AtomicBool::new(false)),
        ));
    }

    fn stop_speclab(&mut self) {
        if let Some(s) = self.speclab.take() {
            s.stop();
        }
    }

    fn run_analysis(&mut self, id: u64, job: AnalysisJob) {
        self.cancel_analysis();
        let cancel = Arc::new(AtomicBool::new(false));
        self.analysis = Some(cancel.clone());
        let dir = self.paths.data_dir.clone();
        JobRunner::spawn("analysis", id, cancel, self.tx.clone(), move |ctx| {
            crate::analysis::run(ctx, &dir, job)
        });
    }

    fn cancel_analysis(&mut self) {
        if let Some(c) = self.analysis.take() {
            c.store(true, Ordering::Relaxed);
        }
    }

    fn run_maintenance(&mut self, id: u64, job: crate::maintenance::MaintenanceJob) {
        let paths = self.paths.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        // a flag only the list holds belongs to a finished job (its thread dropped the other)
        self.maintenance.retain(|(_, c)| Arc::strong_count(c) > 1);
        self.maintenance.push((id, cancel.clone()));
        // The area download writes the tile store and elevation database like the background
        // updaters and calculators do: it counts as a background job while it runs, so a
        // destructive maintenance is refused until it ends (#155).
        let guard = match &job {
            crate::maintenance::MaintenanceJob::DownloadArea { .. } => {
                self.background_running.fetch_add(1, Ordering::SeqCst);
                Some(crate::background::RunningGuard(
                    self.background_running.clone(),
                ))
            }
            _ => None,
        };
        JobRunner::spawn("maintenance", id, cancel, self.tx.clone(), move |ctx| {
            let _guard = guard;
            crate::maintenance::run(ctx, &paths, job)
        });
    }

    fn cancel_maintenance(&mut self, id: u64) {
        if let Some((_, c)) = self.maintenance.iter().find(|(i, _)| *i == id) {
            c.store(true, Ordering::Relaxed);
        }
    }

    fn set_near_field_suppression(&mut self, m: f64) {
        self.paths.engine.set_near_field_suppression_m(m);
    }

    fn set_my(&mut self, call: &str, lat: f64, lon: f64) {
        self.paths.set_my_station(MyStation {
            call: call.into(),
            lat,
            lon,
        });
    }

    fn station_position(&self, call: &str, loc: &str) -> Option<GPoint> {
        let l = self.paths.stations.location_find(call, Some(loc)).ok()??;
        Some(GPoint::new(l.lat, l.lon))
    }

    fn qrv_get_all(&self, call: &str, loc: &str) -> Vec<Qrv> {
        self.paths
            .stations
            .qrv_get_all(call, loc)
            .unwrap_or_default()
    }

    fn qrv_write(&mut self, qrvs: &[Qrv]) -> Result<usize, String> {
        let st = &self.paths.stations;
        let n = st
            .qrv_bulk_insert_or_update_if_newer(qrvs)
            .map_err(|e| e.to_string())?;
        if n > 0 {
            // the background calculator reruns on a user edit (antenna heights); own marker, as
            // "QRV" is the stations updater's last-import marker
            st.set_table_timestamp(as_stations::QRV_USER_EDIT_MARKER, Utc::now())
                .map_err(|e| e.to_string())?;
        }
        Ok(n)
    }

    fn obstruction_load(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
    ) -> Result<LocalObstruction, String> {
        self.paths
            .store
            .local_obstruction_find_or_create_default(model, lat, lon, true)
            .map_err(|e| e.to_string())
    }

    fn obstruction_store(
        &mut self,
        model: ElevationModel,
        o: &LocalObstruction,
    ) -> Result<bool, String> {
        self.paths
            .store
            .local_obstruction_insert_or_update_if_newer(model, o, Utc::now())
            .map_err(|e| e.to_string())
    }

    fn start_horizon(&mut self, req: HorizonRequest) -> Result<(), String> {
        // dropping the old handle cancels its calculation
        self.horizon = Some(HorizonService::start(
            self.paths.clone(),
            req,
            self.tx.clone(),
        ));
        Ok(())
    }

    fn cancel_horizon(&mut self) {
        self.horizon = None;
    }

    fn start_integrations(&mut self, s: &Settings, generation: u64, cache: Arc<PlaneInfoCache>) {
        self.set_web_settings(s);
        self.restart_web_if_changed(s, &cache);
        self.stop_integrations();
        self.set_nearest_settings(NearestSettings::from_settings(s));
        // the services bind and connect on their own threads
        if s.server.activate {
            self.wintest = Some(WinTestService::start(
                WinTestConfig::from_settings(s, self.net),
                WinTestCtx {
                    paths: self.paths.clone(),
                    cache: cache.clone(),
                    live: self.nearest.clone(),
                    tx: self.tx.clone(),
                    generation,
                },
            ));
        }
        // CAT first: the Track worker pushes into the cell of this CAT start
        self.cat.start(s, &self.tx, generation);
        if s.track.activate {
            let cfg = TrackConfig::from_settings(s, &self.paths.data_dir, self.net);
            self.track = Some(TrackService::start(
                cfg,
                TrackCtx {
                    input: self.track_input.clone(),
                    cache,
                    doppler: self.cat.cell(),
                    tx: self.tx.clone(),
                    generation,
                },
            ));
        }
    }

    /// The web server is not in the C# stop list (`MapDlg.cs:2477-2503`): it keeps running
    /// in HISTORY and while Options is open. `Drop` stops it.
    fn stop_integrations(&mut self) {
        if let Some(w) = self.wintest.take() {
            w.stop();
        }
        if let Some(mut t) = self.track.take() {
            t.stop();
        }
        self.cat.stop();
    }

    fn stop_integrations_and_wait(&mut self, max: Duration) {
        // taken first: `stop_integrations` would drop the handle that `wait` needs
        let cat = self.cat.take_stopped();
        self.stop_integrations();
        if let Some(c) = cat {
            c.wait(max);
        }
    }

    fn track_input(&mut self, i: TrackInput) {
        *self
            .track_input
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = i;
    }

    fn track_shown(&self) -> Option<TrackShown> {
        self.track.as_ref().map(TrackService::shown)
    }

    fn set_nearest_settings(&mut self, n: NearestSettings) {
        *self.nearest.write().unwrap_or_else(|e| e.into_inner()) = n;
    }

    fn wintest_local_addr(&self) -> Option<SocketAddr> {
        self.wintest.as_ref().and_then(|w| w.local_addr())
    }

    fn set_web_settings(&mut self, s: &Settings) {
        *self.web_live.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
    }

    fn web_local_addr(&self) -> Option<SocketAddr> {
        self.web.as_ref().and_then(|(_, w)| w.local_addr())
    }

    fn web_state(&self) -> WebState {
        self.web.as_ref().map_or(WebState::Off, |(_, w)| w.state())
    }

    fn load_stations(&mut self, area: Option<GRect>) {
        let db = self.paths.stations.clone();
        let tx = self.tx.clone();
        let r = std::thread::Builder::new()
            .name("load-stations".into())
            .spawn(move || {
                let list = match db.location_get_all(area) {
                    Ok(v) => v.into_iter().map(|l| (l.call, l.loc)).collect(),
                    Err(e) => {
                        let _ = tx.send(ServiceEvent::Error(format!("station list: {e}")));
                        Vec::new()
                    }
                };
                let _ = tx.send(ServiceEvent::Stations(list));
            });
        if let Err(e) = r {
            let _ = self
                .tx
                .send(ServiceEvent::Error(format!("station list: {e}")));
            let _ = self.tx.send(ServiceEvent::Stations(Vec::new()));
        }
    }
}

impl Drop for RealServices {
    fn drop(&mut self) {
        self.stop_feed();
        self.stop_speclab();
        self.stop_background();
        self.stop_integrations();
        if let Some((_, mut w)) = self.web.take() {
            w.stop();
        }
        self.cancel_analysis();
        for (_, c) in &self.maintenance {
            c.store(true, Ordering::Relaxed);
        }
        // `PathService` joins its thread, which can take minutes during a download. Let a
        // detached thread do that; the process does not wait for it.
        if let Some(p) = self.path_service.take() {
            let _ = std::thread::Builder::new()
                .name("path-service-stop".into())
                .spawn(move || drop(p));
        }
    }
}

/// Services of an app whose data directory could not be opened: Play reports the reason.
pub struct FailedServices(pub String);

impl Services for FailedServices {
    fn request_path(&mut self, _req: PathRequest) {}
    fn start_feed(
        &mut self,
        _id: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn stop_feed(&mut self) {}
}
