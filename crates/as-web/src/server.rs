//! The HTTP server on its own `web` thread (`Webserver.cs:74-193`, response headers
//! `:1232-1235`).
//!
//! C# runs an `HttpListener` on `http://+:<port>/` and answers each request on a new
//! background worker. Here hyper serves an axum router on a current-thread tokio runtime.
//! Asset and 404 answers are table lookups and run on that thread. API answers run on the
//! blocking pool, at most [`MAX_WORKERS`] at a time; further API requests wait for a free
//! worker without queueing work.
//! Against a hostile LAN peer (new design; `HttpListener` has its own limits): at most
//! [`MAX_CONNECTIONS`] connections are served, further ones wait in the listen backlog
//! and hold no file descriptor of the process; a request head must arrive within
//! [`HEADER_TIMEOUT`], which also closes an idle keep-alive connection; and a stop drops
//! every connection after [`DRAIN`], so the `web` thread always ends.
//! The request method is ignored, as in C#. A bind error is reported and retried after
//! [`RETRY`]; C# also runs `netsh` with `runas` there (`:158-170`), which is not
//! ported (no command is executed).

use crate::query::Query;
use crate::route::{route, ApiRoute, Route};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::Router as AxumRouter;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;

/// The wait before a failed bind is tried again (`Webserver.cs:174`, `:180`).
pub const RETRY: Duration = Duration::from_secs(10);

/// The most API requests answered at the same time (the blocking pool size).
pub const MAX_WORKERS: usize = 4;

/// How long a connection may take to send a request head, and how long an idle
/// keep-alive connection stays open (hyper restarts the head timer after each answer).
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// The most connections served at the same time.
pub const MAX_CONNECTIONS: usize = 64;

/// How long [`WebServer::stop`] lets open connections finish before it drops them.
pub const DRAIN: Duration = Duration::from_secs(1);

/// The pause after a failed accept (for example out of file descriptors).
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// The shortest wait between bind attempts, whatever `WebConfig::retry` says.
const MIN_RETRY: Duration = Duration::from_millis(100);

/// The answer of a 503 to an API request still waiting for a worker at stop. C# aborts
/// such a request when its listener stops (new design).
const STOPPED: &[u8] = b"Error: web server stopped";

/// The answer of a 500 when an API handler panics.
const INTERNAL_ERROR: &[u8] = b"Error: internal error";

/// The content routes, answered by the application.
pub trait Api: Send + Sync + 'static {
    /// The UTF-8 text of `route` for the raw request target `raw_url`.
    fn answer(&self, route: ApiRoute, raw_url: &str, query: &Query) -> String;
}

/// Where the server listens, how long it waits between bind attempts, and its connection
/// limits. A `retry` below 100 ms is raised to 100 ms, so a failing bind never spins. A
/// `max_connections` of 0 counts as 1.
#[derive(Clone, Debug)]
pub struct WebConfig {
    pub bind: SocketAddr,
    pub retry: Duration,
    /// [`HEADER_TIMEOUT`]: the request head and keep-alive idle limit.
    pub header_timeout: Duration,
    /// [`MAX_CONNECTIONS`]: further connections wait in the listen backlog.
    pub max_connections: usize,
    /// [`DRAIN`]: the stop drain.
    pub drain: Duration,
    /// Maps a raw request target to its answer; [`route`] by default.
    pub router: Router,
}

/// Maps a raw request target to a [`Route`].
pub type Router = fn(&str) -> Route;

impl PartialEq for WebConfig {
    fn eq(&self, other: &Self) -> bool {
        self.bind == other.bind
            && self.retry == other.retry
            && self.header_timeout == other.header_timeout
            && self.max_connections == other.max_connections
            && self.drain == other.drain
            && std::ptr::fn_addr_eq(self.router, other.router)
    }
}

impl Eq for WebConfig {}

impl WebConfig {
    pub fn new(bind: SocketAddr) -> Self {
        WebConfig {
            bind,
            retry: RETRY,
            header_timeout: HEADER_TIMEOUT,
            max_connections: MAX_CONNECTIONS,
            drain: DRAIN,
            router: route,
        }
    }

    /// The same config with `router` answering the request targets.
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = router;
        self
    }
}

