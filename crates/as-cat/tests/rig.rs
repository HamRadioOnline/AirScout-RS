#[path = "common/fake_backend.rs"]
mod fake_backend;

use as_cat::{
    mode_from_hamlib, mode_to_hamlib, BackendError, CatRig, Doppler, DopplerSession, Rig, RigMode,
    RigRit, RigSplit, RigStatus, Saved,
};
use fake_backend::{FakeBackend, Op};

const F: i64 = 144_300_000;

fn rig() -> (FakeBackend, CatRig<FakeBackend>) {
    let fake = FakeBackend::new();
    (fake.clone(), CatRig::new(fake))
}

#[test]
fn enter_saves_state_and_sets_split_rx_rx() {
    let (fake, mut rig) = rig();
    let mut d = DopplerSession::default();
    assert!(d.enter(&mut rig, Doppler::A));
    assert_eq!(
        fake.ops(),
        vec![
            Op::Connect,
            Op::GetFreq,
            Op::GetSplit,
            Op::GetMode,
            Op::GetSplit,
            Op::GetRit,
            Op::GetFreq,
            Op::GetSplit,
            Op::GetFreq,
            Op::SetSplit(true),
            Op::SetFreq(F),
            Op::SetSplitFreq(F),
            Op::SetMode("USB".into()),
            Op::SetSplitMode("USB".into()),
        ]
    );
    assert_eq!(
        d.saved(),
        Some(Saved {
            mode: RigMode::Usb,
            split: RigSplit::SplitOff,
            rit: RigRit::RitOff,
            rx_hz: F,
            tx_hz: F
        })
    );
}

#[test]
fn leave_restores_then_simplex() {
    let (fake, mut rig) = rig();
    let mut d = DopplerSession::default();
    assert!(d.enter(&mut rig, Doppler::A));
    fake.with(|s| {
        s.freq = 144_300_312;
        s.split = true;
        s.split_freq = F;
    });
    fake.clear_log();
    d.leave(&mut rig);
    assert_eq!(
        fake.ops(),
        vec![
            Op::SetMode("USB".into()),
            Op::SetSplit(false),
            Op::SetRit(false),
            Op::SetFreq(F),
            Op::GetSplit,
            Op::SetFreq(F),
            Op::SetSplit(false),
            Op::SetFreq(F),
            Op::SetMode("USB".into()),
        ]
    );
}

#[test]
fn leave_twice_restores_once() {
    let (fake, mut rig) = rig();
    let mut d = DopplerSession::default();
    assert!(d.enter(&mut rig, Doppler::A));
    d.leave(&mut rig);
    fake.clear_log();
    d.leave(&mut rig);
    assert!(fake.ops().is_empty());
}

#[test]
fn leave_without_enter_sends_nothing() {
    let (fake, mut rig) = rig();
    DopplerSession::default().leave(&mut rig);
    assert!(fake.ops().is_empty());
}

#[test]
fn enter_with_failed_save_read_stores_nothing_and_retries() {
    let (fake, mut rig) = rig();
    let mut d = DopplerSession::default();
    // get_status reads the frequency; the save read after it times out
    fake.fail_after("GetFreq", 1, BackendError::Timeout);
    assert!(!d.enter(&mut rig, Doppler::A));
    assert_eq!(d.saved(), None);
    assert!(!fake
        .ops()
        .iter()
        .any(|o| matches!(o, Op::SetSplit(_) | Op::SetFreq(_) | Op::SetSplitFreq(_))));
    fake.with(|s| s.fail.clear());
    assert!(d.enter(&mut rig, Doppler::A));
    assert_eq!(d.saved().map(|v| v.rx_hz), Some(F));
}

#[test]
fn enter_refused_when_offline() {
    let (fake, mut rig) = rig();
    fake.fail("GetFreq", BackendError::Timeout);
    let mut d = DopplerSession::default();
    assert!(!d.enter(&mut rig, Doppler::A));
    assert_eq!(fake.ops(), vec![Op::Connect, Op::GetFreq]);
    assert_eq!(d.saved(), None);
}

