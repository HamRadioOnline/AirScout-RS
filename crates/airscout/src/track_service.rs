//! The Track worker (`bw_Track`, `MapDlg.cs:7997-8315`): steps once a second, fans the step out
//! to the outputs, counts errors, reports the ROT state and pushes the Doppler target of the CAT
//! worker (`:8380-8401`).

use crate::cat_service::{apply_step, doppler_of};
use crate::services::ServiceEvent;
use crate::track_outputs::{build_outputs, OutputError, OutputGroup, TrackConfig, TrackOutput};
use crate::tracker::{TrackInputCell, TrackShown, TrackShownCell, Tracker};
use crate::tracking::{RotStatus, TrackStep};
use as_aircraft::enums::TrackMode;
use as_aircraft::PlaneInfoCache;
use as_cat::DopplerCell;
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

/// `maxerr`: an output is switched off by the error after this many (the 11th).
pub const MAX_ERRORS: u32 = 10;

#[derive(Clone, Debug, PartialEq)]
pub enum TrackEvent {
    Rot(RotStatus),
    /// Text for the status line.
    Text(String),
    /// The group has been switched off after too many errors.
    Disabled(OutputGroup),
}

fn group_name(g: OutputGroup) -> &'static str {
    match g {
        OutputGroup::Dde => "DDE",
        OutputGroup::Udp => "UDP",
        OutputGroup::Serial => "Serial",
        OutputGroup::File => "file",
    }
}

struct Slot {
    out: Box<dyn TrackOutput>,
    disabled: bool,
}

/// The step logic without a thread.
pub struct TrackFanout {
    slots: Vec<Slot>,
    /// Counted errors per group; reset on a successful step (#203), so only a sustained run of
    /// failures switches the output off.
    errors: BTreeMap<OutputGroup, u32>,
    rot: RotStatus,
}

impl TrackFanout {
    pub fn new(outputs: Vec<Box<dyn TrackOutput>>) -> Self {
        Self {
            slots: outputs
                .into_iter()
                .map(|out| Slot {
                    out,
                    disabled: false,
                })
                .collect(),
            errors: BTreeMap::new(),
            rot: RotStatus::Stopped,
        }
    }

    fn set_rot(&mut self, r: RotStatus, ev: &mut Vec<TrackEvent>) {
        if self.rot != r {
            self.rot = r;
            ev.push(TrackEvent::Rot(r));
        }
    }

    pub fn step(&mut self, s: &TrackStep) -> Vec<TrackEvent> {
        let mut ev = Vec::new();
        let Some(v) = &s.values else {
            self.set_rot(RotStatus::Stopped, &mut ev);
            self.idle();
            return ev;
        };
        for i in 0..self.slots.len() {
            if self.slots[i].disabled {
                continue;
            }
            self.set_rot(RotStatus::Tracking, &mut ev);
            let slot = &mut self.slots[i];
            if !slot.out.is_connected() {
                // an error leaves it unconnected; the send then reports it
                let _ = slot.out.connect();
            }
            match slot.out.send(v.my_azimuth, v.my_elevation) {
                Ok(()) => {
                    // #203: a successful step resets the group's error counter.
                    self.errors.remove(&slot.out.group());
                }
                Err(OutputError::Failed(t)) => {
                    // an open failure (`"Track error: …"`, #238) or another uncounted error:
                    // shown, not counted; the output reconnects on the next step.
                    ev.push(TrackEvent::Text(t));
                    slot.out.close();
                }
                Err(OutputError::SerialIo(t)) => {
                    // #239: a serial timeout or I/O error is shown, closes to reconnect, and
                    // counts toward the same switch-off counter as a format error.
                    ev.push(TrackEvent::Text(t));
                    slot.out.close();
                    let g = slot.out.group();
                    let n = self.errors.entry(g).or_insert(0);
                    *n += 1;
                    if *n > MAX_ERRORS {
                        slot.disabled = true;
                        ev.push(TrackEvent::Text(format!(
                            "Tracking via {} disabled.",
                            group_name(g)
                        )));
                        self.set_rot(RotStatus::Error, &mut ev);
                        ev.push(TrackEvent::Disabled(g));
                    }
                }
                Err(OutputError::NotConnected(t)) => {
                    ev.push(TrackEvent::Text(t));
                    let g = slot.out.group();
                    let n = self.errors.entry(g).or_insert(0);
                    *n += 1;
                    if *n > MAX_ERRORS {
                        slot.out.close();
                        slot.disabled = true;
                        ev.push(TrackEvent::Text(format!(
                            "Tracking via {} disabled.",
                            group_name(g)
                        )));
                        self.set_rot(RotStatus::Error, &mut ev);
                        ev.push(TrackEvent::Disabled(g));
                    }
                }
            }
        }
        if s.mode == TrackMode::Single {
            self.set_rot(RotStatus::Stopped, &mut ev);
        }
        ev
    }

