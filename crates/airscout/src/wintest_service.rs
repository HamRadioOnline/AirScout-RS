//! The Win-Test UDP receiver (`MapDlg.cs:7005-7069`): receives datagrams, answers ASSETPATH on
//! its own thread and hands the other commands to the app as [`ServiceEvent::WinTest`].

use crate::background::sleep_cancellable;
use crate::net_addrs::NetAddrs;
use crate::services::{Paths, ServiceEvent};
use crate::wintest_nearest::{parse_set_path, send_reply, NearestSettings};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, gpoint_from_loc, PositionInRectangle};
use as_settings::Settings;
use as_stations::{callsign, GeoSource, Location, StationDb};
use as_wintest::{decode, Decoded, WtKind};
use chrono::Utc;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Wait between two bind attempts.
const RETRY: Duration = Duration::from_secs(5);
/// Read timeout of the socket: the cancel flag is seen within this time.
const READ_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, PartialEq)]
pub struct WinTestConfig {
    pub bind: SocketAddr,
    pub reply_to: SocketAddr,
    pub reply_bind_ip: Ipv4Addr,
    pub server_name: String,
    pub sync_with_kst: bool,
    /// `watchlist_max_count`: no message adds more entries (or creates more station rows).
    pub watchlist_max: usize,
}

