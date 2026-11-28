//! CAT rig control in the app: settings mapping, the Doppler push of a tracking step
//! (`MapDlg.cs:8380-8401`, `:8419-8449`), the status colours and texts (`SayCAT`, `:9262-9308`)
//! and the `cat` worker thread around [`as_cat::run`].

use crate::services::ServiceEvent;
use crate::tracking::{TrackStep, CONTROL, DARK_GRAY};
use as_aircraft::enums::TrackMode;
use as_cat::{
    CatConfig, CatWorker, Doppler, DopplerCell, DopplerTarget, RigInfo, RigRit, RigSplit, RigStatus,
};
use as_settings::{DopplerStrategy, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

/// How long `on_exit` waits for the `cat` thread. It covers a responsive rig's last pass plus
/// the restore (milliseconds). An unresponsive rig cannot be restored anyway, so waiting longer
/// for its timeouts would only delay the exit.
pub const EXIT_WAIT: Duration = Duration::from_secs(2);

pub use as_cat::from_settings::cat_config;

/// The worker's strategy of the settings' Doppler strategy.
pub fn doppler_of(d: DopplerStrategy) -> Doppler {
    match d {
        DopplerStrategy::None => Doppler::None,
        DopplerStrategy::A => Doppler::A,
        DopplerStrategy::B => Doppler::B,
        DopplerStrategy::C => Doppler::C,
        DopplerStrategy::D => Doppler::D,
    }
}

/// What one tracking step asks of the rig (`MapDlg.cs:8380-8401`, `:8419-8449`). TRACK keeps the
/// strategy on; SINGLE ends with STOPPED in the same step (`:8293-8297`), so the strategy is off
/// again afterwards. The frequencies change only while a strategy is set and both are non-zero.
pub fn apply_step(t: &mut DopplerTarget, step: &TrackStep, strategy: Doppler) {
    let Some(v) = &step.values else {
        t.strategy = Doppler::None;
        return;
    };
    match step.mode {
        TrackMode::Track | TrackMode::Single => {
            t.strategy = strategy;
            if strategy.is_active() && v.rx_hz != 0 && v.tx_hz != 0 {
                t.rx_hz = v.rx_hz;
                t.tx_hz = v.tx_hz;
            }
            if step.mode == TrackMode::Single {
                t.strategy = Doppler::None;
            }
        }
        TrackMode::None | TrackMode::Error => t.strategy = Doppler::None,
    }
}

/// `SayCAT("CAT", fore, back)` colours (`MapDlg.cs:9262-9282`) as RGB: `(text, background)`.
pub fn cat_colors(s: RigStatus) -> ([u8; 3], [u8; 3]) {
    match s {
        RigStatus::Online => ([255, 255, 255], [0, 100, 0]),
        RigStatus::Error
        | RigStatus::NoCat
        | RigStatus::NoPort
        | RigStatus::NoRig
        | RigStatus::NotSuitable => ([255, 255, 0], [255, 0, 0]),
        RigStatus::Offline => ([255, 255, 255], [255, 140, 0]),
        RigStatus::None => (DARK_GRAY, CONTROL),
    }
}

/// The status line text of a rig report (`MapDlg.cs:9305-9308`). The frequencies print as plain
/// digits, as `long.ToString(info)` does.
pub fn rig_reports(i: &RigInfo) -> String {
    let on_off = |b: bool| if b { "ON" } else { "OFF" };
    format!(
        "Rig reports RX: {}Hz, TX: {}Hz, Mode: {}, RIT: {}, Split: {}",
        i.rx_hz,
        i.tx_hz,
        i.mode.name(),
        on_off(i.rit == RigRit::RitOn),
        on_off(i.split == RigSplit::SplitOn)
    )
}

/// Thread `cat` running [`as_cat::run`]; its events go out as [`ServiceEvent::Cat`].
pub struct CatService {
    cancel: Arc<AtomicBool>,
    /// Disconnected once the thread has ended (its sender is dropped then).
    done: Receiver<()>,
}

impl CatService {
    /// Spawns the thread; it reads the target from `cell` once per pass.
    pub fn start(
        cfg: CatConfig,
        cell: DopplerCell,
        tx: Sender<ServiceEvent>,
        generation: u64,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (done_tx, done) = channel::<()>();
        let flag = cancel.clone();
        let err_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("cat".into())
            .spawn(move || {
                let _done = done_tx;
                let update = cfg.update;
                let worker = CatWorker::new(as_cat::factory(cfg), update);
                as_cat::run(worker, cell, &flag, &mut |event| {
                    let _ = tx.send(ServiceEvent::Cat { generation, event });
                });
            });
        if let Err(e) = spawned {
            cancel.store(true, Ordering::SeqCst);
            let _ = err_tx.send(ServiceEvent::Error(format!(
                "CAT: cannot start the worker thread: {e}"
            )));
        }
        Self { cancel, done }
    }

    /// Cancels and returns at once; idempotent. The thread restores the rig before it ends.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Waits at most `max` for the thread to end; true once it has.
    pub fn wait(&self, max: Duration) -> bool {
        matches!(
            self.done.recv_timeout(max),
            Err(RecvTimeoutError::Disconnected)
        )
    }
}

impl Drop for CatService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The CAT part of the real services: the running [`CatService`] and the cell the Track
/// worker's steps write into.
#[derive(Default)]
pub struct CatIntegration {
    service: Option<CatService>,
    doppler: DopplerCell,
}

impl CatIntegration {
    /// Stops a running worker, then starts a new one if `cat.activate`, with a fresh cell.
    pub fn start(&mut self, s: &Settings, tx: &Sender<ServiceEvent>, generation: u64) {
        self.stop();
        self.doppler = DopplerCell::default();
        if s.cat.activate {
            self.service = Some(CatService::start(
                cat_config(s),
                self.doppler.clone(),
                tx.clone(),
                generation,
            ));
        }
    }

    /// Stops the worker; returns at once.
    pub fn stop(&mut self) {
        drop(self.take_stopped());
    }

    /// Stops the worker and hands it over, so the caller can wait for its end.
    pub fn take_stopped(&mut self) -> Option<CatService> {
        let mut c = self.service.take()?;
        c.stop();
        Some(c)
    }

    /// The cell of the current start, for the Track worker's Doppler push.
    pub fn cell(&self) -> DopplerCell {
        self.doppler.clone()
    }
}
