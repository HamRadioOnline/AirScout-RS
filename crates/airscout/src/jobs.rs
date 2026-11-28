//! Background jobs of the analysis (history) side: one thread per job, events tagged with the
//! job id so the app can drop those of a superseded job.

use crate::crossing_history::CrossingResult;
use crate::history::HistoryData;
use crate::maintenance::DbId;
use crate::services::ServiceEvent;
use as_geo::latlon::{GPoint, GRect};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// What a job reports; it arrives as [`ServiceEvent::Job`].
#[derive(Debug, Clone)]
pub enum JobEvent {
    Progress(String),
    Done(String),
    Failed(String),
    HistoryLoaded(Box<HistoryData>),
    /// The hexes and calls of the Plane History dialog (already filtered and sorted).
    PlaneLists {
        hexes: Vec<String>,
        calls: Vec<String>,
    },
    /// The crossings of the Crossing History dialog and their chart series.
    Crossings(Box<CrossingResult>),
    /// A batch of routes of the Traffic dialog (thinning is the dialog's business).
    Routes(Vec<Vec<GPoint>>),
    /// File path and size [MB] of every database (maintenance).
    Sizes(Vec<(DbId, String, f64)>),
    /// Name, description and row count of the tables of a database.
    Tables(DbId, Vec<(String, String, u64)>),
    /// The first rows of a table (read-only view).
    View {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// Call and locator of the stations that match the Delete Single Station call field.
    Locations(Vec<(String, String)>),
    /// Tiles of the server's catalogue (6-character indexes) that the elevation database has,
    /// and those it has not (Options Elevation tab coverage map).
    Coverage {
        found: Vec<String>,
        missing: Vec<String>,
    },
    /// The cells of the station map's elevation grid with their elevation [m].
    ElevationGrid(Vec<(GRect, i16)>),
}

/// What a running job sees: its id, the cancel flag and the event channel.
pub struct JobCtx {
    pub id: u64,
    pub cancel: Arc<AtomicBool>,
    pub tx: Sender<ServiceEvent>,
}

impl JobCtx {
    pub fn new(id: u64, cancel: Arc<AtomicBool>, tx: Sender<ServiceEvent>) -> Self {
        Self { id, cancel, tx }
    }

    pub fn send(&self, event: JobEvent) {
        let _ = self.tx.send(ServiceEvent::Job { id: self.id, event });
    }

    pub fn progress(&self, text: impl Into<String>) {
        self.send(JobEvent::Progress(text.into()));
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Starts job threads.
pub struct JobRunner;

impl JobRunner {
    /// Run `f` on a new thread named `name`. A failure to start the thread, or a panic in `f`,
    /// is reported as [`JobEvent::Failed`].
    pub fn spawn(
        name: &str,
        id: u64,
        cancel: Arc<AtomicBool>,
        tx: Sender<ServiceEvent>,
        f: impl FnOnce(&JobCtx) + Send + 'static,
    ) {
        let ctx = JobCtx::new(id, cancel, tx.clone());
        let r = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                if std::panic::catch_unwind(AssertUnwindSafe(|| f(&ctx))).is_err() {
                    ctx.send(JobEvent::Failed("job stopped unexpectedly".into()));
                }
            });
        if let Err(e) = r {
            let _ = tx.send(ServiceEvent::Job {
                id,
                event: JobEvent::Failed(format!("cannot start job: {e}")),
            });
        }
    }
}