impl WinTestConfig {
    pub fn from_settings(s: &Settings, net: NetAddrs) -> Self {
        let port = s.server.port;
        Self {
            bind: SocketAddr::from((net.bind_ip, port)),
            reply_to: SocketAddr::from((net.broadcast_ip, port)),
            reply_bind_ip: net.bind_ip,
            server_name: s.server.name.clone(),
            sync_with_kst: s.watchlist_sync_with_kst,
            watchlist_max: s.watchlist_max_count,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShowPath {
    /// `None` for a frequency that maps to no band (the band stays as it is).
    pub band: Option<Band>,
    pub my_call: String,
    pub my: GPoint,
    pub dx_call: String,
    pub dx: GPoint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WatchEntry {
    pub call: String,
    pub loc: String,
    pub pos: GPoint,
    pub checked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RemoveWatch {
    All,
    Calls(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum WinTestCommand {
    ShowPath(ShowPath),
    Watchlist(Vec<WatchEntry>),
    AddWatch(Vec<WatchEntry>),
    RemoveWatch(RemoveWatch),
}

/// What the receiver thread needs.
pub struct WinTestCtx {
    pub paths: Paths,
    pub cache: Arc<PlaneInfoCache>,
    pub live: Arc<RwLock<NearestSettings>>,
    pub tx: Sender<ServiceEvent>,
    pub generation: u64,
}

/// `MapDlg.cs:7032`: the shared Win-Test socket ([`as_wintest::socket::bind_shared`]) with the
/// receiver's read timeout.
pub fn bind_socket(addr: SocketAddr) -> io::Result<UdpSocket> {
    let sock = as_wintest::socket::bind_shared(addr)?;
    sock.set_read_timeout(Some(READ_TIMEOUT))?;
    Ok(sock)
}

/// `MapDlg.cs:7132-7155`: exact match on the frequency string in units of 100 Hz.
pub fn show_path_band(qrg: &str) -> Option<Band> {
    Some(match qrg {
        "500000" => Band::B50M,
        "700000" => Band::B70M,
        "1440000" => Band::B144M,
        "4320000" => Band::B432M,
        "12960000" => Band::B1_2G,
        "23200000" => Band::B2_3G,
        "34000000" => Band::B3_4G,
        "57600000" => Band::B5_7G,
        "103680000" => Band::B10G,
        "240480000" => Band::B24G,
        "470880000" => Band::B47G,
        "760320000" => Band::B76G,
        _ => return None,
    })
}

fn fields(data: &str) -> Vec<String> {
    data.split(',').map(|f| f.replace('"', "")).collect()
}

/// The first six characters of a locator, `None` if it is shorter.
fn loc6(l: &str) -> Option<String> {
    let c: Vec<char> = l.chars().take(6).collect();
    (c.len() == 6).then(|| c.into_iter().collect())
}

/// `MapDlg.cs:7110-7130`: the stored position if its locator matches, else the locator centre
/// (which is stored).
fn show_path_pos(stations: &StationDb, call: &str, loc: &str) -> Option<GPoint> {
    if let Ok(Some(l)) = stations.location_find(call, None) {
        let stored = maidenhead::loc_from_lat_lon(l.lat, l.lon, false, 3, true)
            .ok()
            .flatten();
        if stored.as_deref() == Some(loc) {
            return Some(GPoint::new(l.lat, l.lon));
        }
    }
    let c = gpoint_from_loc(loc, PositionInRectangle::MiddleMiddle).ok()?;
    // UpdateLocation(.., FROMLOC); a DB error is dropped (upstream logs it)
    if let Ok(Some(l)) = maidenhead::loc_from_lat_lon(c.lat, c.lon, false, 3, true) {
        let _ = stations.location_insert_or_update_if_newer(&Location::new(
            call,
            &l,
            c.lat,
            c.lon,
            GeoSource::FromLoc,
            0,
            Utc::now(),
        ));
    }
    Some(c)
}

/// `MapDlg.cs:7071-7187`. `None` if the message is malformed (validated before any DB access).
pub fn prepare_show_path(data: &str, stations: &StationDb) -> Option<ShowPath> {
    let a = fields(data);
    if a.len() < 5 {
        return None;
    }
    let my_loc = loc6(&a[2])?;
    let dx_loc = loc6(&a[4])?;
    if !callsign::check(&a[1])
        || !callsign::check(&a[3])
        || !maidenhead::check(&my_loc)
        || !maidenhead::check(&dx_loc)
    {
        return None;
    }
    let my = show_path_pos(stations, &a[1], &my_loc)?;
    let dx = show_path_pos(stations, &a[3], &dx_loc)?;
    Some(ShowPath {
        band: show_path_band(&a[0]),
        my_call: a[1].clone(),
        my,
        dx_call: a[3].clone(),
        dx,
    })
}

/// A watchlist entry, or `None` for an invalid call or locator or the own call.
fn watch_entry(
    stations: &StationDb,
    my_call: &str,
    call: &str,
    loc: &str,
    checked: bool,
) -> Option<WatchEntry> {
    if !callsign::check(call) || !maidenhead::check(loc) || call == callsign::cut(my_call) {
        return None;
    }
    let l = stations.location_find_or_create(call, loc).ok()??;
    Some(WatchEntry {
        call: call.to_string(),
        loc: loc.to_string(),
        pos: GPoint::new(l.lat, l.lon),
        checked,
    })
}

fn trim_upper(s: &str) -> String {
    s.trim().to_uppercase()
}

/// `MapDlg.cs:7330-7357`: call/locator pairs; an odd trailing field is ignored.
pub fn prepare_watchlist(
    data: &str,
    my_call: &str,
    stations: &StationDb,
    max: usize,
) -> Vec<WatchEntry> {
    let a: Vec<&str> = data.split(',').collect();
    let mut out = Vec::new();
    let mut i = 1;
    while i + 1 < a.len() && out.len() < max {
        let (call, loc) = (trim_upper(a[i]), trim_upper(a[i + 1]));
        out.extend(watch_entry(stations, my_call, &call, &loc, false));
        i += 2;
    }
    out
}

/// `MapDlg.cs:7395-7440`: `None` for a malformed message.
pub fn prepare_add_watch(
    data: &str,
    my_call: &str,
    stations: &StationDb,
    max: usize,
) -> Option<Vec<WatchEntry>> {
    if !data.contains(',') {
        return None;
    }
    let a: Vec<&str> = data.split(',').collect();
    if !(a.len() - 1).is_multiple_of(3) {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 1;
    while i + 2 < a.len() && out.len() < max {
        let call = trim_upper(a[i]);
        let mut loc = trim_upper(a[i + 1]);
        let checked = trim_upper(a[i + 2]) == "1";
        i += 3;
        if !callsign::check(&call) {
            continue;
        }
        if loc.is_empty() {
            if let Ok(Some(l)) = stations.location_find(&call, None) {
                loc = l.loc;
            }
        }
        out.extend(watch_entry(stations, my_call, &call, &loc, checked));
    }
    Some(out)
}

/// `MapDlg.cs:7500-7530`.
pub fn prepare_remove_watch(data: &str) -> RemoveWatch {
    if !data.contains(',') {
        return RemoveWatch::All;
    }
    RemoveWatch::Calls(data.split(',').skip(1).map(trim_upper).collect())
}

/// Handles one decoded datagram. Only messages for this server's name are looked at; the source
/// and the checksum are not checked (1:1). Errors are dropped (upstream logs them).
pub fn dispatch(d: &Decoded, cfg: &WinTestConfig, ctx: &WinTestCtx, cancel: &AtomicBool) {
    if d.msg.dst != cfg.server_name {
        return;
    }
    let data = &d.msg.data;
    let stations = &ctx.paths.stations;
    let cmd = match d.msg.kind {
        WtKind::AsSetPath => {
            answer_set_path(d, cfg, ctx, cancel);
            return;
        }
        WtKind::AsShowPath => prepare_show_path(data, stations).map(WinTestCommand::ShowPath),
        WtKind::AsWatchlist if cfg.sync_with_kst => {
            let my = ctx.paths.my_station().call;
            Some(WinTestCommand::Watchlist(prepare_watchlist(
                data,
                &my,
                stations,
                cfg.watchlist_max,
            )))
        }
        WtKind::AsAddWatch if !cfg.sync_with_kst => {
            let my = ctx.paths.my_station().call;
            prepare_add_watch(data, &my, stations, cfg.watchlist_max).map(WinTestCommand::AddWatch)
        }
        WtKind::AsRemoveWatch if !cfg.sync_with_kst => {
            Some(WinTestCommand::RemoveWatch(prepare_remove_watch(data)))
        }
        _ => None,
    };
    if let Some(cmd) = cmd {
        let _ = ctx.tx.send(ServiceEvent::WinTest {
            generation: ctx.generation,
            cmd,
        });
    }
}

/// `MapDlg.cs:7189-7310`, on the receiver thread: a slow answer blocks only this thread (1:1).
fn answer_set_path(d: &Decoded, cfg: &WinTestConfig, ctx: &WinTestCtx, cancel: &AtomicBool) {
    let Some(req) = parse_set_path(&d.msg.src, &d.msg.data) else {
        return;
    };
    let n = ctx.live.read().unwrap_or_else(|e| e.into_inner()).clone();
    if let Ok(Some(data)) =
        crate::wintest_nearest::answer(&ctx.paths, &ctx.cache, &n, &req, Utc::now(), cancel)
    {
        // stopped while the answer was being built: stay silent
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let _ = send_reply(
            &cfg.server_name,
            &req,
            data,
            cfg.reply_bind_ip,
            cfg.reply_to,
        );
    }
}

/// Thread `wintest`.
pub struct WinTestService {
    cancel: Arc<AtomicBool>,
    addr: Arc<Mutex<Option<SocketAddr>>>,
}

impl WinTestService {
    /// Spawns the thread; the bind happens on it (the caller may be the UI thread), so
    /// [`Self::local_addr`] is `None` until it succeeded. A failed bind is reported once and
    /// retried every 5 s.
    pub fn start(cfg: WinTestConfig, ctx: WinTestCtx) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let addr = Arc::new(Mutex::new(None));
        let (flag, shared) = (cancel.clone(), addr.clone());
        let spawned = std::thread::Builder::new()
            .name("wintest".into())
            .spawn(move || run(&cfg, &ctx, &flag, &shared));
        if spawned.is_err() {
            cancel.store(true, Ordering::SeqCst);
        }
        Self { cancel, addr }
    }

    /// The bound address, once the bind has succeeded.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *self.addr.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Cancels and returns at once; the thread closes the socket within 250 ms.
    pub fn stop(self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Drop for WinTestService {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

fn run(
    cfg: &WinTestConfig,
    ctx: &WinTestCtx,
    cancel: &AtomicBool,
    shared: &Mutex<Option<SocketAddr>>,
) {
    // A stopped receiver says nothing: its late error or status would reach the app after a
    // restart and could not be told from the new receiver's.
    let event = |e: ServiceEvent| {
        if !cancel.load(Ordering::SeqCst) {
            let _ = ctx.tx.send(e);
        }
    };
    let mut failed = false;
    let sock = loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        match bind_socket(cfg.bind) {
            Ok(s) => break s,
            Err(e) => {
                if !failed {
                    failed = true;
                    event(ServiceEvent::Error(format!(
                        "Win-Test UDP server: cannot listen on {}: {e}",
                        cfg.bind
                    )));
                }
                if sleep_cancellable(RETRY, cancel) {
                    return;
                }
            }
        }
    };
    let local = sock.local_addr().unwrap_or(cfg.bind);
    *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(local);
    if failed {
        event(ServiceEvent::Status(format!(
            "Win-Test UDP server listening on {local}"
        )));
    }
    let mut buf = vec![0u8; 65_535];
    while !cancel.load(Ordering::SeqCst) {
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => {
                if let Ok(d) = decode(&buf[..n]) {
                    dispatch(&d, cfg, ctx, cancel);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            // e.g. Windows WSAECONNRESET after an ICMP port unreachable
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    *shared.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
