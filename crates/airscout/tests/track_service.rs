//! Track fan-out (error counters, reconnect) and the Track worker thread.

use airscout::net_addrs::NetAddrs;
use airscout::services::ServiceEvent;
use airscout::track_outputs::{OutputError, OutputGroup, TrackConfig, TrackOutput, NEWLINE};
use airscout::track_service::{
    TrackCtx, TrackEvent, TrackFanout, TrackService, MAX_ERRORS, TRACK_STEP,
};
use airscout::tracker::{TrackInput, TrackInputCell, TrackParams};
use airscout::tracking::{RotStatus, TrackStatus, TrackStep, TrackValues};
use as_aircraft::enums::TrackMode;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_cat::{Doppler, DopplerCell};
use as_settings::{DopplerStrategy, Settings, TrackFile, TrackUdp};
use chrono::Utc;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
enum Script {
    Ok,
    NotConnected,
    Failed,
}

struct Fake {
    group: OutputGroup,
    script: Script,
    connected: bool,
    connects: Arc<AtomicU32>,
    sends: Arc<AtomicU32>,
}

impl Fake {
    fn make(
        group: OutputGroup,
        script: Script,
    ) -> (Box<dyn TrackOutput>, Arc<AtomicU32>, Arc<AtomicU32>) {
        let connects = Arc::new(AtomicU32::new(0));
        let sends = Arc::new(AtomicU32::new(0));
        let f = Fake {
            group,
            script,
            connected: false,
            connects: connects.clone(),
            sends: sends.clone(),
        };
        (Box::new(f), connects, sends)
    }
}

impl TrackOutput for Fake {
    fn group(&self) -> OutputGroup {
        self.group
    }
    fn connect(&mut self) -> Result<(), String> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        self.connected = true;
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn send(&mut self, _az: f64, _el: f64) -> Result<(), OutputError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        match self.script {
            Script::Ok => Ok(()),
            Script::NotConnected => Err(OutputError::NotConnected("[UDP]: nope".into())),
            Script::Failed => Err(OutputError::Failed("boom".into())),
        }
    }
    fn close(&mut self) {
        self.connected = false;
    }
}

fn values(az: f64, el: f64) -> TrackValues {
    let mut v = TrackValues::new(Utc::now());
    v.my_azimuth = az;
    v.my_elevation = el;
    v
}

fn some(mode: TrackMode) -> TrackStep {
    TrackStep {
        values: Some(values(123.0, 5.0)),
        mode,
    }
}

fn none() -> TrackStep {
    TrackStep {
        values: None,
        mode: TrackMode::Track,
    }
}

#[test]
fn fanout_counts_only_not_connected_and_disables_after_11() {
    let (o, _c, sends) = Fake::make(OutputGroup::Udp, Script::NotConnected);
    let mut f = TrackFanout::new(vec![o]);
    for i in 1..=MAX_ERRORS {
        let ev = f.step(&some(TrackMode::Track));
        assert!(
            !ev.iter().any(|e| matches!(e, TrackEvent::Disabled(_))),
            "step {i}"
        );
    }
    let ev = f.step(&some(TrackMode::Track));
    assert!(ev.contains(&TrackEvent::Text("Tracking via UDP disabled.".into())));
    assert!(ev.contains(&TrackEvent::Rot(RotStatus::Error)));
    assert!(ev.contains(&TrackEvent::Disabled(OutputGroup::Udp)));
    assert_eq!(sends.load(Ordering::SeqCst), 11);
    f.step(&some(TrackMode::Track));
    assert_eq!(sends.load(Ordering::SeqCst), 11);
}

#[test]
fn failed_send_is_not_counted_and_reconnects() {
    let (o, connects, _s) = Fake::make(OutputGroup::Udp, Script::Failed);
    let mut f = TrackFanout::new(vec![o]);
    for _ in 0..20 {
        let ev = f.step(&some(TrackMode::Track));
        assert!(!ev.iter().any(|e| matches!(e, TrackEvent::Disabled(_))));
    }
    assert_eq!(connects.load(Ordering::SeqCst), 20);
}

