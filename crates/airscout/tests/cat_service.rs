//! CAT settings mapping, the Doppler push and the `cat` worker thread against a fake rigctld
//! on 127.0.0.1.
#[path = "../../as-cat/tests/common/fake_rigctld.rs"]
mod fake_rigctld;

use airscout::app::{RealServices, Services};
use airscout::cat_service::{
    apply_step, cat_colors, cat_config, doppler_of, rig_reports, CatService, EXIT_WAIT,
};
use airscout::net_addrs::NetAddrs;
use airscout::services::{open_data, ServiceEvent};
use airscout::tracker::{TrackInput, TrackParams};
use airscout::tracking::{TrackStep, TrackValues};
use as_aircraft::enums::TrackMode;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_cat::{
    CatEvent, Doppler, DopplerCell, DopplerTarget, Engine, RigInfo, RigMode, RigRit, RigSplit,
    RigStatus,
};
use as_settings::{CatEngine, DopplerStrategy, Settings};
use chrono::{TimeZone, Utc};
use fake_rigctld::{FakeRigctld, RigState};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);

const LEAVE: [&str; 9] = [
    "M USB -1",
    "S 0 VFOA",
    "U RIT 0",
    "F 144300000",
    "s",
    "F 144300000",
    "S 0 VFOA",
    "F 144300000",
    "M USB -1",
];

fn target(strategy: Doppler, rx_hz: i64, tx_hz: i64) -> DopplerTarget {
    DopplerTarget {
        strategy,
        rx_hz,
        tx_hz,
    }
}

fn step(mode: TrackMode, rx_hz: i64, tx_hz: i64) -> TrackStep {
    let mut v = TrackValues::new(Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap());
    v.my_azimuth = 10.0;
    v.my_elevation = 1.0;
    v.rx_hz = rx_hz;
    v.tx_hz = tx_hz;
    TrackStep {
        values: Some(v),
        mode,
    }
}

fn rigctld_settings(port: u16, update_ms: u64) -> Settings {
    let mut s = Settings::default();
    s.cat.activate = true;
    s.cat.engine = CatEngine::Rigctld;
    s.cat.rigctld_host = "127.0.0.1".into();
    s.cat.rigctld_port = port;
    s.cat.update_ms = update_ms;
    s
}

/// Collect the CAT events of `generation` until `done` holds for them or `max` has passed.
fn cat_events_until(
    rx: &Receiver<ServiceEvent>,
    generation: u64,
    max: Duration,
    mut done: impl FnMut(&[CatEvent]) -> bool,
) -> Vec<CatEvent> {
    let end = Instant::now() + max;
    let mut ev = Vec::new();
    while !done(&ev) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left.min(Duration::from_millis(50))) {
            Ok(ServiceEvent::Cat {
                generation: g,
                event,
            }) if g == generation => ev.push(event),
            Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    ev
}

fn cell(t: DopplerTarget) -> DopplerCell {
    Arc::new(Mutex::new(t))
}

#[test]
fn apply_step_track_sets_strategy_and_freqs() {
    let mut t = DopplerTarget::default();
    apply_step(
        &mut t,
        &step(TrackMode::Track, 144_300_312, 144_300_000),
        Doppler::A,
    );
    assert_eq!(t, target(Doppler::A, 144_300_312, 144_300_000));
}

#[test]
fn apply_step_strategy_none_keeps_freqs() {
    let mut t = target(Doppler::A, 1, 2);
    apply_step(
        &mut t,
        &step(TrackMode::Track, 144_300_312, 144_300_000),
        Doppler::None,
    );
    assert_eq!(t, target(Doppler::None, 1, 2));
}

#[test]
fn apply_step_zero_freq_keeps_old() {
    let mut t = target(Doppler::A, 144_300_100, 144_300_000);
    apply_step(&mut t, &step(TrackMode::Track, 312, 0), Doppler::A);
    assert_eq!(t, target(Doppler::A, 144_300_100, 144_300_000));
}

#[test]
fn apply_step_single_ends_none_with_freqs() {
    let mut t = DopplerTarget::default();
    apply_step(
        &mut t,
        &step(TrackMode::Single, 144_300_000, 144_300_000),
        Doppler::A,
    );
    assert_eq!(t, target(Doppler::None, 144_300_000, 144_300_000));
}

#[test]
fn apply_step_invalid_is_none() {
    let mut t = target(Doppler::A, 144_300_312, 144_300_000);
    let s = TrackStep {
        values: None,
        mode: TrackMode::Track,
    };
    apply_step(&mut t, &s, Doppler::A);
    assert_eq!(t.strategy, Doppler::None);
}

