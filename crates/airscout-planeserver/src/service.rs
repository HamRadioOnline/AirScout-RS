//! The planeserver core: up to [`MAX_FEEDS`] plane feeds fill one [`PlaneInfoCache`], and a
//! LAN web server answers `/planes.json` from that cache (C# `MainDlg.cs`, `Webserver.cs`).
//!
//! Each started feed runs in its own [`FeedWorker`] with its own event channel and a small
//! forwarder thread that writes the events into that feed's [`FeedStatus`] row (a
//! [`FeedEvent`] carries no slot). The server answers only `/planes.json` and the welcome page
//! `/`; everything else is 404, and no file is ever read for an answer.

use crate::config::{feed_needs_disclaimer, PlaneServerConfig, MAX_FEEDS, WELCOME};
use as_aircraft::db::AircraftDb;
use as_aircraft::PlaneInfoCache;
use as_feeds::pipeline::PipelineConfig;
use as_feeds::registry::feed_by_name;
use as_feeds::worker::{FeedEvent, FeedHandle, FeedWorker, WorkerConfig};
use as_feeds::{FeedArgs, PlaneFeed};
use as_web::assets::Asset;
use as_web::planes::{planes_json, PLANES_JSON_VERSION};
use as_web::query::Query;
use as_web::route::{ApiRoute, Route};
use as_web::server::{Api, WebConfig, WebServer};
use chrono::{DateTime, Utc};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// The answer of `/` (`Settings.settings:20`).
pub static WELCOME_ASSET: Asset = Asset {
    path: "",
    mime: "text/html",
    bytes: WELCOME.as_bytes(),
};

/// The longest [`Service::stop`] waits for the feed workers and the server thread. A feed
/// request still in flight after that ends on its own; its worker then sees the stop.
const STOP_LIMIT: Duration = Duration::from_millis(1500);

/// The planeserver routes: `/planes.json` and `/`, compared on the lower-cased path without
/// the query; everything else is 404. Unlike `as_web::route::route`, a query is allowed on
/// `/planes.json`: the AirScoutServer feed URL may carry one (`%MINLAT%`-style variables).
pub fn router(raw: &str) -> Route {
    let path = raw.split('?').next().unwrap_or("").to_lowercase();
    match path.as_str() {
        "/planes.json" => Route::Api(ApiRoute::Planes),
        "/" => Route::Asset(&WELCOME_ASSET),
        _ => Route::NotFound,
    }
}

/// The state of one configured feed slot.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedStatus {
    pub name: String,
    /// `starting`, `running`, `stopped`, or why the feed was not started.
    pub state: String,
    /// The time of the last successful cycle.
    pub last_ok: Option<DateTime<Utc>>,
    /// The last error of the worker (never holds credentials). Never cleared: compare
    /// `last_error_at` with `last_ok` to tell whether it is current.
    pub last_error: Option<String>,
    /// The time of the last error.
    pub last_error_at: Option<DateTime<Utc>>,
}

/// A snapshot of the service.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub started: DateTime<Utc>,
    /// The planes `/planes.json` serves now (`get_all(now, lifetime_min)`).
    pub planes: usize,
    pub feeds: Vec<FeedStatus>,
    /// `listening on <addr>`, `off`, `starting on <addr>` or the last server error.
    pub server: String,
    /// Set when the aircraft database in the data dir could not be opened and an in-memory
    /// database is used instead.
    pub db_error: Option<String>,
}

/// The feed slots of `cfg` that name a feed: `[none]` and empty slots are skipped.
fn slot_names(cfg: &PlaneServerConfig) -> impl Iterator<Item = &str> {
    cfg.feeds
        .iter()
        .map(String::as_str)
        .filter(|n| !n.is_empty() && *n != "[none]")
        .take(MAX_FEEDS)
}

/// One feed of the config, or the status text of why it does not start.
fn make_feed(cfg: &PlaneServerConfig, name: &str) -> Result<Box<dyn PlaneFeed>, String> {
    let mut feed = feed_by_name(name).ok_or_else(|| format!("unknown feed ({name})"))?;
    if feed_needs_disclaimer(feed.as_ref())
        && !cfg.feeds_disclaimer_accepted.iter().any(|a| a == name)
    {
        return Err(format!("disclaimer not accepted ({name})"));
    }
    if let Some(v) = cfg.feed_settings.get(name) {
        let t = v
            .as_table()
            .ok_or_else(|| format!("feed settings of {name} are not a table"))?;
        // `FeedError` texts name the key, never a value.
        feed.load_settings(t).map_err(|e| e.to_string())?;
    }
    if name == "AirScoutServer"
        && cfg
            .feed_settings
            .get(name)
            .and_then(|settings| settings.get("url"))
            .and_then(toml::Value::as_str)
            .is_none_or(str::is_empty)
    {
        return Err("AirScoutServer feed URL is unconfigured".into());
    }
    Ok(feed)
}

