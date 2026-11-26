//! Spectrum Lab poller and spectrum state (mockito, no network, no display).
use airscout::services::ServiceEvent;
use airscout::speclab::{SpecLabService, SpectrumState, SPECTRUM_MAX_POINTS};
use as_settings::SpecLab;
use as_signals::SignalDb;
use chrono::{DateTime, Utc};
use mockito::{Matcher, Server, ServerGuard};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(20);
const FAST: Duration = Duration::from_millis(5);

/// A Spectrum Lab `_fft_expt.json` with time stamp `t` and maximum -60.25.
fn fft(t: u64) -> String {
    format!(
        "[{{\"t_unix\":{t}.5,\"f0\":0.0,\"df\":5.3833,\"ch\":1,\"f1\":400,\"f2\":1600,\"nbins\":4,\"unit\":\"dB\"}},\r\n[-87.5,-60.25,-75.0,-99.9]\r\n]"
    )
}

fn settings(url: &str) -> SpecLab {
    SpecLab {
        enabled: true,
        url: url.to_string(),
        ..SpecLab::default()
    }
}

/// Server whose body carries a new time stamp on every request.
fn server() -> (ServerGuard, mockito::Mock) {
    let mut srv = Server::new();
    let n = AtomicU64::new(1_795_438_000);
    let m = srv
        .mock("GET", "/_fft_expt.json")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("f1".into(), "400".into()),
            Matcher::UrlEncoded("f2".into(), "1600".into()),
        ]))
        .with_body_from_request(move |_| fft(n.fetch_add(1, Ordering::SeqCst)).into_bytes())
        .expect_at_least(1)
        .create();
    (srv, m)
}

struct Started {
    svc: SpecLabService,
    rx: Receiver<ServiceEvent>,
    signals: Arc<SignalDb>,
    record: Arc<AtomicBool>,
}

fn start(url: &str, record: bool) -> Started {
    let (tx, rx) = channel();
    let signals = Arc::new(SignalDb::open_in_memory().unwrap());
    let record = Arc::new(AtomicBool::new(record));
    let svc = SpecLabService::start_with_interval(
        &settings(url),
        signals.clone(),
        record.clone(),
        tx,
        Arc::new(AtomicBool::new(false)),
        FAST,
    );
    Started {
        svc,
        rx,
        signals,
        record,
    }
}

/// Wait for `n` spectrum events; their `(at, level)`.
fn levels(rx: &Receiver<ServiceEvent>, n: usize) -> Vec<(DateTime<Utc>, f64)> {
    let end = Instant::now() + WAIT;
    let mut got = Vec::new();
    while got.len() < n {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left).expect("not enough spectrum events") {
            ServiceEvent::Spectrum { at, level } => got.push((at, level)),
            ServiceEvent::SpectrumError(e) => panic!("unexpected error: {e}"),
            _ => {}
        }
    }
    got
}

#[test]
fn poller_sends_levels() {
    let (srv, mock) = server();
    let s = start(&srv.url(), false);
    let got = levels(&s.rx, 3);
    s.svc.stop();
    // maximum of the bins; the last character of the last bin is dropped (logic-findings),
    // so -99.9 reads -99. and the maximum is still -60.25; time stamp truncated to seconds
    assert!(got.iter().all(|(_, l)| *l == -60.25));
    assert_eq!(got[0].0.timestamp(), 1_795_438_000);
    assert_eq!(got[1].0.timestamp(), 1_795_438_001);
    mock.assert();
}