/// Status, `Content-Type` and body of a routed request (`Webserver.cs:1113-1228`).
pub fn respond(route: Route, raw_url: &str, api: &dyn Api) -> (u16, &'static str, Vec<u8>) {
    match route {
        Route::Asset(a) => (200, a.mime, a.bytes.to_vec()),
        Route::Api(r) => (
            200,
            "text/json",
            api.answer(r, raw_url, &Query::from_raw_url(raw_url))
                .into_bytes(),
        ),
        Route::NotFound => (404, "text/html", b"Not found!".to_vec()),
    }
}

/// The running server. Dropping it stops it.
pub struct WebServer {
    shutdown: watch::Sender<bool>,
    local_addr: Arc<OnceLock<SocketAddr>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

type OnError = Arc<dyn Fn(String) + Send + Sync>;

impl WebServer {
    /// Spawns the `web` thread, which binds `cfg.bind` (retrying) and serves until
    /// [`stop`](Self::stop). Errors go to `on_error`, each distinct text once.
    pub fn start(
        cfg: WebConfig,
        api: Arc<dyn Api>,
        on_error: Box<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        Self::start_with(cfg, Box::new(move || api), on_error)
    }

    /// Like [`start`](Self::start), but `make_api` builds the application on the `web`
    /// thread before the first bind attempt, so its setup (opening a database) never runs
    /// on the caller's thread.
    pub fn start_with(
        cfg: WebConfig,
        make_api: Box<dyn FnOnce() -> Arc<dyn Api> + Send>,
        on_error: Box<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        let (shutdown, rx) = watch::channel(false);
        let local_addr = Arc::new(OnceLock::new());
        let on_error: OnError = Arc::from(on_error);
        let addr_slot = local_addr.clone();
        let report = on_error.clone();
        let spawned = std::thread::Builder::new()
            .name("web".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .max_blocking_threads(MAX_WORKERS)
                    .build();
                match rt {
                    Ok(rt) => {
                        let api = make_api();
                        rt.block_on(run(cfg, api, report, rx, addr_slot));
                        // A handler still running finishes on its own; its answer is
                        // dropped. The thread ends now.
                        rt.shutdown_background();
                    }
                    Err(e) => report(format!("Web server: cannot start: {e}")),
                }
            });
        let thread = match spawned {
            Ok(t) => Some(t),
            Err(e) => {
                on_error(format!("Web server: cannot start: {e}"));
                None
            }
        };
        WebServer {
            shutdown,
            local_addr,
            thread,
        }
    }

