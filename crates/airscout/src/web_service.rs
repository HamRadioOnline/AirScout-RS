//! The desktop web server lifecycle.

use crate::net_addrs::NetAddrs;
use crate::services::ServiceEvent;
use crate::web_api::{WebApi, WebCtx};
use as_settings::Settings;
use as_web::server::{Api, WebConfig, WebServer};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

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

/// Dropping the web service stops the server.
pub struct WebService {
    server: WebServer,
    /// The last error the server reported.
    last_error: Arc<Mutex<Option<String>>>,
    /// `WebCtx::stop`: cancels running path calculations.
    stop: Arc<AtomicBool>,
}

impl WebService {
    /// Starts the server on the `web` thread. The `web`
    /// thread opens the aircraft database before the bind when `ctx.aircraft` is `None`,
    /// so the caller does no database I/O. Errors go to `tx` until the service stops.
    pub fn start(cfg: WebConfig, ctx: WebCtx, tx: Sender<ServiceEvent>) -> Self {
        let stop = ctx.stop.clone();
        let report = error_sink(stop.clone(), tx);
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

    /// Stops the server and returns at once. A running request is still
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
