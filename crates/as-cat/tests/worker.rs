use as_cat::{
    make_rig, run, CatConfig, CatEvent, CatWorker, Doppler, DopplerTarget, Engine, Rig, RigIdent,
    RigInfo, RigMode, RigRit, RigSplit, RigStatus,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const UPDATE: Duration = Duration::from_millis(500);

struct State {
    status: RigStatus,
    error: Option<String>,
    /// Set as the error on every NoCat get_status, like the real rig.
    nocat_text: Option<String>,
    rit: RigRit,
    rx_hz: i64,
    tx_hz: i64,
    log: Vec<String>,
    /// The cat_version the rig reports from its first status poll on.
    ident_on_poll: Option<String>,
    cat_version: String,
}

#[derive(Clone)]
struct FakeRig(Arc<Mutex<State>>);

impl FakeRig {
    fn new() -> Self {
        FakeRig(Arc::new(Mutex::new(State {
            status: RigStatus::Online,
            error: None,
            nocat_text: None,
            rit: RigRit::RitOff,
            rx_hz: 144_300_000,
            tx_hz: 144_300_000,
            log: Vec::new(),
            ident_on_poll: None,
            cat_version: "Fake".into(),
        })))
    }
    fn set_status(&self, s: RigStatus) {
        self.0.lock().unwrap().status = s;
    }
    fn set_freqs(&self, rx: i64, tx: i64) {
        let mut s = self.0.lock().unwrap();
        s.rx_hz = rx;
        s.tx_hz = tx;
    }
    fn log(&self) -> Vec<String> {
        self.0.lock().unwrap().log.clone()
    }
    fn count(&self, what: &str) -> usize {
        self.log().iter().filter(|l| *l == what).count()
    }
    fn note(&self, s: String) {
        self.0.lock().unwrap().log.push(s);
    }
}

impl Rig for FakeRig {
    fn ident(&self) -> RigIdent {
        RigIdent {
            cat_version: self.0.lock().unwrap().cat_version.clone(),
            rig_type: "[Fake] test".into(),
        }
    }
    fn get_status(&mut self, _: Doppler) -> RigStatus {
        self.note("get_status".into());
        let mut s = self.0.lock().unwrap();
        if let Some(v) = s.ident_on_poll.take() {
            s.cat_version = v;
        }
        if s.status == RigStatus::NoCat {
            s.error = s.nocat_text.clone();
        }
        s.status
    }
    fn take_error(&mut self) -> Option<String> {
        self.0.lock().unwrap().error.take()
    }
    fn get_mode(&mut self) -> RigMode {
        self.note("get_mode".into());
        RigMode::Usb
    }
    fn get_split(&mut self) -> RigSplit {
        self.note("get_split".into());
        RigSplit::SplitOff
    }
    fn get_rit(&mut self) -> RigRit {
        self.note("get_rit".into());
        self.0.lock().unwrap().rit
    }
    fn get_rx_frequency(&mut self) -> i64 {
        self.0.lock().unwrap().rx_hz
    }
    fn get_tx_frequency(&mut self) -> i64 {
        self.0.lock().unwrap().tx_hz
    }
    fn set_mode(&mut self, m: RigMode) -> bool {
        self.note(format!("set_mode({})", m.name()));
        true
    }
    fn set_split(&mut self, s: RigSplit) -> bool {
        self.note(format!("set_split({s:?})"));
        true
    }
    fn set_rit(&mut self, r: RigRit) -> bool {
        self.note(format!("set_rit({r:?})"));
        true
    }
    fn set_rx_frequency(&mut self, hz: i64) -> bool {
        self.note(format!("set_rx_frequency({hz})"));
        true
    }
    fn set_tx_frequency(&mut self, hz: i64) -> bool {
        self.note(format!("set_tx_frequency({hz})"));
        true
    }
    fn set_simplex_mode(&mut self, hz: i64, m: RigMode) -> bool {
        self.note(format!("set_simplex_mode({hz},{})", m.name()));
        true
    }
    fn set_split_mode(&mut self, rx: i64, tx: i64, m: RigMode) -> bool {
        self.note(format!("set_split_mode({rx},{tx},{})", m.name()));
        true
    }
}

fn worker(rig: &FakeRig) -> (CatWorker, Arc<Mutex<usize>>) {
    let calls = Arc::new(Mutex::new(0usize));
    let c = calls.clone();
    let r = rig.clone();
    let f = Box::new(move || {
        *c.lock().unwrap() += 1;
        Some(Box::new(r.clone()) as Box<dyn Rig>)
    });
    (CatWorker::new(f, UPDATE), calls)
}

fn info(rit: RigRit) -> RigInfo {
    RigInfo {
        rx_hz: 144_300_000,
        tx_hz: 144_300_000,
        mode: RigMode::Usb,
        split: RigSplit::SplitOff,
        rit,
        doppler_active: false,
    }
}

fn ident() -> CatEvent {
    CatEvent::Ident(Some(FakeRig::new().ident()))
}

fn text(s: &str) -> CatEvent {
    CatEvent::Text(s.into())
}

const OPEN: &str = "Opening CAT: Success!";

fn target(s: Doppler, rx: i64, tx: i64) -> DopplerTarget {
    DopplerTarget {
        strategy: s,
        rx_hz: rx,
        tx_hz: tx,
    }
}

const NONE: DopplerTarget = DopplerTarget {
    strategy: Doppler::None,
    rx_hz: 0,
    tx_hz: 0,
};

#[test]
fn no_rig_reports_ident_none_text_norig_and_waits_10s() {
    let mut w = CatWorker::new(Box::new(|| None), UPDATE);
    let (ev, wait) = w.pass(NONE);
    assert_eq!(
        ev,
        vec![
            CatEvent::Ident(None),
            text("No rig selected or rig is not supported by any available CAT!"),
            CatEvent::Status(RigStatus::NoRig),
        ]
    );
    assert_eq!(wait, Duration::from_secs(10));
}

#[test]
fn opening_reports_success_ident_status_info() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    let (ev, wait) = w.pass(NONE);
    assert_eq!(
        ev,
        vec![
            text(OPEN),
            ident(),
            CatEvent::Status(RigStatus::Online),
            CatEvent::Info(info(RigRit::RitOff)),
        ]
    );
    assert_eq!(wait, UPDATE);
    assert_eq!(w.pass(NONE).0, vec![]);
}

