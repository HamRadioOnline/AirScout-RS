//! The CAT polling loop (AirScout.CAT `CATWorker.OnDoWork`).
//!
//! `CatWorker::pass` is one loop iteration without sleeping, so it is
//! testable; `run` adds the cancellable loop.
//!
//! Time bound of one pass: every command is bounded by the backend timeout,
//! and after the first timeout the backend closes the connection so later
//! commands fail at once. `Rigctld::connect` shares one deadline across all
//! resolved addresses, so a connect takes at most one timeout C (DNS
//! resolution is not bounded by it). A pass is bounded by C + R (connect,
//! one read timeout R). Only a pass that attempts the Doppler enter can take
//! 2C + 2R: a read timeout closes the connection, the enter's `get_status`
//! reconnects, and a second read times out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Doppler, DopplerSession, Rig, RigIdent, RigMode, RigRit, RigSplit, RigStatus};

/// Pause after "no rig" (CATWorker.cs:196).
pub const NO_RIG_WAIT: Duration = Duration::from_secs(10);

/// Longest sleep step, so cancel is seen quickly.
const STEP: Duration = Duration::from_millis(250);

/// What the UI wants the rig to do (`DopplerStrategy`, `RxFrequency`,
/// `TxFrequency` of the C# worker).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DopplerTarget {
    pub strategy: Doppler,
    pub rx_hz: i64,
    pub tx_hz: i64,
}

