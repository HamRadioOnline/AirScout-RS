//! HRD rotator output over DDE: poke bytes and fanout behaviour, with a fake `DdeLink` (all OS).

use airscout::track_outputs::{hrd_pokes, DdeOutput, OutputGroup, TrackOutput};
use airscout::track_service::{TrackEvent, TrackFanout};
use airscout::tracking::{TrackStep, TrackValues};
use as_aircraft::enums::TrackMode;
use as_dde::{DdeLink, CF_TEXT, ITEM, POKE_TIMEOUT, SERVICE, TOPIC};
use chrono::Utc;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const NOT_CONNECTED: &str = "[DDE]: Client not connected.";

type Poke = (String, Vec<u8>, u32, Duration);

#[derive(Default)]
struct Log {
    /// Scripted results of `connect`, front first; empty means success.
    connects: VecDeque<bool>,
    connect_calls: u32,
    targets: Vec<(String, String)>,
    pokes: Vec<Poke>,
    poke_result: bool,
    disconnects: u32,
    /// The server has ended the conversation: `is_connected` is false, `connect` fails.
    gone: bool,
    is_connected_calls: u32,
}

struct FakeLink {
    log: Arc<Mutex<Log>>,
    connected: bool,
}

impl DdeLink for FakeLink {
    fn connect(&mut self, service: &str, topic: &str) -> Result<(), String> {
        let mut l = self.log.lock().unwrap();
        l.connect_calls += 1;
        l.targets.push((service.into(), topic.into()));
        if !l.gone && l.connects.pop_front().unwrap_or(true) {
            self.connected = true;
            Ok(())
        } else {
            Err(format!("DDE: {service}/{topic}: error 16394"))
        }
    }

    fn is_connected(&self) -> bool {
        let mut l = self.log.lock().unwrap();
        l.is_connected_calls += 1;
        self.connected && !l.gone
    }

    fn poke(&mut self, item: &str, data: &[u8], format: u32, timeout: Duration) -> bool {
        let mut l = self.log.lock().unwrap();
        l.pokes.push((item.into(), data.to_vec(), format, timeout));
        l.poke_result
    }

    fn disconnect(&mut self) {
        self.log.lock().unwrap().disconnects += 1;
        self.connected = false;
    }
}

fn fake(connects: &[bool], poke_result: bool) -> (DdeOutput, Arc<Mutex<Log>>) {
    let log = Arc::new(Mutex::new(Log {
        connects: connects.iter().copied().collect(),
        poke_result,
        ..Log::default()
    }));
    let link = FakeLink {
        log: log.clone(),
        connected: false,
    };
    (DdeOutput::new(Box::new(link)), log)
}

fn step(az: f64, el: f64) -> TrackStep {
    let mut v = TrackValues::new(Utc::now());
    v.my_azimuth = az;
    v.my_elevation = el;
    TrackStep {
        values: Some(v),
        mode: TrackMode::Track,
    }
}