#[test]
fn status_change_reported_with_error_text() {
    let rig = FakeRig::new();
    rig.set_status(RigStatus::NoCat);
    rig.0.lock().unwrap().nocat_text = Some("rigctld 127.0.0.1:1: refused".into());
    let (mut w, _) = worker(&rig);
    assert_eq!(
        w.pass(NONE).0,
        vec![
            text(OPEN),
            ident(),
            text("rigctld 127.0.0.1:1: refused"),
            CatEvent::Status(RigStatus::NoCat),
        ]
    );
    assert_eq!(w.pass(NONE).0, vec![]);
    assert_eq!(w.pass(NONE).0, vec![]);
    rig.set_status(RigStatus::Online);
    assert_eq!(
        w.pass(NONE).0,
        vec![
            CatEvent::Status(RigStatus::Online),
            CatEvent::Info(info(RigRit::RitOff))
        ]
    );
}

#[test]
fn values_read_only_online() {
    let rig = FakeRig::new();
    rig.set_status(RigStatus::Offline);
    let (mut w, _) = worker(&rig);
    w.pass(NONE);
    assert_eq!(rig.count("get_mode"), 0);
    assert_eq!(rig.count("get_rit"), 0);
}

#[test]
fn info_on_any_change() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(NONE);
    assert_eq!(w.pass(NONE).0, vec![]);
    rig.0.lock().unwrap().rit = RigRit::RitOn;
    assert_eq!(w.pass(NONE).0, vec![CatEvent::Info(info(RigRit::RitOn))]);
    assert_eq!(w.pass(NONE).0, vec![]);
}