#[test]
fn one_failing_output_does_not_stop_the_next() {
    let (u, _c, _s) = Fake::make(OutputGroup::Udp, Script::Failed);
    let (file, _c2, file_sends) = Fake::make(OutputGroup::File, Script::Ok);
    let mut f = TrackFanout::new(vec![u, file]);
    for _ in 0..3 {
        f.step(&some(TrackMode::Track));
    }
    assert_eq!(file_sends.load(Ordering::SeqCst), 3);
}

#[test]
fn invalid_values_report_rot_stopped_once() {
    let (o, _c, _s) = Fake::make(OutputGroup::File, Script::Ok);
    let mut f = TrackFanout::new(vec![o]);
    f.step(&some(TrackMode::Track));
    let a = f.step(&none());
    let b = f.step(&none());
    assert_eq!(a, [TrackEvent::Rot(RotStatus::Stopped)]);
    assert!(b.is_empty());
}

#[test]
fn single_mode_ends_with_rot_stopped() {
    let (o, _c, _s) = Fake::make(OutputGroup::File, Script::Ok);
    let mut f = TrackFanout::new(vec![o]);
    assert_eq!(
        f.step(&some(TrackMode::Single)),
        [
            TrackEvent::Rot(RotStatus::Tracking),
            TrackEvent::Rot(RotStatus::Stopped)
        ]
    );
}

#[test]
fn no_output_enabled_stays_stopped() {
    let mut f = TrackFanout::new(vec![]);
    assert!(f.step(&some(TrackMode::Track)).is_empty());
}

fn cfg(dir: &std::path::Path, file: TrackFile) -> TrackConfig {
    let mut s = Settings::default();
    s.track.udp = TrackUdp::None;
    s.track.file = file;
    TrackConfig::from_settings(&s, dir, NetAddrs::loopback())
}

/// UDP AirScout output to `port` on 127.0.0.1, no file.
fn udp_cfg(dir: &std::path::Path, port: u16) -> TrackConfig {
    let mut s = Settings::default();
    s.track.udp = TrackUdp::AirScout;
    s.track.udp_airscout_port = port;
    s.track.file = TrackFile::None;
    TrackConfig::from_settings(&s, dir, NetAddrs::loopback())
}

const DIAL: i64 = 144_300_000;

fn settings() -> Settings {
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        dx_lat: 51.5,
        dx_lon: 3.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    s.track.activate = true;
    s.track.set_az = 123.0;
    s.track.set_el = 5.0;
    s.doppler.strategy = DopplerStrategy::A;
    s.doppler.dial_freq_hz = DIAL;
    s
}

/// A cache with plane `A1B2C3` reported now, between My and DX.
fn cache() -> Arc<PlaneInfoCache> {
    let c = PlaneInfoCache::new();
    c.set(PlaneInfo::new(
        Utc::now(),
        "CA1B2C3",
        "",
        "A1B2C3",
        51.3,
        1.6,
        135.0,
        32000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    ));
    Arc::new(c)
}

fn input(mode: TrackMode) -> TrackInput {
    TrackInput {
        rev: 1,
        life: true,
        mode,
        hex: Some("A1B2C3".into()),
        heights: Some((0.0, 0.0)),
        params: Some(TrackParams::from_settings(&settings())),
    }
}

struct Worker {
    svc: TrackService,
    input: TrackInputCell,
    doppler: DopplerCell,
    rx: Receiver<ServiceEvent>,
}

fn worker(c: TrackConfig, i: TrackInput, generation: u64) -> Worker {
    let (tx, rx) = channel();
    let input: TrackInputCell = Arc::new(Mutex::new(i));
    let doppler = DopplerCell::default();
    let svc = TrackService::start(
        c,
        TrackCtx {
            input: input.clone(),
            cache: cache(),
            doppler: doppler.clone(),
            tx,
            generation,
        },
    );
    Worker {
        svc,
        input,
        doppler,
        rx,
    }
}

fn udp_rx() -> (UdpSocket, u16) {
    let rx = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = rx.local_addr().unwrap().port();
    (rx, port)
}