fn texts(ev: &[TrackEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            TrackEvent::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn constants_match_csharp() {
    assert_eq!(SERVICE, "HRDRotator");
    assert_eq!(TOPIC, "Position");
    assert_eq!(ITEM, "PositionData");
    assert_eq!(CF_TEXT, 1);
    assert_eq!(POKE_TIMEOUT, Duration::from_millis(10_000));
}

#[test]
fn hrd_poke_bytes() {
    assert_eq!(
        hrd_pokes(123.45, 5.0),
        vec![b"SET-AZ:123.5".to_vec(), b"SET-EL:5.0".to_vec()]
    );
    assert_eq!(hrd_pokes(359.0, 95.0), vec![b"SET-AZ:359.0".to_vec()]);
    assert_eq!(hrd_pokes(10.0, -1.0), vec![b"SET-AZ:10.0".to_vec()]);
    // both bounds are inclusive (`MapDlg.cs:7849`, `:7855`)
    assert_eq!(
        hrd_pokes(360.0, 90.0),
        vec![b"SET-AZ:360.0".to_vec(), b"SET-EL:90.0".to_vec()]
    );
    assert_eq!(hrd_pokes(-0.1, 0.0), vec![b"SET-EL:0.0".to_vec()]);
    assert!(hrd_pokes(f64::NAN, f64::NAN).is_empty());
}

#[test]
fn pokes_use_cf_text_and_10s() {
    let (mut o, log) = fake(&[], true);
    assert_eq!(o.group(), OutputGroup::Dde);
    o.connect().unwrap();
    assert!(o.is_connected());
    o.send(123.45, 5.0).unwrap();
    let l = log.lock().unwrap();
    assert_eq!(l.targets, vec![("HRDRotator".into(), "Position".into())]);
    assert_eq!(l.pokes.len(), 2);
    for (item, _, format, timeout) in &l.pokes {
        assert_eq!(item, "PositionData");
        assert_eq!(*format, 1);
        assert_eq!(*timeout, Duration::from_secs(10));
    }
    assert_eq!(l.pokes[0].1, b"SET-AZ:123.5");
    assert_eq!(l.pokes[1].1, b"SET-EL:5.0");
}

#[test]
fn not_connected_is_counted_and_reconnected() {
    let (o, log) = fake(&[false; 20], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    for n in 1..=10 {
        let ev = fan.step(&step(123.4, 5.0));
        assert_eq!(texts(&ev), vec![NOT_CONNECTED.to_string()], "step {n}");
        assert!(!ev.iter().any(|e| matches!(e, TrackEvent::Disabled(_))));
    }
    let ev = fan.step(&step(123.4, 5.0));
    assert_eq!(
        texts(&ev),
        vec![
            NOT_CONNECTED.to_string(),
            "Tracking via DDE disabled.".to_string()
        ]
    );
    assert!(ev.contains(&TrackEvent::Disabled(OutputGroup::Dde)));
    {
        let l = log.lock().unwrap();
        assert_eq!(l.connect_calls, 11);
        assert!(l.pokes.is_empty());
    }
    // switched off: no more connects or texts
    assert!(texts(&fan.step(&step(123.4, 5.0))).is_empty());
    assert_eq!(log.lock().unwrap().connect_calls, 11);
}

#[test]
fn late_server_is_picked_up() {
    let (o, log) = fake(&[false, false, true], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    for _ in 0..2 {
        assert_eq!(
            texts(&fan.step(&step(123.4, 5.0))),
            vec![NOT_CONNECTED.to_string()]
        );
    }
    assert!(log.lock().unwrap().pokes.is_empty());
    assert!(texts(&fan.step(&step(123.4, 5.0))).is_empty());
    let l = log.lock().unwrap();
    assert_eq!(l.connect_calls, 3);
    assert_eq!(l.pokes.len(), 2);
}

#[test]
fn poke_failure_is_silent() {
    let (o, log) = fake(&[], false);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    for _ in 0..3 {
        assert!(texts(&fan.step(&step(123.4, 5.0))).is_empty());
    }
    let l = log.lock().unwrap();
    // connected once, then every step pokes AZ and EL
    assert_eq!(l.connect_calls, 1);
    assert_eq!(l.pokes.len(), 6);
}

#[test]
fn close_disconnects() {
    let (o, log) = fake(&[], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    fan.step(&step(123.4, 5.0));
    fan.close();
    assert_eq!(log.lock().unwrap().disconnects, 1);
}

#[test]
fn server_gone_is_counted_and_disabled() {
    let (o, log) = fake(&[], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    assert!(texts(&fan.step(&step(123.4, 5.0))).is_empty());
    assert_eq!(log.lock().unwrap().pokes.len(), 2);
    // HRD exits: the conversation is lost, every step after it counts (`MapDlg.cs:7845-7846`)
    log.lock().unwrap().gone = true;
    for n in 1..=10 {
        let ev = fan.step(&step(123.4, 5.0));
        assert_eq!(texts(&ev), vec![NOT_CONNECTED.to_string()], "error {n}");
    }
    let ev = fan.step(&step(123.4, 5.0));
    assert_eq!(
        texts(&ev),
        vec![
            NOT_CONNECTED.to_string(),
            "Tracking via DDE disabled.".to_string()
        ]
    );
    assert!(ev.contains(&TrackEvent::Disabled(OutputGroup::Dde)));
    let l = log.lock().unwrap();
    // one connect that worked, then one try per counted step
    assert_eq!(l.connect_calls, 12);
    assert_eq!(l.pokes.len(), 2);
    assert_eq!(l.disconnects, 1);
}

/// Idle passes reach the link (`Ddeml::is_connected` drains the thread's queue) without a poke.
#[test]
fn idle_reaches_the_link_without_poking() {
    let (mut o, log) = fake(&[], true);
    o.connect().unwrap();
    let before = log.lock().unwrap().is_connected_calls;
    for _ in 0..5 {
        o.idle();
    }
    let l = log.lock().unwrap();
    assert_eq!(l.is_connected_calls, before + 5);
    assert!(l.pokes.is_empty());
}

#[test]
fn stopped_tracking_idles_the_link() {
    let (o, log) = fake(&[], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    fan.step(&step(123.4, 5.0));
    let (calls, pokes) = {
        let l = log.lock().unwrap();
        (l.is_connected_calls, l.pokes.len())
    };
    let stopped = TrackStep {
        values: None,
        mode: TrackMode::Track,
    };
    for _ in 0..3 {
        assert!(texts(&fan.step(&stopped)).is_empty());
    }
    let l = log.lock().unwrap();
    assert_eq!(l.is_connected_calls, calls + 3);
    assert_eq!(l.pokes.len(), pokes);
}

/// The server goes away while tracking is idle: the idle pass tears the link down once (so no
/// DDE instance is left unpumped), never pokes and never counts; the next valid step reconnects.
#[test]
fn server_gone_while_idle_tears_down_once() {
    let (o, log) = fake(&[], true);
    let mut fan = TrackFanout::new(vec![Box::new(o)]);
    fan.step(&step(123.4, 5.0));
    let stopped = TrackStep {
        values: None,
        mode: TrackMode::Track,
    };
    log.lock().unwrap().gone = true;
    for _ in 0..5 {
        assert!(texts(&fan.step(&stopped)).is_empty());
        fan.idle();
    }
    {
        let l = log.lock().unwrap();
        assert_eq!(l.disconnects, 1);
        assert_eq!(l.connect_calls, 1);
        assert_eq!(l.pokes.len(), 2);
    }
    // HRD is back: the next valid step connects and pokes, without a counted error
    log.lock().unwrap().gone = false;
    assert!(texts(&fan.step(&step(123.4, 5.0))).is_empty());
    let l = log.lock().unwrap();
    assert_eq!(l.connect_calls, 2);
    assert_eq!(l.pokes.len(), 4);
}

#[test]
fn idle_without_a_link_does_not_disconnect() {
    let (mut o, log) = fake(&[false], true);
    let _ = o.connect();
    for _ in 0..3 {
        o.idle();
    }
    assert_eq!(log.lock().unwrap().disconnects, 0);
}