#[test]
fn rig_lost_recreates() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    w.pass(NONE);
    assert_eq!(*calls.lock().unwrap(), 1);
    rig.set_status(RigStatus::None);
    let (ev, wait) = w.pass(NONE);
    assert_eq!(
        ev,
        vec![
            text("Error while polling CAT: Rig lost during polling!"),
            CatEvent::Status(RigStatus::NoRig),
        ]
    );
    assert_eq!(wait, UPDATE);
    rig.set_status(RigStatus::Online);
    let (ev, _) = w.pass(NONE);
    assert_eq!(ev[0], text(OPEN));
    assert_eq!(*calls.lock().unwrap(), 2);
}

const ENTER: &str = "set_split_mode(144300000,144300000,USB)";
const TICK: &str = "set_split_mode(144300312,144300000,NONE)";

#[test]
fn doppler_enter_once_then_split_every_pass() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    for _ in 0..3 {
        w.pass(t);
    }
    assert_eq!(rig.count(ENTER), 1);
    assert_eq!(rig.count(TICK), 3);
}

#[test]
fn doppler_enter_retried_until_online() {
    let rig = FakeRig::new();
    rig.set_status(RigStatus::Offline);
    let (mut w, _) = worker(&rig);
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    w.pass(t);
    w.pass(t);
    assert!(!rig.log().iter().any(|l| l.starts_with("set_split_mode")));
    rig.set_status(RigStatus::Online);
    w.pass(t);
    assert_eq!(rig.count(ENTER), 1);
    assert_eq!(rig.count(TICK), 1);
}

/// The app must not take a Doppler-shifted RX as its dial base: an Info read while the Doppler
/// drives the rig says so.
#[test]
fn info_marks_doppler_active() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    let first = w.pass(NONE).0;
    assert!(
        first.contains(&CatEvent::Info(info(RigRit::RitOff))),
        "{first:?}"
    );
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    // the enter pass reads before it enters: nothing has shifted yet
    assert!(!w.pass(t).0.iter().any(|e| matches!(e, CatEvent::Info(_))));
    // the rig now runs at the shifted RX
    rig.set_freqs(144_300_312, 144_300_000);
    let shifted = RigInfo {
        rx_hz: 144_300_312,
        doppler_active: true,
        ..info(RigRit::RitOff)
    };
    assert_eq!(w.pass(t).0, vec![CatEvent::Info(shifted)]);
    // the leave pass still reads the shifted rig; the pass after the restore does not
    assert_eq!(w.pass(NONE).0, vec![]);
    rig.set_freqs(144_300_000, 144_300_000);
    assert_eq!(w.pass(NONE).0, vec![CatEvent::Info(info(RigRit::RitOff))]);
}

#[test]
fn doppler_none_leaves_once() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    w.pass(NONE);
    assert_eq!(rig.count("set_simplex_mode(144300000,USB)"), 1);
    let n = rig.log().len();
    w.pass(NONE);
    assert!(rig.log()[n..].iter().all(|l| l.starts_with("get_")));
}

#[test]
fn rig_lost_mid_doppler_keeps_saved_state() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    w.pass(t);
    assert_eq!(rig.count(ENTER), 1);
    // the rig now sits on the pushed Doppler frequencies; then rigctld restarts
    rig.set_freqs(144_300_312, 144_300_000);
    rig.set_status(RigStatus::None);
    w.pass(t);
    rig.set_status(RigStatus::Online);
    w.pass(t);
    assert_eq!(*calls.lock().unwrap(), 2);
    // the active target is unchanged, so there is no second enter (stage 4c C1)
    assert_eq!(rig.count(ENTER), 1);
    w.pass(NONE);
    assert_eq!(rig.count("set_rx_frequency(144300000)"), 1);
    assert_eq!(rig.count("set_tx_frequency(144300000)"), 1);
    assert_eq!(rig.count("set_simplex_mode(144300000,USB)"), 1);
    assert!(!rig.log().iter().any(|l| l.contains("144300312,USB")));
}