#[test]
fn records_every_60_samples_only_while_flag_set() {
    let (srv, _m) = server();
    let s = start(&srv.url(), false);
    // flag clear: levels arrive, nothing is stored
    levels(&s.rx, 70);
    assert_eq!(s.signals.count().unwrap(), 0);
    // flag set: buffered, written in one batch when 60 are collected
    s.record.store(true, Ordering::SeqCst);
    // drain what was already in flight with the flag clear, then count from the switch
    let end = Instant::now() + WAIT;
    while s.signals.count().unwrap() == 0 {
        assert!(Instant::now() < end, "no batch written");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(s.signals.count().unwrap(), 60);
    // and stays at 60 until the next 60 are in
    s.record.store(false, Ordering::SeqCst);
    levels(&s.rx, 20);
    s.svc.stop();
    assert_eq!(s.signals.count().unwrap(), 60);
}

#[test]
fn poller_error_reported_not_fatal() {
    let mut srv = Server::new();
    let _bad = srv
        .mock("GET", Matcher::Any)
        .with_status(500)
        .expect_at_least(1)
        .create();
    let s = start(&srv.url(), false);
    let end = Instant::now() + WAIT;
    let mut errors = 0;
    while errors < 2 {
        let left = end.saturating_duration_since(Instant::now());
        if let ServiceEvent::SpectrumError(e) = s.rx.recv_timeout(left).expect("no error event") {
            assert!(e.contains("500"), "{e}");
            errors += 1;
        }
    }
    // not fatal: the poller kept going after the first error
    s.svc.stop();
}

#[test]
fn ring_keeps_600() {
    let mut sp = SpectrumState::default();
    for i in 0..700 {
        sp.push(f64::from(i), i % 2 == 0);
    }
    assert_eq!(SPECTRUM_MAX_POINTS, 600);
    assert_eq!(sp.len(), 600);
    // the oldest 100 are gone
    assert_eq!(sp.levels().next(), Some(100.0));
    assert_eq!(sp.levels().last(), Some(699.0));
    // the recorded flags keep their samples: 100 is even
    assert!(sp.recorded().next().unwrap());
}

#[test]
fn y_range_defaults_then_autoscales_by_10_percent() {
    let mut sp = SpectrumState::default();
    assert_eq!(sp.y_range(), (-120.0, 0.0));
    sp.push(-80.0, false);
    sp.push(-60.0, false);
    let (lo, hi) = sp.y_range();
    assert!(
        (lo + 82.0).abs() < 1e-9 && (hi + 58.0).abs() < 1e-9,
        "{lo} {hi}"
    );
}

// ------------------------------------------------------------------ app wiring

use airscout::app::{AppState, BottomTab, Services};
use airscout::services::PathRequest;
use airscout::ui::map::MapView;
use airscout::ui::spectrum::recorded_runs;
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use as_settings::Settings;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::sync::Mutex;

#[derive(Default)]
struct Calls {
    started: u32,
    stopped: u32,
    record: Option<Arc<AtomicBool>>,
}

struct Recorder(Arc<Mutex<Calls>>);

impl Services for Recorder {
    fn request_path(&mut self, _req: PathRequest) {}
    fn start_feed(
        &mut self,
        _id: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn start_speclab(&mut self, _s: &Settings, record: Arc<AtomicBool>) {
        let mut c = self.0.lock().unwrap();
        c.started += 1;
        c.record = Some(record);
    }
    fn stop_speclab(&mut self) {
        self.0.lock().unwrap().stopped += 1;
    }
}

fn app(enabled: bool) -> (AppState, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut s = Settings::default();
    s.speclab.enabled = enabled;
    let st = AppState::new(
        s,
        None,
        Box::new(Recorder(calls.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    (st, calls)
}

#[test]
fn play_starts_poller_only_when_enabled_and_pause_stops_it() {
    let (mut st, calls) = app(false);
    st.play().unwrap();
    assert_eq!(calls.lock().unwrap().started, 0);
    st.pause();

    let (mut st, calls) = app(true);
    st.play().unwrap();
    assert_eq!(calls.lock().unwrap().started, 1);
    let stopped = calls.lock().unwrap().stopped;
    st.pause();
    assert_eq!(calls.lock().unwrap().stopped, stopped + 1);
}

#[test]
fn record_flag_follows_nearest_plane_and_events_fill_the_ring() {
    let (mut st, calls) = app(true);
    st.play().unwrap();
    let record = calls.lock().unwrap().record.clone().unwrap();
    // a tick without planes: nothing is nearest
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    assert!(!record.load(Ordering::SeqCst));
    st.on_event(ServiceEvent::Spectrum {
        at: Utc::now(),
        level: -70.0,
    });
    st.tick.nearest = Some(PlaneInfo::default());
    record.store(true, Ordering::SeqCst);
    st.on_event(ServiceEvent::Spectrum {
        at: Utc::now(),
        level: -60.0,
    });
    assert_eq!(st.spectrum.len(), 2);
    assert_eq!(recorded_runs(&st.spectrum), vec![(1.0, 2.0)]);
    st.on_event(ServiceEvent::SpectrumError("boom".into()));
    assert!(st.spectrum.is_error && st.spectrum.status == "boom");
    // events of a poller that was just stopped are dropped
    st.pause();
    st.on_event(ServiceEvent::Spectrum {
        at: Utc::now(),
        level: -50.0,
    });
    assert_eq!(st.spectrum.len(), 2);
    assert!(!record.load(Ordering::SeqCst));
}

#[test]
fn spectrum_tab_renders_status_and_nearest() {
    let (mut st, _c) = app(true);
    st.bottom_tab = BottomTab::Spectrum;
    st.spectrum.push(-80.0, true);
    st.spectrum.on_level(Utc::now(), -60.0);
    st.tick.nearest = Some(PlaneInfo {
        call: "DLH123".into(),
        ..PlaneInfo::default()
    });
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, map): &mut (AppState, MapView)| airscout::ui::show(ui, st, map),
            (st, MapView::without_tiles()),
        );
    h.run();
    h.get_by_label_contains("Spectrum Lab: -60.0 dB");
    h.get_by_label_contains("Nearest: DLH123");
}

#[test]
fn cancel_flushes_buffered_samples() {
    let (srv, _m) = server();
    let s = start(&srv.url(), true);
    // fewer than 60 recorded samples, then Pause
    levels(&s.rx, 5);
    s.svc.stop();
    let end = Instant::now() + WAIT;
    while s.signals.count().unwrap() == 0 {
        assert!(Instant::now() < end, "buffered samples were dropped");
        std::thread::sleep(Duration::from_millis(5));
    }
    let n = s.signals.count().unwrap();
    assert!((5..60).contains(&n), "{n}");
}

#[test]
fn y_range_has_minimum_span() {
    let mut sp = SpectrumState::default();
    sp.push(-80.0, false);
    let (lo, hi) = sp.y_range();
    assert!(hi - lo >= 10.0 && lo < -80.0 && hi > -80.0, "{lo} {hi}");
    // constant levels, also near the edge of -120..0: stays inside where possible
    let mut sp = SpectrumState::default();
    sp.push(-119.0, false);
    sp.push(-119.0, false);
    let (lo, hi) = sp.y_range();
    assert!(hi - lo >= 10.0 && lo >= -120.0, "{lo} {hi}");
    let mut sp = SpectrumState::default();
    sp.push(0.0, false);
    let (lo, hi) = sp.y_range();
    assert!(hi - lo >= 10.0 && hi <= 0.0, "{lo} {hi}");
}