#[test]
fn doppler_of_maps_every_strategy() {
    for (d, e) in [
        (DopplerStrategy::None, Doppler::None),
        (DopplerStrategy::A, Doppler::A),
        (DopplerStrategy::B, Doppler::B),
        (DopplerStrategy::C, Doppler::C),
        (DopplerStrategy::D, Doppler::D),
    ] {
        assert_eq!(doppler_of(d), e);
    }
}

#[test]
fn cat_config_clamps() {
    let mut s = rigctld_settings(4533, 0);
    s.cat.timeout_ms = 60_000;
    let c = cat_config(&s);
    assert_eq!(c.update, Duration::from_millis(100));
    assert_eq!(c.timeout, Duration::from_millis(10_000));
    assert_eq!(
        c.engine,
        Engine::Rigctld {
            host: "127.0.0.1".into(),
            port: 4533
        }
    );
    s.cat.engine = CatEngine::None;
    assert_eq!(cat_config(&s).engine, Engine::None);
}

#[test]
fn cat_config_hamlib_carries_serial() {
    let mut s = Settings::default();
    s.cat.engine = CatEngine::Hamlib;
    s.cat.hamlib_model = 2;
    s.cat.port_name = "/dev/ttyTEST0".into();
    s.cat.baudrate = 19_200;
    s.cat.data_bits = 7;
    s.cat.parity = as_settings::Parity::Even;
    s.cat.stop_bits = as_settings::StopBits::Two;
    s.cat.rts = as_settings::FlowControl::High;
    s.cat.dtr = as_settings::FlowControl::Handshake;
    let Engine::Hamlib { model, serial } = cat_config(&s).engine else {
        panic!("not hamlib");
    };
    assert_eq!(model, 2);
    assert_eq!(serial.port_name, "/dev/ttyTEST0");
    assert_eq!(serial.baudrate, 19_200);
    assert_eq!(serial.data_bits, 7);
    assert_eq!(serial.parity, as_cat::Parity::Even);
    assert_eq!(serial.stop_bits, as_cat::StopBits::Two);
    assert_eq!(serial.rts, as_cat::FlowControl::High);
    assert_eq!(serial.dtr, as_cat::FlowControl::Handshake);
}

#[test]
fn cat_colors_match_csharp() {
    let red = ([255, 255, 0], [255, 0, 0]);
    assert_eq!(
        cat_colors(RigStatus::Online),
        ([255, 255, 255], [0, 100, 0])
    );
    for s in [
        RigStatus::Error,
        RigStatus::NoCat,
        RigStatus::NoPort,
        RigStatus::NoRig,
        RigStatus::NotSuitable,
    ] {
        assert_eq!(cat_colors(s), red, "{s:?}");
    }
    assert_eq!(
        cat_colors(RigStatus::Offline),
        ([255, 255, 255], [255, 140, 0])
    );
    assert_eq!(
        cat_colors(RigStatus::None),
        ([169, 169, 169], [240, 240, 240])
    );
}

#[test]
fn rig_reports_text() {
    let i = RigInfo {
        rx_hz: 144_300_312,
        tx_hz: 144_300_000,
        mode: RigMode::CwR,
        split: RigSplit::SplitOn,
        rit: RigRit::RitOn,
        doppler_active: false,
    };
    assert_eq!(
        rig_reports(&i),
        "Rig reports RX: 144300312Hz, TX: 144300000Hz, Mode: CW_R, RIT: ON, Split: ON"
    );
    let off = RigInfo {
        split: RigSplit::SplitOff,
        rit: RigRit::None,
        ..i
    };
    assert!(rig_reports(&off).ends_with("RIT: OFF, Split: OFF"));
}

#[test]
fn exit_wait_is_two_seconds() {
    assert_eq!(EXIT_WAIT, Duration::from_secs(2));
}

#[test]
fn service_reports_online_against_fake_rigctld() {
    let fake = FakeRigctld::start(RigState::default());
    let (tx, rx) = channel();
    let cfg = cat_config(&rigctld_settings(fake.addr.port(), 100));
    let mut svc = CatService::start(cfg, cell(DopplerTarget::default()), tx, 7);
    let ev = cat_events_until(&rx, 7, WAIT, |ev| {
        ev.iter().any(|e| matches!(e, CatEvent::Info(_)))
    });
    let want_text = |t: &str| CatEvent::Text(t.into());
    let pos = |f: &dyn Fn(&CatEvent) -> bool| ev.iter().position(f).unwrap_or(usize::MAX);
    let started = pos(&|e| *e == want_text("CAT Interface started."));
    let opened = pos(&|e| *e == want_text("Opening CAT: Success!"));
    let online = pos(&|e| *e == CatEvent::Status(RigStatus::Online));
    let info = pos(&|e| {
        matches!(
            e,
            CatEvent::Info(RigInfo {
                rx_hz: 144_300_000,
                ..
            })
        )
    });
    assert!(
        started < opened && opened < online && online < info && info < ev.len(),
        "{ev:?}"
    );
    svc.stop();
    assert!(svc.wait(WAIT));
}

