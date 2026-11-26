//! The feed worker thread (`PlaneFeed.OnDoWork`).
//!
//! Loop: check the area, `start`, then repeatedly `get_planes` → [`pipeline::process`] →
//! cache, aircraft database, position history (if `keep_history`) → [`FeedEvent::Planes`],
//! and sleep `interval_s` in 1 s steps so that a stop request is seen within a second.
//! A failing `get_planes` sends [`FeedEvent::Error`] and the loop goes on after the interval.

use crate::pipeline::{self, PipelineConfig};
use crate::{FeedArgs, PlaneFeed};
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::PositionDb;
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Worker settings.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerConfig {
    /// `Planes_Interval` [s].
    pub interval_s: u64,
    pub pipeline: PipelineConfig,
}

/// What the worker reports (`ReportProgress`).
#[derive(Debug, Clone)]
pub enum FeedEvent {
    Started,
    /// The planes that passed all checks in one cycle.
    Planes(Vec<PlaneInfo>),
    /// A user-readable error; never contains credentials.
    Error(String),
    Stopped,
}

/// Handle of a running worker.
pub struct FeedHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FeedHandle {
    /// Ask the worker to stop (`CancelAsync`). Returns at once.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Wait for the worker thread to end.
    pub fn join(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for FeedHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// The feed worker.
pub struct FeedWorker;

struct Ctx {
    args: FeedArgs,
    cfg: WorkerConfig,
    cache: Arc<PlaneInfoCache>,
    db: Arc<AircraftDb>,
    positions: Option<Arc<PositionDb>>,
    events: Sender<FeedEvent>,
    stop: Arc<AtomicBool>,
}

impl Ctx {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn send(&self, e: FeedEvent) {
        // the receiver may be gone; the worker then just runs until stopped
        let _ = self.events.send(e);
    }

    fn sleep_interval(&self) {
        let mut i = 0;
        while !self.stopping() && i < self.cfg.interval_s {
            std::thread::sleep(Duration::from_secs(1));
            i += 1;
        }
    }

    fn cycle(&self, feed: &mut dyn PlaneFeed) {
        let raw = match feed.get_planes(&self.args) {
            Ok(r) => r,
            Err(e) => {
                self.send(FeedEvent::Error(format!("Plane Feed Exception: {e}")));
                return;
            }
        };
        let p = pipeline::process(raw, &self.args, &self.cfg.pipeline, &self.cache, &self.db);
        self.cache.bulk_insert_or_update_if_newer(&p.planes);
        if let Err(e) = self.db.plane_info_bulk_upsert(&p.planes) {
            self.send(FeedEvent::Error(format!("Aircraft database: {e}")));
        }
        if self.args.keep_history {
            if let Some(pos) = &self.positions {
                if let Err(e) = pos.bulk_insert_plane_infos(&p.planes, self.db.as_ref()) {
                    self.send(FeedEvent::Error(format!("Position database: {e}")));
                }
            }
        }
        self.send(FeedEvent::Planes(p.planes));
    }

    fn run(&self, mut feed: Box<dyn PlaneFeed>) {
        let a = &self.args;
        if a.max_lat <= a.min_lat || a.max_lon <= a.min_lon {
            self.send(FeedEvent::Error(
                "Area boundaries mismatch. Check your Covered Area parameters!".into(),
            ));
            return;
        }
        while !self.stopping() {
            if let Err(e) = feed.start(&self.args) {
                // C# retries Start at once in a tight loop (#49); wait one interval instead
                self.send(FeedEvent::Error(format!("Plane Feed Exception: {e}")));
                self.sleep_interval();
                continue;
            }
            loop {
                self.cycle(feed.as_mut());
                self.sleep_interval();
                if self.stopping() {
                    break;
                }
            }
            if let Err(e) = feed.stop(&self.args) {
                self.send(FeedEvent::Error(format!("Plane Feed Exception: {e}")));
            }
        }
    }
}

impl FeedWorker {
    /// Start the worker thread. It sends [`FeedEvent::Started`] first and
    /// [`FeedEvent::Stopped`] last.
    pub fn spawn(
        feed: Box<dyn PlaneFeed>,
        args: FeedArgs,
        cfg: WorkerConfig,
        cache: Arc<PlaneInfoCache>,
        db: Arc<AircraftDb>,
        positions: Option<Arc<PositionDb>>,
        events: Sender<FeedEvent>,
    ) -> FeedHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let ctx = Ctx {
            args,
            cfg,
            cache,
            db,
            positions,
            events,
            stop: stop.clone(),
        };
        let thread = std::thread::Builder::new()
            .name("plane-feed".into())
            .spawn(move || {
                ctx.send(FeedEvent::Started);
                ctx.run(feed);
                ctx.send(FeedEvent::Stopped);
            })
            .ok();
        FeedHandle { stop, thread }
    }
}
