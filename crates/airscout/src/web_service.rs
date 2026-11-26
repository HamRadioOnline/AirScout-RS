//! The running web server and the `planes.json` writer (`MapDlg.cs:2402-2414`,
//! `:8463-8545`).
//!
//! C# runs the JSON writer whenever the background workers run: it starts with them
//! (`:2173-2174`), stops with them (`:2487`) and starts again after Options (`:3979`),
//! also while `Server_Activate` is off. Here the writer runs exactly while the web server
//! runs, its only reader. It writes atomically and escapes strings (port deviation, L19).

use crate::background::sleep_cancellable;
use crate::net_addrs::NetAddrs;
use crate::services::ServiceEvent;
use crate::web_api::{WebApi, WebCtx};
use as_aircraft::PlaneInfoCache;
use as_settings::Settings;
use as_web::server::{Api, WebConfig, WebServer};
use chrono::{DateTime, Utc};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// The wait between two writes (`MapDlg.cs:8466`).
pub const PLANES_JSON_PERIOD: Duration = Duration::from_secs(60);

/// The file name in the tmp directory (`MapDlg.cs:8479`).
pub const PLANES_JSON: &str = "planes.json";

pub use as_web::planes::{planes_json, PLANES_JSON_VERSION};

/// Distinguishes the temp files of writers that overlap during a restart.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Writes `<tmp_dir>/planes.json` from the planes of `cache` valid at `now`, through a temp
/// file and a rename, so a reader never sees a partial file. Without planes nothing is
/// written and the answer is `false` (`MapDlg.cs:8474`).
pub fn write_planes_json(
    cache: &PlaneInfoCache,
    tmp_dir: &Path,
    now: DateTime<Utc>,
    ttl_min: i64,
) -> io::Result<bool> {
    let planes = cache.get_all(now, ttl_min);
    if planes.is_empty() {
        return Ok(false);
    }
    std::fs::create_dir_all(tmp_dir)?;
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp = tmp_dir.join(format!("{PLANES_JSON}.{}-{seq}.tmp", std::process::id()));
    let written = std::fs::write(&temp, planes_json(&planes, PLANES_JSON_VERSION))
        .and_then(|()| std::fs::rename(&temp, tmp_dir.join(PLANES_JSON)));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written.map(|()| true)
}

/// The web server config the settings ask for: `None` unless `server.activate`
/// (`MapDlg.cs:2405-2414`); else `<bind ip>:<web.port>`.
pub fn web_config(s: &Settings, net: NetAddrs) -> Option<WebConfig> {
    s.server
        .activate
        .then(|| WebConfig::new((net.bind_ip, s.web.port).into()))
}

/// Whether the web server listens, for the Options HTTP group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebState {
    /// No server: `server.activate` is off.
    Off,
    /// Started, the first bind attempt has neither succeeded nor failed yet.
    Starting,
    /// Bound to this port.
    Listening(u16),
    /// Not bound; the last bind error. The server retries.
    NotListening(String),
}

/// Whether a server running `running` in `state` is kept when the settings ask for
/// `want`: the config is unchanged and no bind error was reported. A server that is still
/// starting is kept, so its own first bind never races a new server on the same port.
pub fn keeps_server(running: &WebConfig, state: &WebState, want: Option<&WebConfig>) -> bool {
    Some(running) == want && !matches!(state, WebState::NotListening(_))
}

/// The web server and the `planes.json` writer. Dropping it stops both.
pub struct WebService {
    server: WebServer,
    /// The last error the server reported.
    last_error: Arc<Mutex<Option<String>>>,
    /// `WebCtx::stop`: cancels the writer and the running path calculations.
    stop: Arc<AtomicBool>,
}

impl WebService {
    /// Starts the server (thread `web`) and the writer (thread `planes-json`). The `web`
    /// thread opens the aircraft database before the bind when `ctx.aircraft` is `None`,
    /// so the caller does no database I/O. Errors go to `tx` until the service stops.
    pub fn start(cfg: WebConfig, ctx: WebCtx, tx: Sender<ServiceEvent>) -> Self {
        let stop = ctx.stop.clone();
        let report = error_sink(stop.clone(), tx);
        spawn_writer(
            ctx.cache.clone(),
            ctx.live.clone(),
            ctx.tmp_dir.clone(),
            stop.clone(),
            report.clone(),
        );
        let api_report = report.clone();
        let make_api = move || {
            let api = WebApi::with_errors(ctx, Box::new(move |e| api_report(e)));
            // Opened before the bind (C20); a failure is reported and tried again on the
            // next `/airports.json`.
            let _ = api.aircraft();
            Arc::new(api) as Arc<dyn Api>
        };
        let last_error: Arc<Mutex<Option<String>>> = Arc::default();
        let slot = last_error.clone();
        let stopped = stop.clone();
        let on_error = move |e: String| {
            if stopped.load(Ordering::Relaxed) {
                return;
            }
            *slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.clone());
            report(e);
        };
        let server = WebServer::start_with(cfg, Box::new(make_api), Box::new(on_error));
        WebService {
            server,
            last_error,
            stop,
        }
    }

    /// Listening, or not with the last error; cheap (a short lock, no I/O).
    pub fn state(&self) -> WebState {
        if let Some(a) = self.local_addr() {
            return WebState::Listening(a.port());
        }
        match &*self.last_error.lock().unwrap_or_else(|p| p.into_inner()) {
            Some(t) => WebState::NotListening(t.clone()),
            None => WebState::Starting,
        }
    }

    /// The bound address, once the bind succeeded.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.server.local_addr()
    }

    /// Stops the server and the writer and returns at once. A running request is still
    /// answered; a running path calculation is cancelled.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.server.stop();
    }
}

impl Drop for WebService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Where the service threads report an error.
type Report = Arc<dyn Fn(String) + Send + Sync>;

/// Sends an error to `tx` as [`ServiceEvent::Error`], unless `stop` is set: a stopped
/// service reports nothing.
fn error_sink(stop: Arc<AtomicBool>, tx: Sender<ServiceEvent>) -> Report {
    // `Sender` is `Sync` (Rust 1.72), so one sink serves every thread.
    Arc::new(move |e| {
        if !stop.load(Ordering::Relaxed) {
            let _ = tx.send(ServiceEvent::Error(e));
        }
    })
}

/// The writer loop: at once, then every [`PLANES_JSON_PERIOD`] until `stop`. A failed write
/// is reported each time (at most once a period), so a lasting failure stays visible.
fn spawn_writer(
    cache: Arc<PlaneInfoCache>,
    live: Arc<RwLock<Settings>>,
    tmp_dir: PathBuf,
    stop: Arc<AtomicBool>,
    report: Report,
) {
    let err_report = report.clone();
    let spawned = std::thread::Builder::new()
        .name("planes-json".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let ttl = live
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .planes_position_ttl_min;
                if let Err(e) = write_planes_json(&cache, &tmp_dir, Utc::now(), ttl) {
                    report(format!("{PLANES_JSON}: {e}"));
                }
                if sleep_cancellable(PLANES_JSON_PERIOD, &stop) {
                    break;
                }
            }
        });
    if let Err(e) = spawned {
        err_report(format!("{PLANES_JSON}: {e}"));
    }
}