/// One entry per configured feed slot (`[none]` and empty slots are skipped). An `Err` is
/// the slot's status text: `unknown feed (<name>)`, `disclaimer not accepted (<name>)`, or
/// the `load_settings` error.
pub fn make_feeds(cfg: &PlaneServerConfig) -> Vec<Result<Box<dyn PlaneFeed>, String>> {
    slot_names(cfg).map(|n| make_feed(cfg, n)).collect()
}

/// The mutable part of the status.
struct Shared {
    feeds: Vec<FeedStatus>,
    server_error: Option<String>,
}

fn lock(m: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `/planes.json` from the cache.
struct PlanesApi {
    cache: Arc<PlaneInfoCache>,
    lifetime_min: i64,
}

impl Api for PlanesApi {
    fn answer(&self, route: ApiRoute, _raw_url: &str, _query: &Query) -> String {
        match route {
            ApiRoute::Planes => planes_json(
                &self.cache.get_all(Utc::now(), self.lifetime_min),
                PLANES_JSON_VERSION,
            ),
            // `router` routes nothing else here.
            _ => String::new(),
        }
    }
}

/// One running feed: its worker and the thread forwarding its events into the status.
struct Running {
    handle: FeedHandle,
    forwarder: Option<JoinHandle<()>>,
}

/// Writes the events of the feed in `slot` into its status row. Ends when the worker has
/// finished and dropped its sender.
fn forward(rx: Receiver<FeedEvent>, shared: Arc<Mutex<Shared>>, slot: usize) {
    for e in rx {
        let mut s = lock(&shared);
        let Some(f) = s.feeds.get_mut(slot) else {
            return;
        };
        match e {
            FeedEvent::Started => f.state = "running".into(),
            FeedEvent::Planes(_) => f.last_ok = Some(Utc::now()),
            FeedEvent::RejectedRecords(count) => {
                f.state = format!("running, skipped {count} invalid records");
            }
            FeedEvent::DroppedRecords(count) => {
                f.state = format!("running, dropped {count} pending observations");
            }
            FeedEvent::Error(text) => {
                f.last_error = Some(text);
                f.last_error_at = Some(Utc::now());
            }
            FeedEvent::Stopped => f.state = "stopped".into(),
        }
    }
}

/// Creates `dir`; the error as a status text.
fn create_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))
}

/// The running planeserver.
pub struct Service {
    started: DateTime<Utc>,
    lifetime_min: i64,
    cache: Arc<PlaneInfoCache>,
    shared: Arc<Mutex<Shared>>,
    feeds: Vec<Running>,
    web: Option<WebServer>,
    bind: SocketAddr,
    db_error: Option<String>,
}

impl Service {
    /// Starts the feeds of `feeds` (from [`make_feeds`]) and, if `cfg.server_active`, the
    /// server on `cfg.bind:cfg.port`.
    pub fn start(
        cfg: &PlaneServerConfig,
        feeds: Vec<Result<Box<dyn PlaneFeed>, String>>,
    ) -> Result<Service, String> {
        let web = WebConfig::new(SocketAddr::new(cfg.bind, cfg.port)).with_router(router);
        Self::start_with_web(cfg, feeds, web)
    }