/// SETAZIMUTH datagrams received within `max`, stopping early at `want`.
fn count_az(rx: &UdpSocket, max: Duration, want: usize) -> usize {
    let end = Instant::now() + max;
    let mut n = 0;
    let mut buf = [0u8; 2048];
    while n < want {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        rx.set_read_timeout(Some(left)).unwrap();
        match rx.recv(&mut buf) {
            Ok(len) if buf[..len].starts_with(b"SETAZIMUTH") => n += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    n
}

/// Waits up to 10 s for `f`, returning early; true once it held.
fn wait_until(mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f()
}

fn doppler(w: &Worker) -> as_cat::DopplerTarget {
    *w.doppler.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_rot_stopped(rx: &Receiver<ServiceEvent>) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(ServiceEvent::Track {
                event: TrackEvent::Rot(RotStatus::Stopped),
                ..
            }) => return,
            Ok(_) => {}
            Err(_) => panic!("no Rot(Stopped) within 10 s"),
        }
    }
}

fn wait_file(path: &std::path::Path, want: &str) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if std::fs::read_to_string(path).is_ok_and(|s| s == want) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("{} never held {want:?}", path.display());
}

#[test]
fn track_step_is_one_second() {
    assert_eq!(TRACK_STEP, Duration::from_secs(1));
}

#[test]
fn service_reports_rot_stopped_and_writes_file() {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path(), TrackFile::Native);
    let path = c.azel_path.clone();
    // SINGLE points at `Track_SetAz`/`Track_SetEl` (123, 5)
    let mut w = worker(c, input(TrackMode::Single), 7);
    wait_rot_stopped(&w.rx);
    wait_file(&path, &format!("123.0,5.0{NEWLINE}"));
    w.svc.stop();
}

#[test]
fn service_stop_ends_output() {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path(), TrackFile::Native);
    let path = c.azel_path.clone();
    let mut w = worker(c, input(TrackMode::Single), 1);
    wait_rot_stopped(&w.rx);
    wait_file(&path, &format!("123.0,5.0{NEWLINE}"));
    w.svc.stop();
    w.svc.stop(); // idempotent
    std::thread::sleep(Duration::from_millis(500));
    std::fs::remove_file(&path).unwrap();
    // a running worker would write again within one step
    std::thread::sleep(TRACK_STEP + Duration::from_millis(500));
    assert!(!path.exists());
}

#[test]
fn worker_steps_without_ui() {
    let dir = tempfile::tempdir().unwrap();
    let (rx, port) = udp_rx();
    let mut w = worker(udp_cfg(dir.path(), port), input(TrackMode::Track), 1);
    assert!(count_az(&rx, Duration::from_secs(10), 3) >= 3);
    let d = doppler(&w);
    assert_eq!(d.strategy, Doppler::A);
    assert_eq!(d.tx_hz, DIAL);
    assert_eq!(w.svc.shown().status, TrackStatus::Tracking);
    assert_eq!(w.svc.shown().rev, 1);
    w.svc.stop();
}

#[test]
fn steps_once_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let (rx, port) = udp_rx();
    let mut w = worker(udp_cfg(dir.path(), port), input(TrackMode::Track), 1);
    // the first step is immediate: steps at 0, 1, 2 and 3 s
    let n = count_az(&rx, Duration::from_millis(3500), usize::MAX);
    assert!((3..=5).contains(&n), "{n} steps in 3.5 s");
    w.svc.stop();
}

#[test]
fn steady_tracking_sends_no_event_per_step() {
    let dir = tempfile::tempdir().unwrap();
    let (rx, port) = udp_rx();
    let mut w = worker(udp_cfg(dir.path(), port), input(TrackMode::Track), 1);
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match w.rx.recv_timeout(left) {
            Ok(ServiceEvent::Track {
                event: TrackEvent::Rot(RotStatus::Tracking),
                ..
            }) => break,
            Ok(_) => {}
            Err(_) => panic!("no Rot(Tracking) within 10 s"),
        }
    }
    // the first step and 4 more
    assert_eq!(count_az(&rx, Duration::from_secs(10), 5), 5);
    std::thread::sleep(Duration::from_millis(200));
    let track: Vec<_> =
        w.rx.try_iter()
            .filter(|e| matches!(e, ServiceEvent::Track { .. }))
            .collect();
    assert!(track.is_empty(), "{track:?}");
    w.svc.stop();
}