    /// True once the `web` thread has ended (or never started).
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    /// The bound address, once the bind succeeded.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr.get().copied()
    }

    /// Stops the server and waits until the `web` thread has ended, so the port is free.
    pub fn join(mut self) {
        self.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    /// Signals the shutdown and returns at once. The listener closes at once and idle
    /// connections close. A running request is still answered within [`DRAIN`]; an API
    /// request waiting for a worker answers 503 and never runs.
    pub fn stop(&mut self) {
        self.shutdown.send_replace(true);
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Resolves once a stop was signalled or the [`WebServer`] is gone.
async fn stopped(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|stop| *stop).await;
}

/// The bind loop, then the server until the stop signal.
async fn run(
    cfg: WebConfig,
    api: Arc<dyn Api>,
    on_error: OnError,
    rx: watch::Receiver<bool>,
    local_addr: Arc<OnceLock<SocketAddr>>,
) {
    let mut reported = BTreeSet::new();
    let listener = loop {
        if *rx.borrow() {
            return;
        }
        match TcpListener::bind(cfg.bind).await {
            Ok(l) => break l,
            Err(e) => {
                let text = format!("Web server: cannot listen on {}: {e}", cfg.bind);
                if reported.insert(text.clone()) {
                    on_error(text);
                }
                tokio::select! {
                    () = tokio::time::sleep(cfg.retry.max(MIN_RETRY)) => {}
                    () = stopped(rx.clone()) => return,
                }
            }
        }
    };
    if let Ok(addr) = listener.local_addr() {
        let _ = local_addr.set(addr);
    }
    let workers = Arc::new(Semaphore::new(MAX_WORKERS));
    let state = AppState {
        api,
        router: cfg.router,
        workers: workers.clone(),
    };
    let app = AxumRouter::new().fallback(handle).with_state(state);
    let mut http1 = http1::Builder::new();
    http1
        .timer(TokioTimer::new())
        .header_read_timeout(cfg.header_timeout);
    let conns = Arc::new(Semaphore::new(cfg.max_connections.max(1)));
    let mut tasks = JoinSet::new();
    loop {
        while tasks.try_join_next().is_some() {}
        // Over the cap, a new connection waits in the listen backlog.
        let permit = tokio::select! {
            p = conns.clone().acquire_owned() => p,
            () = stopped(rx.clone()) => break,
        };
        let Ok(permit) = permit else { break };
        let accepted = tokio::select! {
            a = listener.accept() => a,
            () = stopped(rx.clone()) => break,
        };
        let stream = match accepted {
            Ok((stream, _)) => stream,
            // For example out of file descriptors: wait instead of spinning.
            Err(_) => {
                tokio::select! {
                    () = tokio::time::sleep(ACCEPT_PAUSE) => continue,
                    () = stopped(rx.clone()) => break,
                }
            }
        };
        tasks.spawn(serve_connection(
            http1.clone(),
            stream,
            app.clone(),
            permit,
            rx.clone(),
        ));
    }
    // The port is free at once. A queued API request answers without a worker, open
    // connections get the drain, then the rest is dropped.
    drop(listener);
    workers.close();
    let _ = tokio::time::timeout(cfg.drain, async {
        while tasks.join_next().await.is_some() {}
    })
    .await;
    tasks.abort_all();
}

/// Serves one connection until it closes. After the stop signal, the connection ends once
/// the request it reads or answers is done (an idle connection at once).
async fn serve_connection(
    http1: http1::Builder,
    stream: TcpStream,
    app: AxumRouter,
    permit: OwnedSemaphorePermit,
    rx: watch::Receiver<bool>,
) {
    let _permit = permit;
    let conn = http1.serve_connection(TokioIo::new(stream), TowerToHyperService::new(app));
    let mut conn = std::pin::pin!(conn);
    tokio::select! {
        _ = conn.as_mut() => {}
        () = stopped(rx) => {
            conn.as_mut().graceful_shutdown();
            let _ = conn.await;
        }
    }
}

/// The handler state: the application and one permit per blocking worker.
#[derive(Clone)]
struct AppState {
    api: Arc<dyn Api>,
    router: Router,
    workers: Arc<Semaphore>,
}

/// Answers `route` on a blocking worker. The permit is taken before `spawn_blocking`, so
/// a waiting request holds no queued closure: when its client goes away, hyper drops this
/// future and the request never runs. Waiting requests are not refused, as in C#.
async fn answer_api(state: AppState, routed: Route, raw: String) -> (u16, &'static str, Vec<u8>) {
    let internal_error = || (500, "text/json", INTERNAL_ERROR.to_vec());
    // The pool closes on stop: a waiting request answers at once and never runs.
    let Ok(permit) = state.workers.acquire_owned().await else {
        return (503, "text/json", STOPPED.to_vec());
    };
    let api = state.api;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        respond(routed, &raw, &*api)
    })
    .await
    .unwrap_or_else(|_| internal_error())
}

/// Answers any method like GET. The route comes from the raw request target: hyper only
/// accepts targets of visible ASCII, so a target with other bytes (not UTF-8) is answered
/// 400 by hyper and never reaches this handler.
async fn handle(State(state): State<AppState>, req: Request) -> Response {
    let raw = req
        .uri()
        .path_and_query()
        .map_or("", |pq| pq.as_str())
        .to_owned();
    let routed = (state.router)(&raw);
    let (status, mime, body) = match routed {
        Route::Api(_) => answer_api(state, routed, raw).await,
        _ => respond(routed, &raw, &*state.api),
    };
    // `Webserver.cs:1232-1235`.
    let len = body.len();
    let mut resp = Response::new(Body::from(body));
    *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = resp.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(mime));
    headers.insert(CONTENT_LENGTH, HeaderValue::from(len));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}