#[test]
fn strategy_none_while_rig_gone_restores_on_return() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    rig.set_freqs(144_300_312, 144_300_000);
    rig.set_status(RigStatus::None);
    w.pass(NONE);
    rig.set_status(RigStatus::Online);
    let n = rig.log().len();
    w.pass(NONE);
    assert_eq!(*calls.lock().unwrap(), 2);
    let log = rig.log();
    let pass3 = &log[n..];
    assert_eq!(
        pass3
            .iter()
            .filter(|l| *l == "set_simplex_mode(144300000,USB)")
            .count(),
        1
    );
    assert!(!pass3.iter().any(|l| l.starts_with("set_split_mode")));
    assert!(!log.iter().any(|l| l.contains("144300312,USB")));
}

#[test]
fn active_target_survives_recreate_without_second_enter() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    w.pass(t);
    rig.set_status(RigStatus::None);
    w.pass(t);
    rig.set_status(RigStatus::Online);
    let n = rig.log().len();
    w.pass(t);
    w.pass(t);
    assert_eq!(*calls.lock().unwrap(), 2);
    let log = rig.log();
    let after = &log[n..];
    assert!(!after.iter().any(|l| l == ENTER));
    assert_eq!(after.iter().filter(|l| *l == TICK).count(), 2);
}

#[test]
fn finish_recreates_missing_rig_to_restore() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    rig.set_status(RigStatus::None);
    w.pass(NONE);
    rig.set_status(RigStatus::Online);
    assert_eq!(*calls.lock().unwrap(), 1);
    let n = rig.log().len();
    assert_eq!(w.finish(), vec![CatEvent::Status(RigStatus::None)]);
    assert_eq!(*calls.lock().unwrap(), 2);
    let log = rig.log();
    assert_eq!(log[n], "get_status");
    assert!(log[n..].contains(&"set_simplex_mode(144300000,USB)".to_string()));
}

#[test]
fn finish_without_saved_state_opens_nothing() {
    let rig = FakeRig::new();
    let (mut w, calls) = worker(&rig);
    w.pass(NONE);
    rig.set_status(RigStatus::None);
    w.pass(NONE);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(w.finish(), vec![CatEvent::Status(RigStatus::None)]);
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn doppler_tick_only_online() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    let t = target(Doppler::A, 144_300_312, 144_300_000);
    w.pass(t);
    assert_eq!(rig.count(TICK), 1);
    for s in [RigStatus::Offline, RigStatus::NoCat, RigStatus::NotSuitable] {
        rig.set_status(s);
        w.pass(t);
        assert_eq!(rig.count(TICK), 1, "{s:?}");
    }
    rig.set_status(RigStatus::Online);
    w.pass(t);
    assert_eq!(rig.count(TICK), 2);
}

#[test]
fn zero_target_not_pushed() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(target(Doppler::A, 0, 0));
    assert_eq!(rig.count(ENTER), 1);
    assert!(!rig
        .log()
        .iter()
        .any(|l| l.starts_with("set_split_mode(0,") || l.contains(",0,")));
    assert_eq!(
        rig.log()
            .iter()
            .filter(|l| l.starts_with("set_split_mode"))
            .count(),
        1
    );
}

#[test]
fn doppler_leave_retried_until_online() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    rig.set_status(RigStatus::Offline);
    let n = rig.log().len();
    w.pass(NONE);
    assert!(rig.log()[n..].iter().all(|l| l.starts_with("get_")));
    rig.set_status(RigStatus::Online);
    w.pass(NONE);
    assert_eq!(rig.count("set_simplex_mode(144300000,USB)"), 1);
    assert_eq!(rig.count("set_mode(USB)"), 1);
}

