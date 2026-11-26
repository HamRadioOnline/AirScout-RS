//! RTL1090 feed (`AirScout.PlaneFeeds.Plugin.RTL1090`): raw Mode-S frames from a local
//! receiver over TCP, Beast binary or AVR text, decoded by [`as_adsb::decoder::Decoder`].
//!
//! A receiver thread reads the socket (1 s read timeout, so a stop request is seen
//! quickly) and feeds the decoder, which sits behind a mutex: `get_planes` never reads
//! it while the thread writes (the C# plugin shares it unsynchronised). On error or EOF
//! the thread reconnects after the back-off (10 s). Frames are stamped with the receive
//! time; the Beast MLAT timestamp is ignored (logic-findings #41).

use super::{set_bool, set_str};
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use as_adsb::decoder::{AdsbInfo, Decoder};
use as_adsb::framing::{avr_line, BeastReader};
use chrono::Utc;
use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Wait before reconnecting (C#: 10 × 1 s).
const BACKOFF: Duration = Duration::from_secs(10);
/// `ReceiveBinaryMsg` throws a `TimeoutException` after 10 s without a complete frame.
const IDLE: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const STEP: Duration = Duration::from_millis(100);
const UNKNOWN: &str = "[unknown]";

/// Settings shared with the receiver thread.
#[derive(Debug, Clone)]
struct Conn {
    server: String,
    port: u16,
    binary: bool,
    use_geometric_alt_only: bool,
    backoff: Duration,
    idle: Duration,
}

/// The RTL1090 feed.
pub struct Rtl1090 {
    pub server: String,
    pub port: u16,
    /// Beast binary (true) or AVR text.
    pub binary: bool,
    pub use_geometric_alt_only: bool,
    /// Prefix calls with `@` to mark local reception.
    pub mark_local: bool,
    backoff: Duration,
    idle: Duration,
    decoder: Arc<Mutex<Decoder>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Rtl1090 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rtl1090")
            .field("server", &self.server)
            .field("port", &self.port)
            .field("binary", &self.binary)
            .field("running", &self.thread.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for Rtl1090 {
    fn default() -> Self {
        Self::with_backoff(BACKOFF)
    }
}

fn lock(d: &Mutex<Decoder>) -> MutexGuard<'_, Decoder> {
    // a panic while decoding leaves the decoder usable
    d.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sleep `d` in short steps; false if a stop was requested.
fn pause(d: Duration, stop: &AtomicBool) -> bool {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(STEP.min(end.saturating_duration_since(Instant::now())));
    }
    !stop.load(Ordering::SeqCst)
}

fn connect(c: &Conn) -> std::io::Result<TcpStream> {
    let mut last = std::io::Error::new(ErrorKind::NotFound, "no address");
    for addr in (c.server.as_str(), c.port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Read one connection until EOF, an error, `c.idle` without a complete frame (C#'s
/// `TimeoutException`, so a link that died without FIN is reconnected) or a stop request.
fn session(mut s: TcpStream, c: &Conn, dec: &Mutex<Decoder>, stop: &AtomicBool) {
    if s.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }
    let mut beast = BeastReader::default();
    let mut line = Vec::new();
    let mut buf = [0u8; 4096];
    let mut last_frame = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        if last_frame.elapsed() >= c.idle {
            return;
        }
        let n = match s.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        let mut frames = Vec::new();
        if c.binary {
            frames = beast.push(&buf[..n]);
        } else {
            for &b in &buf[..n] {
                if b == b'\n' {
                    if let Some(f) = avr_line(&String::from_utf8_lossy(&line)) {
                        frames.push(f);
                    }
                    line.clear();
                } else if line.len() < 1024 {
                    line.push(b);
                }
            }
        }
        if frames.is_empty() {
            continue;
        }
        last_frame = Instant::now();
        let now = Utc::now();
        let mut d = lock(dec);
        for f in frames {
            // malformed frames are dropped, as the C# loop ignores DecodeMessage errors
            let _ = d.decode_message(&f, now, c.use_geometric_alt_only);
        }
    }
}

fn receiver(c: Conn, dec: Arc<Mutex<Decoder>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        if let Ok(s) = connect(&c) {
            session(s, &c, &dec, &stop);
        }
        if !pause(c.backoff, &stop) {
            break;
        }
    }
}

impl Rtl1090 {
    /// A feed with a custom reconnect back-off (tests).
    #[doc(hidden)]
    pub fn with_backoff(backoff: Duration) -> Self {
        Self::with_timeouts(backoff, IDLE)
    }

    /// A feed with a custom back-off and no-frame timeout (tests).
    #[doc(hidden)]
    pub fn with_timeouts(backoff: Duration, idle: Duration) -> Self {
        Self {
            server: "localhost".into(),
            port: 31001,
            binary: true,
            use_geometric_alt_only: false,
            mark_local: false,
            backoff,
            idle,
            decoder: Arc::new(Mutex::new(Decoder::new(5))),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn plane(&self, i: AdsbInfo) -> FeedPlane {
        let alt = if self.use_geometric_alt_only {
            if i.geo_minus_baro_ft.is_nan() {
                f64::from(i32::MIN)
            } else {
                i.baro_alt_ft + i.geo_minus_baro_ft
            }
        } else {
            i.baro_alt_ft
        };
        FeedPlane {
            time: Some(i.timestamp),
            hex: i.icao24,
            call: if self.mark_local {
                format!("@{}", i.call)
            } else {
                i.call
            },
            lat: i.lat,
            lon: i.lon,
            alt_ft: alt,
            speed_kts: i.speed_kts,
            track: i.heading,
            reg: UNKNOWN.into(),
            type_code: UNKNOWN.into(),
            manufacturer: UNKNOWN.into(),
            model: UNKNOWN.into(),
            category: 0,
            ..FeedPlane::default()
        }
    }
}

impl Drop for Rtl1090 {
    fn drop(&mut self) {
        self.halt();
    }
}

impl PlaneFeed for Rtl1090 {
    fn name(&self) -> &'static str {
        "RTL1090"
    }

    fn info(&self) -> &'static str {
        "Local ADS-B receiver (RTL1090, dump1090, readsb …) over TCP, Beast binary or AVR \
         text format."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        None
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("server".into(), self.server.clone().into());
        t.insert("port".into(), i64::from(self.port).into());
        t.insert("binary".into(), self.binary.into());
        t.insert(
            "use_geometric_alt_only".into(),
            self.use_geometric_alt_only.into(),
        );
        t.insert("mark_local".into(), self.mark_local.into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "server", &mut self.server)?;
        if let Some(v) = t.get("port") {
            self.port = v
                .as_integer()
                .and_then(|p| u16::try_from(p).ok())
                .filter(|p| *p > 0)
                .ok_or_else(|| FeedError::Config("port must be 1..65535".into()))?;
        }
        set_bool(t, "binary", &mut self.binary)?;
        set_bool(
            t,
            "use_geometric_alt_only",
            &mut self.use_geometric_alt_only,
        )?;
        set_bool(t, "mark_local", &mut self.mark_local)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        self.halt();
        self.stop = Arc::new(AtomicBool::new(false));
        let c = Conn {
            server: self.server.clone(),
            port: self.port,
            binary: self.binary,
            use_geometric_alt_only: self.use_geometric_alt_only,
            backoff: self.backoff,
            idle: self.idle,
        };
        let (dec, stop) = (self.decoder.clone(), self.stop.clone());
        self.thread = Some(
            std::thread::Builder::new()
                .name("rtl1090-receiver".into())
                .spawn(move || receiver(c, dec, stop))?,
        );
        Ok(())
    }

    fn get_planes(&mut self, _args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let infos = lock(&self.decoder).planes();
        Ok(infos
            .into_iter()
            .map(|i| self.plane(i))
            .filter(|p| p.alt_ft > 0.0)
            .collect())
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        self.halt();
        Ok(())
    }
}