#[test]
fn service_pushes_doppler_and_restores_on_stop() {
    let fake = FakeRigctld::start(RigState::default());
    let (tx, rx) = channel();
    let cfg = cat_config(&rigctld_settings(fake.addr.port(), 100));
    let c = cell(target(Doppler::A, 144_300_312, 144_300_000));
    let mut svc = CatService::start(cfg, c, tx, 3);
    assert!(fake.wait_for_line("F 144300312", WAIT));
    assert!(fake.wait_for_line("I 144300000", WAIT));
    svc.stop();
    svc.stop();
    assert!(svc.wait(WAIT));
    let lines = fake.lines();
    assert!(lines.len() >= LEAVE.len(), "{lines:?}");
    assert_eq!(&lines[lines.len() - LEAVE.len()..], LEAVE, "{lines:?}");
    let last = rx
        .try_iter()
        .filter_map(|e| match e {
            ServiceEvent::Cat {
                generation: 3,
                event,
            } => Some(event),
            _ => None,
        })
        .last();
    assert_eq!(last, Some(CatEvent::Status(RigStatus::None)));
}

#[test]
fn refused_rigctld_reports_nocat_once() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let (tx, rx) = channel();
    let mut svc = CatService::start(
        cat_config(&rigctld_settings(port, 100)),
        cell(DopplerTarget::default()),
        tx,
        1,
    );
    let nocat = CatEvent::Status(RigStatus::NoCat);
    let mut ev = cat_events_until(&rx, 1, WAIT, |ev| ev.contains(&nocat));
    assert!(ev.contains(&nocat), "{ev:?}");
    ev.extend(cat_events_until(&rx, 1, Duration::from_secs(3), |_| false));
    assert_eq!(ev.iter().filter(|e| **e == nocat).count(), 1, "{ev:?}");
    let texts = ev
        .iter()
        .filter(|e| matches!(e, CatEvent::Text(t) if t.starts_with("rigctld 127.0.0.1:")))
        .count();
    assert_eq!(texts, 1, "{ev:?}");
    svc.stop();
    assert!(svc.wait(WAIT));
}

#[test]
fn real_services_start_stop_cat() {
    let fake = FakeRigctld::start(RigState::default());
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = channel();
    let mut svc =
        RealServices::new(open_data(dir.path()).unwrap(), tx).with_net(NetAddrs::loopback());
    let mut s = rigctld_settings(fake.addr.port(), 100);
    s.track.activate = true;
    s.server.activate = false;
    s.doppler.strategy = DopplerStrategy::A;
    s.doppler.dial_freq_hz = 144_300_000;
    (s.my_lat, s.my_lon, s.dx_lat, s.dx_lon) = (50.5, 1.0, 51.5, 3.0);
    s.planes_min_alt_m = 0;
    // a plane reported now, between My and DX
    let cache = Arc::new(PlaneInfoCache::new());
    cache.set(PlaneInfo::new(
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
    svc.start_integrations(&s, 1, cache);
    let online = CatEvent::Status(RigStatus::Online);
    let ev = cat_events_until(&rx, 1, WAIT, |ev| ev.contains(&online));
    assert!(ev.contains(&online), "{ev:?}");
    // the Track worker steps the input and pushes the Doppler target into the CAT cell
    svc.track_input(TrackInput {
        rev: 1,
        life: true,
        mode: TrackMode::Track,
        hex: Some("A1B2C3".into()),
        heights: Some((0.0, 0.0)),
        params: Some(TrackParams::from_settings(&s)),
    });
    assert!(
        fake.wait_for_line("I 144300000", WAIT),
        "{:?}",
        fake.lines()
    );
    let end = Instant::now() + WAIT;
    while Instant::now() < end && !fake.lines().iter().any(|l| l.starts_with("F 1443")) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        fake.lines().iter().any(|l| l.starts_with("F 1443")),
        "{:?}",
        fake.lines()
    );
    svc.stop_integrations_and_wait(Duration::from_secs(5));
    let lines = fake.lines();
    assert!(lines.len() >= LEAVE.len(), "{lines:?}");
    assert_eq!(&lines[lines.len() - LEAVE.len()..], LEAVE, "{lines:?}");
}

#[test]
fn omnirig_rig_clamped() {
    let mut s = Settings::default();
    s.cat.engine = CatEngine::OmniRig;
    for (n, want) in [(0, 1), (1, 1), (3, 3), (4, 4), (9, 4)] {
        s.cat.omnirig_rig = n;
        assert_eq!(cat_config(&s).engine, Engine::OmniRig { rig: want }, "{n}");
    }
}