#[test]
fn status_mapping() {
    let (fake, mut rig) = rig();
    fake.fail("Connect", BackendError::NoCat("x".into()));
    assert_eq!(rig.get_status(Doppler::B), RigStatus::NoCat);
    assert_eq!(rig.take_error(), Some("x".into()));
    assert_eq!(rig.take_error(), None);

    fake.fail("Connect", BackendError::NoPort("p".into()));
    assert_eq!(rig.get_status(Doppler::B), RigStatus::NoPort);
    assert_eq!(rig.take_error(), Some("p".into()));

    fake.with(|s| s.fail.clear());
    fake.fail("GetFreq", BackendError::Rejected(-5));
    assert_eq!(rig.get_status(Doppler::B), RigStatus::Offline);
    fake.fail("GetFreq", BackendError::Timeout);
    assert_eq!(rig.get_status(Doppler::B), RigStatus::Offline);
    fake.fail("GetFreq", BackendError::Lost("gone".into()));
    assert_eq!(rig.get_status(Doppler::B), RigStatus::None);
    assert_eq!(rig.take_error(), Some("gone".into()));
    fake.with(|s| s.fail.clear());
    assert_eq!(rig.get_status(Doppler::B), RigStatus::Online);
}

#[test]
fn strategy_a_needs_split() {
    let (fake, mut rig) = rig();
    fake.fail("GetSplit", BackendError::Rejected(-11));
    assert_eq!(rig.get_status(Doppler::A), RigStatus::NotSuitable);
    fake.clear_log();
    assert_eq!(rig.get_status(Doppler::B), RigStatus::Online);
    assert_eq!(fake.ops(), vec![Op::Connect, Op::GetFreq]);
}

#[test]
fn getters_swallow_errors() {
    let (fake, mut rig) = rig();
    fake.fail_all_gets();
    assert_eq!(rig.get_mode(), RigMode::None);
    assert_eq!(rig.get_split(), RigSplit::None);
    assert_eq!(rig.get_rit(), RigRit::None);
    assert_eq!(rig.get_rx_frequency(), 0);
    assert_eq!(rig.get_tx_frequency(), 0);
}

#[test]
fn tx_frequency_uses_split_freq_when_split_on() {
    let (fake, mut rig) = rig();
    fake.with(|s| {
        s.split = true;
        s.split_freq = 144_300_500;
    });
    assert_eq!(rig.get_tx_frequency(), 144_300_500);
}

#[test]
fn zero_frequency_never_written() {
    let (fake, mut rig) = rig();
    assert!(!rig.set_rx_frequency(0));
    assert!(!rig.set_tx_frequency(-1));
    assert!(!rig.set_simplex_mode(0, RigMode::Usb));
    assert!(!rig.set_split_mode(0, F, RigMode::None));
    assert!(!rig.set_split_mode(F, 0, RigMode::None));
    assert!(fake.ops().is_empty());
}

#[test]
fn set_mode_none_or_other_sends_nothing() {
    let (fake, mut rig) = rig();
    assert!(!rig.set_mode(RigMode::None));
    assert!(!rig.set_mode(RigMode::Other));
    assert!(!rig.set_split(RigSplit::None));
    assert!(!rig.set_rit(RigRit::None));
    assert!(fake.ops().is_empty());
}

#[test]
fn mode_mapping() {
    let pairs = [
        ("CW", RigMode::Cw),
        ("CWR", RigMode::CwR),
        ("LSB", RigMode::Lsb),
        ("USB", RigMode::Usb),
        ("PKTLSB", RigMode::Dig),
        ("PKTUSB", RigMode::DigR),
        ("AM", RigMode::Am),
        ("FM", RigMode::Fm),
    ];
    for (n, m) in pairs {
        assert_eq!(mode_from_hamlib(n), m);
        assert_eq!(mode_to_hamlib(m), Some(n));
    }
    for n in ["RTTY", "WFM", "", "usb"] {
        assert_eq!(mode_from_hamlib(n), RigMode::None);
    }
    assert_eq!(mode_to_hamlib(RigMode::None), None);
    assert_eq!(mode_to_hamlib(RigMode::Other), None);
}

#[test]
fn enums_match_csharp() {
    let s = [
        (RigStatus::None, "NONE", 0),
        (RigStatus::NoCat, "NOCAT", 1),
        (RigStatus::NoPort, "NOPORT", 2),
        (RigStatus::NoRig, "NORIG", 4),
        (RigStatus::NotSuitable, "NOTSUITABLE", 8),
        (RigStatus::Offline, "OFFLINE", 16),
        (RigStatus::Online, "ONLINE", 64),
        (RigStatus::Error, "ERROR", 128),
    ];
    for (e, n, v) in s {
        assert_eq!((e.name(), e.value()), (n, v));
    }
    let d = [
        Doppler::None,
        Doppler::A,
        Doppler::B,
        Doppler::C,
        Doppler::D,
    ];
    let vals: Vec<u8> = d.iter().map(|x| x.value()).collect();
    assert_eq!(vals, vec![0, 1, 2, 4, 8]);
    assert!(!Doppler::None.is_active() && Doppler::D.is_active());
}