    /// Calls `idle` on every output that is not switched off.
    pub fn idle(&mut self) {
        for s in self.slots.iter_mut().filter(|s| !s.disabled) {
            s.out.idle();
        }
    }

    pub fn close(&mut self) {
        for s in &mut self.slots {
            s.out.close();
        }
    }
}

/// One tracking step per second (`Thread.Sleep(1000)`, `MapDlg.cs:8313`). The C# loop sleeps a
/// fixed 1000 ms between two steps and `Track_Update` applies only to its outer loop, so the port
/// does not read `Track.update_ms`.
pub const TRACK_STEP: Duration = Duration::from_secs(1);

/// How often the thread looks at the cancel flag between two steps.
const CANCEL_POLL: Duration = Duration::from_millis(250);

/// What the Track worker reads and writes besides its outputs.
pub struct TrackCtx {
    /// The UI's current input; copied at every step.
    pub input: TrackInputCell,
    pub cache: Arc<PlaneInfoCache>,
    /// The CAT worker's Doppler target; each step pushes into it.
    pub doppler: DopplerCell,
    pub tx: Sender<ServiceEvent>,
    pub generation: u64,
}

/// Thread `track`: owns the step clock and the [`Tracker`] (`oldvalues`, a local of the C#
/// `bw_Track`, `MapDlg.cs:7997`), steps the input through a [`TrackFanout`] and pushes the
/// Doppler target, also while the window is minimised.
pub struct TrackService {
    cancel: Arc<AtomicBool>,
    shown: TrackShownCell,
}

impl TrackService {
    /// Spawns the thread. It sends `Rot(Stopped)` first (`:8050-8051`), steps at once and then
    /// every [`TRACK_STEP`] on a deadline (a late step does not cause a burst), and sees `stop`
    /// within 250 ms.
    pub fn start(cfg: TrackConfig, ctx: TrackCtx) -> Self {
        Self::start_with(move || build_outputs(&cfg), ctx)
    }

    /// As [`start`](Self::start), with the outputs made by `make`. `make` runs on the new thread,
    /// so thread-bound outputs (DDE) are created, used and dropped on that one thread.
    pub fn start_with<F>(make: F, ctx: TrackCtx) -> Self
    where
        F: FnOnce() -> Vec<Box<dyn TrackOutput>> + Send + 'static,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        let shown: TrackShownCell = Arc::default();
        let flag = cancel.clone();
        let out = shown.clone();
        let err_tx = ctx.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("track".into())
            .spawn(move || run(make, ctx, &flag, &out));
        if let Err(e) = spawned {
            cancel.store(true, Ordering::SeqCst);
            let _ = err_tx.send(ServiceEvent::Error(format!(
                "Track: cannot start the worker thread: {e}"
            )));
        }
        Self { cancel, shown }
    }

    /// What the last step showed; the default before the first one.
    pub fn shown(&self) -> TrackShown {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Cancels and returns at once; idempotent.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// The body of thread `track`.
fn run(
    make: impl FnOnce() -> Vec<Box<dyn TrackOutput>>,
    ctx: TrackCtx,
    cancel: &AtomicBool,
    shown: &TrackShownCell,
) {
    let TrackCtx {
        input,
        cache,
        doppler,
        tx,
        generation,
    } = ctx;
    let send = |event| {
        let _ = tx.send(ServiceEvent::Track { generation, event });
    };
    let mut fan = TrackFanout::new(make());
    let mut tracker = Tracker::default();
    send(TrackEvent::Rot(RotStatus::Stopped));
    let mut next = Instant::now();
    while !cancel.load(Ordering::SeqCst) {
        // every pass, at least every `CANCEL_POLL`, also without a step (`TrackOutput::idle`)
        fan.idle();
        let now = Instant::now();
        if now < next {
            std::thread::sleep((next - now).min(CANCEL_POLL));
            continue;
        }
        let i = input.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if let Some((step, sh)) = tracker.step(&i, Utc::now(), &cache) {
            // Doppler first, `shown` after: `shown` is what readers (and tests) wait on, so
            // once they see the new status the doppler state must already match it.
            if let Some(p) = &i.params {
                let mut t = doppler.lock().unwrap_or_else(PoisonError::into_inner);
                apply_step(&mut t, &step, doppler_of(p.strategy));
            }
            *shown.lock().unwrap_or_else(PoisonError::into_inner) = sh;
            fan.step(&step).into_iter().for_each(send);
        }
        // on a deadline: no drift; a step that ran late is not caught up
        next += TRACK_STEP;
        let after = Instant::now();
        if next <= after {
            next = after + TRACK_STEP;
        }
    }
    fan.close();
}

impl Drop for TrackService {
    fn drop(&mut self) {
        self.stop();
    }
}