#[test]
fn input_change_seen_next_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_rx, port) = udp_rx();
    let mut w = worker(udp_cfg(dir.path(), port), input(TrackMode::Track), 1);
    assert!(wait_until(|| w.svc.shown().status == TrackStatus::Tracking));
    assert_eq!(doppler(&w).strategy, Doppler::A);
    {
        let mut i = w.input.lock().unwrap();
        i.mode = TrackMode::None;
        i.rev = 2;
    }
    let end = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < end && w.svc.shown().status != TrackStatus::Stopped {
        std::thread::sleep(Duration::from_millis(20));
    }
    let shown = w.svc.shown();
    assert_eq!(shown.status, TrackStatus::Stopped);
    assert_eq!(shown.rev, 2);
    assert_eq!(doppler(&w).strategy, Doppler::None);
    w.svc.stop();
}

#[test]
fn history_input_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (rx, port) = udp_rx();
    let mut i = input(TrackMode::Track);
    i.life = false;
    let mut w = worker(udp_cfg(dir.path(), port), i, 1);
    assert_eq!(count_az(&rx, Duration::from_millis(2500), 1), 0);
    w.svc.stop();
}

/// Counts `idle` and `send` calls; always connected.
struct Idler {
    idles: Arc<AtomicU32>,
    sends: Arc<AtomicU32>,
}

impl Idler {
    fn make() -> (Box<dyn TrackOutput>, Arc<AtomicU32>, Arc<AtomicU32>) {
        let idles = Arc::new(AtomicU32::new(0));
        let sends = Arc::new(AtomicU32::new(0));
        let o = Idler {
            idles: idles.clone(),
            sends: sends.clone(),
        };
        (Box::new(o), idles, sends)
    }
}

impl TrackOutput for Idler {
    fn group(&self) -> OutputGroup {
        OutputGroup::Dde
    }
    fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn send(&mut self, _az: f64, _el: f64) -> Result<(), OutputError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn close(&mut self) {}
    fn idle(&mut self) {
        self.idles.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn step_without_values_idles_the_outputs() {
    let (o, idles, sends) = Idler::make();
    let mut fan = TrackFanout::new(vec![o]);
    fan.step(&none());
    fan.step(&none());
    assert_eq!(idles.load(Ordering::SeqCst), 2);
    assert_eq!(sends.load(Ordering::SeqCst), 0);
    fan.idle();
    assert_eq!(idles.load(Ordering::SeqCst), 3);
}

fn idle_worker(i: TrackInput) -> (TrackService, Arc<AtomicU32>, Arc<AtomicU32>) {
    let (o, idles, sends) = Idler::make();
    let (tx, _rx) = channel();
    let svc = TrackService::start_with(
        move || vec![o],
        TrackCtx {
            input: Arc::new(Mutex::new(i)),
            cache: cache(),
            doppler: DopplerCell::default(),
            tx,
            generation: 1,
        },
    );
    (svc, idles, sends)
}

/// Outside LIFE the tracker gives no step at all; the worker still idles its outputs at least
/// every 250 ms (here: 8 times within 2 s at most, waiting at most 10 s).
#[test]
fn worker_without_steps_idles_outputs() {
    let mut i = input(TrackMode::Track);
    i.life = false;
    let (mut svc, idles, sends) = idle_worker(i);
    let t0 = Instant::now();
    assert!(wait_until(|| idles.load(Ordering::SeqCst) >= 8));
    assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
    assert_eq!(sends.load(Ordering::SeqCst), 0);
    svc.stop();
}

/// Tracking off: every step has no values; the outputs are idled, not sent to.
#[test]
fn worker_with_no_values_idles_outputs() {
    let (mut svc, idles, sends) = idle_worker(input(TrackMode::None));
    assert!(wait_until(|| idles.load(Ordering::SeqCst) >= 8));
    assert_eq!(sends.load(Ordering::SeqCst), 0);
    svc.stop();
}