    /// Like [`start`](Self::start), with the server limits and address of `web` (tests use
    /// short timeouts). The router is always [`router`].
    pub fn start_with_web(
        cfg: &PlaneServerConfig,
        feeds: Vec<Result<Box<dyn PlaneFeed>, String>>,
        web: WebConfig,
    ) -> Result<Service, String> {
        let dir = cfg.data_dir();
        let (tmp_dir, log_dir, database_dir) =
            (dir.join("tmp"), dir.join("log"), dir.join("database"));
        let dirs = [&dir, &tmp_dir, &log_dir, &database_dir]
            .into_iter()
            .try_for_each(|d| create_dir(d));
        let (db, db_error) = match dirs.and_then(|()| {
            AircraftDb::open(&dir).map_err(|e| format!("cannot open the aircraft database: {e}"))
        }) {
            Ok(db) => (db, None),
            Err(e) => {
                let db = AircraftDb::open_in_memory()
                    .map_err(|e| format!("cannot open an in-memory aircraft database: {e}"))?;
                (db, Some(format!("{e}; using an in-memory database")))
            }
        };
        let db = Arc::new(db);
        let cache = Arc::new(PlaneInfoCache::new());

        let my_lat = (cfg.min_lat + cfg.max_lat) / 2.0;
        let my_lon = (cfg.min_lon + cfg.max_lon) / 2.0;
        // The centre of the area, never a station: some feeds send these values out.
        let args = FeedArgs {
            app_dir: dir.clone(),
            data_dir: dir.clone(),
            tmp_dir,
            log_dir,
            database_dir,
            min_lat: cfg.min_lat,
            max_lat: cfg.max_lat,
            min_lon: cfg.min_lon,
            max_lon: cfg.max_lon,
            min_alt_m: cfg.min_alt_m,
            max_alt_m: cfg.max_alt_m,
            my_lat,
            my_lon,
            dx_lat: my_lat,
            dx_lon: my_lon,
            keep_history: false,
            airscout_listener: cfg.server_active.then_some(web.bind),
        };
        let worker_cfg = WorkerConfig {
            // 0 would make the worker loop without pause
            interval_s: cfg.interval_s.max(1),
            pipeline: PipelineConfig {
                extended_plausibility: cfg.extended_plausibility,
                max_error_dist: cfg.max_error_dist,
                log_errors: false,
            },
        };

        let names: Vec<&str> = slot_names(cfg).collect();
        let rows = feeds
            .iter()
            .enumerate()
            .map(|(i, f)| match f {
                Ok(f) => FeedStatus {
                    name: f.name().to_owned(),
                    state: "starting".into(),
                    last_ok: None,
                    last_error: None,
                    last_error_at: None,
                },
                Err(e) => FeedStatus {
                    name: names.get(i).copied().unwrap_or("").to_owned(),
                    state: e.clone(),
                    last_ok: None,
                    last_error: None,
                    last_error_at: None,
                },
            })
            .collect();
        let shared = Arc::new(Mutex::new(Shared {
            feeds: rows,
            server_error: None,
        }));

        let mut running = Vec::new();
        for (slot, feed) in feeds.into_iter().enumerate() {
            let Ok(feed) = feed else { continue };
            let (tx, rx) = channel();
            let handle = FeedWorker::spawn(
                feed,
                args.clone(),
                worker_cfg.clone(),
                cache.clone(),
                db.clone(),
                None,
                tx,
            );
            let to = shared.clone();
            let forwarder = std::thread::Builder::new()
                .name("feed-forward".into())
                .spawn(move || forward(rx, to, slot));
            let forwarder = match forwarder {
                Ok(t) => Some(t),
                Err(e) => {
                    if let Some(f) = lock(&shared).feeds.get_mut(slot) {
                        f.last_error = Some(format!("cannot start the status thread: {e}"));
                    }
                    None
                }
            };
            running.push(Running { handle, forwarder });
        }

        let bind = web.bind;
        let web = cfg.server_active.then(|| {
            let api = Arc::new(PlanesApi {
                cache: cache.clone(),
                lifetime_min: cfg.lifetime_min,
            });
            let errors = shared.clone();
            WebServer::start(
                web.with_router(router),
                api,
                Box::new(move |text| lock(&errors).server_error = Some(text)),
            )
        });

        Ok(Service {
            started: Utc::now(),
            lifetime_min: cfg.lifetime_min,
            cache,
            shared,
            feeds: running,
            web,
            bind,
            db_error,
        })
    }

    pub fn status(&self) -> Status {
        let s = lock(&self.shared);
        let server = match (&self.web, self.local_addr(), &s.server_error) {
            (None, _, _) => "off".to_owned(),
            (Some(_), Some(addr), _) => format!("listening on {addr}"),
            (Some(_), None, Some(e)) => e.clone(),
            (Some(_), None, None) => format!("starting on {}", self.bind),
        };
        Status {
            started: self.started,
            planes: self.cache.get_all(Utc::now(), self.lifetime_min).len(),
            feeds: s.feeds.clone(),
            server,
            db_error: self.db_error.clone(),
        }
    }

    /// The address the server listens on, once the bind succeeded.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.web.as_ref().and_then(WebServer::local_addr)
    }

    /// Stops the feeds and the server and waits for their threads, at most [`STOP_LIMIT`]:
    /// the server thread ends within its drain, a worker within a second unless a feed
    /// request is in flight, which then ends on its own.
    pub fn stop(mut self) {
        for f in &self.feeds {
            f.handle.stop();
        }
        if let Some(w) = &mut self.web {
            w.stop();
        }
        let web = self.web.take();
        let feeds = std::mem::take(&mut self.feeds);
        let join_all = move || {
            if let Some(w) = web {
                w.join();
            }
            for f in feeds {
                f.handle.join();
                if let Some(t) = f.forwarder {
                    let _ = t.join();
                }
            }
        };
        let (done_tx, done_rx) = channel();
        let joiner = std::thread::Builder::new()
            .name("planeserver-stop".into())
            .spawn(move || {
                join_all();
                let _ = done_tx.send(());
            });
        if joiner.is_ok() {
            let _ = done_rx.recv_timeout(STOP_LIMIT);
        }
    }
}