pub type DopplerCell = Arc<Mutex<DopplerTarget>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RigInfo {
    pub rx_hz: i64,
    pub tx_hz: i64,
    pub mode: RigMode,
    pub split: RigSplit,
    pub rit: RigRit,
    /// The rig was read while the Doppler drives it (entered in an earlier pass and not left),
    /// so `rx_hz` and `tx_hz` may be shifted. The C# guarantees the same by setting
    /// `TrackStatus` and `DopplerStrategy` together on the UI thread (`MapDlg.cs:8380-8405`);
    /// here the Track worker drives the target, so the app cannot rely on its TRK status.
    pub doppler_active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CatEvent {
    Text(String),
    Status(RigStatus),
    Ident(Option<RigIdent>),
    Info(RigInfo),
}

pub type RigFactory = Box<dyn FnMut() -> Option<Box<dyn Rig>> + Send>;

pub struct CatWorker {
    factory: RigFactory,
    update: Duration,
    rig: Option<Box<dyn Rig>>,
    session: DopplerSession,
    old_status: RigStatus,
    old_info: Option<RigInfo>,
    old_strategy: Doppler,
    /// The ident last sent; an engine that learns its ident at the first poll changes it.
    sent_ident: Option<RigIdent>,
}

impl CatWorker {
    pub fn new(factory: RigFactory, update: Duration) -> Self {
        Self {
            factory,
            update,
            rig: None,
            session: DopplerSession::default(),
            old_status: RigStatus::None,
            old_info: None,
            old_strategy: Doppler::None,
            sent_ident: None,
        }
    }

    /// One loop iteration: the events to report and the time to wait.
    pub fn pass(&mut self, t: DopplerTarget) -> (Vec<CatEvent>, Duration) {
        let mut ev = Vec::new();
        if self.rig.is_none() {
            match (self.factory)() {
                None => {
                    ev.push(CatEvent::Ident(None));
                    ev.push(CatEvent::Text(
                        "No rig selected or rig is not supported by any available CAT!".into(),
                    ));
                    ev.push(CatEvent::Status(RigStatus::NoRig));
                    return (ev, NO_RIG_WAIT);
                }
                Some(rig) => {
                    ev.push(CatEvent::Text("Opening CAT: Success!".into()));
                    let ident = rig.ident();
                    ev.push(CatEvent::Ident(Some(ident.clone())));
                    self.sent_ident = Some(ident);
                    self.rig = Some(rig);
                    // The saved Doppler state survives a new rig: the config, and so the
                    // physical rig, is the same for the whole worker. With a saved
                    // state `old_strategy` is kept too: an unchanged active target goes
                    // on ticking without a second enter (which would save the shifted
                    // frequencies), and a target `None` restores once the new rig is
                    // Online. Without one nothing needs restoring.
                    self.old_status = RigStatus::None;
                    self.old_info = None;
                    if self.session.saved().is_none() {
                        self.old_strategy = Doppler::None;
                    }
                }
            }
        }
        let Some(rig) = self.rig.as_mut() else {
            return (ev, self.update);
        };
        let rig: &mut dyn Rig = rig.as_mut();

        let st = rig.get_status(t.strategy);
        if st == RigStatus::None {
            ev.push(CatEvent::Text(
                "Error while polling CAT: Rig lost during polling!".into(),
            ));
            ev.push(CatEvent::Status(RigStatus::NoRig));
            // Port deviation: the saved Doppler state is kept (C# loses it with the
            // rig), so the leave after re-creation restores the original state.
            self.rig = None;
            return (ev, self.update);
        }
        // Taken every pass so a stale text cannot surface at a later change.
        let err = rig.take_error();
        if st != self.old_status {
            self.old_status = st;
            if let Some(e) = err {
                ev.push(CatEvent::Text(e));
            }
            ev.push(CatEvent::Status(st));
        }
        let ident = rig.ident();
        if self.sent_ident.as_ref() != Some(&ident) {
            self.sent_ident = Some(ident.clone());
            ev.push(CatEvent::Ident(Some(ident)));
        }

        if st == RigStatus::Online {
            let mode = rig.get_mode();
            let split = rig.get_split();
            let rit = rig.get_rit();
            let rx_hz = rig.get_rx_frequency();
            let tx_hz = rig.get_tx_frequency();
            let info = RigInfo {
                rx_hz,
                tx_hz,
                mode,
                split,
                rit,
                doppler_active: self.old_strategy.is_active(),
            };
            if self.old_info != Some(info) {
                self.old_info = Some(info);
                ev.push(CatEvent::Info(info));
            }
        }

        let s = t.strategy;
        if s != self.old_strategy {
            if s.is_active() {
                // Port deviation: C# enters while not online and marks the
                // strategy done anyway. Here the enter waits for Online and
                // is retried every pass until it works.
                if st == RigStatus::Online && self.session.enter(rig, s) {
                    self.old_strategy = s;
                }
            } else if st == RigStatus::Online {
                // Setters never reconnect and `leave` drops the saved state,
                // so the restore waits for a live connection and is retried
                // every pass (like the enter).
                self.session.leave(rig);
                self.old_strategy = Doppler::None;
            }
        }
        if st == RigStatus::Online
            && s.is_active()
            && self.old_strategy == s
            && t.rx_hz > 0
            && t.tx_hz > 0
        {
            rig.set_split_mode(t.rx_hz, t.tx_hz, RigMode::None);
        }
        (ev, self.update)
    }

    /// Restore the rig and report the end (`OnDoWork` after the loop).
    ///
    /// Port deviation: when the rig was lost and a saved Doppler state exists, the
    /// factory is called once to restore it. That is one extra connect at exit,
    /// bounded by the backend timeout; the app's `EXIT_WAIT` still caps its wait.
    /// Without a saved state the factory is not called.
    pub fn finish(&mut self) -> Vec<CatEvent> {
        if self.rig.is_none() && self.session.saved().is_some() {
            self.rig = (self.factory)();
        }
        if let Some(rig) = self.rig.as_mut() {
            // Best effort: reconnect first, setters never do.
            rig.get_status(Doppler::None);
            self.session.leave(rig.as_mut());
        }
        vec![CatEvent::Status(RigStatus::None)]
    }
}

/// Sleep `d` in steps of at most 250 ms; returns early when cancelled.
fn sleep_cancellable(d: Duration, cancel: &AtomicBool) {
    let end = Instant::now() + d;
    while !cancel.load(Ordering::Relaxed) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(STEP));
    }
}

/// The worker loop. Ends after `cancel` is set: within one pass plus 250 ms.
pub fn run(
    mut w: CatWorker,
    cell: DopplerCell,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(CatEvent),
) {
    emit(CatEvent::Text("CAT Interface started.".into()));
    while !cancel.load(Ordering::Relaxed) {
        let t = *cell.lock().unwrap_or_else(|p| p.into_inner());
        let (events, wait) = w.pass(t);
        for e in events {
            emit(e);
        }
        sleep_cancellable(wait, cancel);
    }
    for e in w.finish() {
        emit(e);
    }
}