#[test]
fn finish_checks_status_before_leaving() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    let n = rig.log().len();
    w.finish();
    let log = rig.log();
    assert_eq!(log[n], "get_status");
    assert!(log[n..].contains(&"set_simplex_mode(144300000,USB)".to_string()));
}

#[test]
fn finish_leaves_and_reports_none() {
    let rig = FakeRig::new();
    let (mut w, _) = worker(&rig);
    w.pass(target(Doppler::A, 144_300_312, 144_300_000));
    assert_eq!(w.finish(), vec![CatEvent::Status(RigStatus::None)]);
    assert_eq!(rig.count("set_simplex_mode(144300000,USB)"), 1);
}

#[test]
fn run_starts_and_stops_within_bounds() {
    let rig = FakeRig::new();
    let (w, _) = worker(&rig);
    let cell = Arc::new(Mutex::new(NONE));
    let cancel = AtomicBool::new(false);
    let events = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        s.spawn(|| run(w, cell, &cancel, &mut |e| events.lock().unwrap().push(e)));
        let start = Instant::now();
        while events.lock().unwrap().len() < 3 && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let t = Instant::now();
        cancel.store(true, Ordering::Relaxed);
        while !events
            .lock()
            .unwrap()
            .last()
            .is_some_and(|e| *e == CatEvent::Status(RigStatus::None))
            && t.elapsed() < Duration::from_secs(2)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(t.elapsed() < Duration::from_secs(2));
    });
    let ev = events.into_inner().unwrap();
    assert_eq!(ev[0], text("CAT Interface started."));
    assert_eq!(ev.last(), Some(&CatEvent::Status(RigStatus::None)));
}

fn cfg(engine: Engine) -> CatConfig {
    CatConfig {
        engine,
        update: UPDATE,
        timeout: Duration::from_secs(1),
    }
}

#[test]
fn make_rig_engines() {
    assert!(make_rig(&cfg(Engine::None)).is_none());
    let r = make_rig(&cfg(Engine::Rigctld {
        host: "127.0.0.1".into(),
        port: 4532,
    }))
    .unwrap();
    assert_eq!(r.ident().rig_type, "[rigctld] 127.0.0.1:4532");
}

#[cfg(not(feature = "hamlib"))]
#[test]
fn hamlib_engine_without_feature_is_norig() {
    use as_cat::{hamlib_available, SerialConfig};
    let serial = SerialConfig {
        port_name: String::new(),
        baudrate: 9600,
        data_bits: 8,
        parity: Default::default(),
        stop_bits: Default::default(),
        rts: Default::default(),
        dtr: Default::default(),
    };
    assert!(make_rig(&cfg(Engine::Hamlib { model: 1, serial })).is_none());
    assert!(!hamlib_available());
}

#[cfg(not(windows))]
#[test]
fn omnirig_engine_off_windows_is_norig() {
    assert!(make_rig(&cfg(Engine::OmniRig { rig: 1 })).is_none());
    assert!(!as_cat::omnirig_available());
}

#[test]
fn ident_is_sent_again_when_it_changes_after_the_first_poll() {
    let fake = FakeRig::new();
    fake.0.lock().unwrap().ident_on_poll = Some("Late V2".into());
    let f = fake.clone();
    let mut w = CatWorker::new(Box::new(move || Some(Box::new(f.clone()))), UPDATE);
    let idents = |ev: &[CatEvent]| -> Vec<String> {
        ev.iter()
            .filter_map(|e| match e {
                CatEvent::Ident(Some(i)) => Some(i.cat_version.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(idents(&w.pass(NONE).0), ["Fake", "Late V2"]);
    assert!(idents(&w.pass(NONE).0).is_empty());
    assert!(idents(&w.pass(NONE).0).is_empty());
}
