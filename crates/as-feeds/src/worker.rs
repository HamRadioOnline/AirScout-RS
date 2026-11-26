//! The feed worker thread (`PlaneFeed.OnDoWork`).
//!
//! Loop: check the area, `start`, then repeatedly `get_planes` → [`pipeline::process`] →
//! cache, aircraft database, position history (if `keep_history`) → [`FeedEvent::Planes`],
//! and sleep `interval_s` in 1 s steps so that a stop request is seen within a second.
//! A failing `get_planes` sends [`FeedEvent::Error`] and the loop goes on after the interval.

use crate::pipeline::{self, PipelineConfig};
use crate::{FeedArgs, FeedPlane, PlaneFeed};
use as_aircraft::check;
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::PositionDb;
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Worker settings.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerConfig {
    /// `Planes_Interval` [s].
    pub interval_s: u64,
    pub pipeline: PipelineConfig,
}

/// Test-only synchronization points for deterministic multi-worker interleaving (#50). Each
/// [`SyncPoint`] is a one-shot, timeout-bounded rendezvous between the worker and the test. Every
/// field is `None` in production; [`FeedWorker::spawn_with_sync`] wires them into the worker
/// thread.
#[doc(hidden)]
#[derive(Default)]
pub struct WorkerSync {
    /// Pause after candidates are computed and sorted, before atomic admission.
    pub at_admission: Option<Arc<SyncPoint>>,
    /// Pause after the live event is emitted, before the aircraft/history DB writes.
    pub at_db_write: Option<Arc<SyncPoint>>,
}

/// A one-shot, timeout-bounded rendezvous between a worker and a test (#50). The worker calls
/// [`pause`](Self::pause) to signal its arrival and then wait (bounded) for a release; the test
/// calls [`wait_arrived`](Self::wait_arrived) and then [`release`](Self::release). The handshake
/// uses `mpsc` channels with [`recv_timeout`](Receiver::recv_timeout), so a missing or
/// already-consumed release never blocks `stop()`/`join()` indefinitely.
#[doc(hidden)]
pub struct SyncPoint {
    arrived: SyncSender<()>,
    arrived_rx: Mutex<Receiver<()>>,
    proceed: SyncSender<()>,
    proceed_rx: Mutex<Receiver<()>>,
}

impl SyncPoint {
    /// Bound on the worker's wait for a release; keeps `pause` from blocking `stop`/`join`.
    pub const TIMEOUT: Duration = Duration::from_secs(2);

    /// Bound on the test's wait for the worker to arrive. Generous because the worker may first
    /// sleep a full interval (e.g. while a pending report waits out the admission skew).
    const ARRIVAL_TIMEOUT: Duration = Duration::from_secs(10);

    /// A fresh rendezvous shared by a worker and its test via [`Arc`]. Both directions use a
    /// one-slot bounded channel so that the signalling side (the worker's arrival or the test's
    /// release) never blocks, even when the other side is absent or has already finished. The
    /// receiving halves are wrapped in a [`Mutex`] so [`SyncPoint`] is `Send + Sync` (a bare
    /// [`Receiver`] is not `Sync`) and can travel inside the worker's [`Arc`].
    pub fn new() -> Arc<Self> {
        let (arrived, arrived_rx) = sync_channel(1);
        let (proceed, proceed_rx) = sync_channel(1);
        Arc::new(Self {
            arrived,
            arrived_rx: Mutex::new(arrived_rx),
            proceed,
            proceed_rx: Mutex::new(proceed_rx),
        })
    }

    /// Worker side: signal arrival, then wait (bounded) for the release.
    pub(crate) fn pause(&self) {
        let _ = self.arrived.send(());
        let _ = self
            .proceed_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(Self::TIMEOUT);
    }

    /// Test side: wait (bounded) for the worker to reach this point.
    pub fn wait_arrived(&self) {
        self.arrived_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(Self::ARRIVAL_TIMEOUT)
            .expect("worker reached the sync point");
    }

    /// Test side: release a worker paused at this point.
    pub fn release(&self) {
        let _ = self.proceed.send(());
    }
}

/// What the worker reports (`ReportProgress`).
#[derive(Debug, Clone)]
pub enum FeedEvent {
    Started,
    /// The planes that passed all checks in one cycle.
    Planes(Vec<PlaneInfo>),
    /// Number of records the adapter could not decode or map in a successful response.
    RejectedRecords(usize),
    /// Number of pending observations dropped by the residence or capacity limits (#376).
    DroppedRecords(usize),
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

/// Positive source-clock skew admitted as eligible beyond receipt time (#365/#376): a report is
/// eligible while its UTC source time is at most this far ahead of the receipt instant.
const ADMISSION_SKEW: ChronoDuration = ChronoDuration::seconds(10);
/// Pending future reports expire after this much monotonic residence (#376).
const PENDING_RESIDENCE: Duration = Duration::from_secs(10 * 60);
/// At most this many distinct pending entries per worker (#376).
const PENDING_CAP: usize = 8192;

/// A future report held until it becomes eligible, together with its monotonic arrival time
/// (used for the residence limit, independent of any wall-clock rollback).
struct PendingEntry {
    plane: FeedPlane,
    received_at: Instant,
}

/// Normalized hex key for coalescing and the capacity tie-break: quote-stripped, uppercased and
/// trimmed, like `PlaneInfoChecker` normalization.
fn pending_hex(plane: &FeedPlane) -> String {
    plane.hex.replace('"', "").to_uppercase().trim().to_string()
}

/// Stateless per-report validity for pending admission (#376), run before coalescing so a bad
/// report can neither shadow a valid report at the same `(hex, time)` nor consume `PENDING_CAP`.
/// Ground reports, hex that is invalid and not recoverable from the registration, non-finite or
/// out-of-area coordinates, invalid track, and (for component-timed reports that never borrow
/// from the cache) out-of-range alt/speed are dropped. A recovered hex is stored back on the
/// returned plane so coalescing keys on it.
fn pending_valid(plane: &FeedPlane, args: &FeedArgs, db: &AircraftDb) -> Option<FeedPlane> {
    if plane.ground {
        return None;
    }
    let mut p = plane.clone();
    if !check::hex(&p.hex) {
        if !check::reg(&p.reg, db) {
            return None;
        }
        p.hex = db.aircraft_find_by_reg(&p.reg).ok().flatten()?.hex;
    }
    if !check::lat(p.lat) || !check::lon(p.lon) {
        return None;
    }
    if p.lat < args.min_lat || p.lat > args.max_lat || p.lon < args.min_lon || p.lon > args.max_lon
    {
        return None;
    }
    if !check::track(p.track) {
        return None;
    }
    // A component-timed report never borrows motion or height from the cache, so its alt and
    // speed must already be in range; otherwise the pipeline may recover them from the cache.
    if p.components_valid_until.is_some() && (!check::alt(p.alt_ft) || !check::speed(p.speed_kts)) {
        return None;
    }
    Some(p)
}

struct Ctx {
    args: FeedArgs,
    cfg: WorkerConfig,
    cache: Arc<PlaneInfoCache>,
    db: Arc<AircraftDb>,
    positions: Option<Arc<PositionDb>>,
    events: Sender<FeedEvent>,
    stop: Arc<AtomicBool>,
    pending: Vec<PendingEntry>,
    sync: WorkerSync,
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
        // A zero setting would loop without pause and hammer the feed (#49); floor it at 1 s
        // without rewriting the stored TOML.
        let interval = self.cfg.interval_s.max(1);
        let mut i = 0;
        while !self.stopping() && i < interval {
            std::thread::sleep(Duration::from_secs(1));
            i += 1;
        }
    }

    fn cycle(&mut self, feed: &mut dyn PlaneFeed) {
        let (raw, successful) = match feed.get_batch(&self.args) {
            Ok(batch) => {
                if batch.rejected != 0 {
                    self.send(FeedEvent::RejectedRecords(batch.rejected));
                }
                (batch.planes, true)
            }
            Err(e) => {
                self.send(FeedEvent::Error(format!("Plane Feed Exception: {e}")));
                (Vec::new(), false)
            }
        };
        let receipt_time = Utc::now();

        // Partition the fresh reports and hold the future ones back (#365/#376).
        let (eligible, future) = split_future_reports(raw, receipt_time);
        self.push_pending(future);
        // Promote pending reports that became eligible; drain before expiry and trimming (#376).
        let promoted = self.drain_pending(receipt_time);

        // Validate every candidate against the accepted live state (the cache as of the previous
        // cycle), then merge normal and promoted candidates in timestamp order per hex.
        let mut candidates = pipeline::process(
            eligible,
            &self.args,
            &self.cfg.pipeline,
            &self.cache,
            &self.db,
        )
        .planes;
        candidates.extend(
            pipeline::process(
                promoted,
                &self.args,
                &self.cfg.pipeline,
                &self.cache,
                &self.db,
            )
            .planes,
        );
        candidates.sort_by(|a, b| a.hex.cmp(&b.hex).then_with(|| a.time.cmp(&b.time)));

        // Atomic admission returns the actual winners: a strictly newer sample supersedes an
        // older one and the first of two equal-time samples wins (#50). Live events and metadata
        // use winners only; otherwise valid older reports stay eligible for history (#28). Each
        // candidate is re-validated against the accepted live state under the cache lock, and the
        // live event is enqueued under that same lock before any slower DB write, so admission
        // order and live-event order match across workers.
        // Test-only admission hook (#50): one-shot (`take`) and gated on non-empty work, so a
        // cycle with no candidates passes through and a later cycle can never re-arm it.
        if !candidates.is_empty() {
            if let Some(point) = self.sync.at_admission.take() {
                point.pause();
            }
        }
        let cfg = &self.cfg.pipeline;
        let (winners, history_eligible) = self.cache.bulk_insert_validated(
            &candidates,
            |plane, current| pipeline::plausibility_pass(plane, current, cfg).is_ok(),
            |winners| {
                if successful || !winners.is_empty() {
                    self.send(FeedEvent::Planes(winners.to_vec()));
                }
            },
        );

        if !winners.is_empty() || !history_eligible.is_empty() {
            if let Some(point) = self.sync.at_db_write.take() {
                point.pause();
            }
        }
        if let Err(e) = self.db.plane_info_bulk_upsert(&winners) {
            self.send(FeedEvent::Error(format!("Aircraft database: {e}")));
        }
        if self.args.keep_history {
            if let Some(pos) = &self.positions {
                // Only candidates that passed the atomic re-validation reach history; a report
                // rejected by plausibility under the lock is never persisted (#50 round 2).
                if let Err(e) = pos.bulk_insert_plane_infos(&history_eligible, self.db.as_ref()) {
                    self.send(FeedEvent::Error(format!("Position database: {e}")));
                }
            }
        }
    }

    /// Append future reports to the pending queue, dropping reports that fail the stateless
    /// validity checks (#376) and coalescing equal normalized-hex/UTC-timestamp entries so the
    /// first received wins and a repeating feed cannot grow the queue.
    fn push_pending(&mut self, future: Vec<FeedPlane>) {
        let valid: Vec<FeedPlane> = future
            .into_iter()
            .filter_map(|plane| pending_valid(&plane, &self.args, &self.db))
            .collect();
        coalesce_into(&mut self.pending, valid, Instant::now());
    }

    /// Move the now-eligible pending reports out, dropping component-expired (#366), residence-
    /// expired and, if over capacity, the farthest source timestamps (#376). Reports sanitized
    /// drop counts.
    fn drain_pending(&mut self, receipt_time: DateTime<Utc>) -> Vec<FeedPlane> {
        let (promoted, kept, dropped) = drain_pending(
            std::mem::take(&mut self.pending),
            receipt_time,
            Instant::now(),
        );
        self.pending = kept;
        if dropped != 0 {
            self.send(FeedEvent::DroppedRecords(dropped));
        }
        promoted
    }

    fn run(&mut self, mut feed: Box<dyn PlaneFeed>) {
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
        Self::spawn_with_sync(
            feed,
            args,
            cfg,
            cache,
            db,
            positions,
            events,
            WorkerSync::default(),
        )
    }

    /// [`spawn`] with test-only synchronization points (see [`WorkerSync`]).
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_with_sync(
        feed: Box<dyn PlaneFeed>,
        args: FeedArgs,
        cfg: WorkerConfig,
        cache: Arc<PlaneInfoCache>,
        db: Arc<AircraftDb>,
        positions: Option<Arc<PositionDb>>,
        events: Sender<FeedEvent>,
        sync: WorkerSync,
    ) -> FeedHandle {
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(geometric_only) = feed.component_feed_geometric_only() {
            cache.set_component_feed_geometric_only(geometric_only);
        }
        let ctx = Ctx {
            args,
            cfg,
            cache,
            db,
            positions,
            events,
            stop: stop.clone(),
            pending: Vec::new(),
            sync,
        };
        let thread = std::thread::Builder::new()
            .name("plane-feed".into())
            .spawn(move || {
                ctx.send(FeedEvent::Started);
                let mut ctx = ctx;
                ctx.run(feed);
                ctx.send(FeedEvent::Stopped);
            })
            .ok();
        FeedHandle { stop, thread }
    }
}

fn split_future_reports(
    reports: Vec<FeedPlane>,
    receipt_time: DateTime<Utc>,
) -> (Vec<FeedPlane>, Vec<FeedPlane>) {
    let cutoff = receipt_time
        .checked_add_signed(ADMISSION_SKEW)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    let mut eligible = Vec::new();
    let mut future = Vec::new();
    for report in reports {
        if report
            .components_valid_until
            .is_some_and(|expiry| receipt_time > expiry)
        {
            continue;
        }
        // a report is eligible through receipt + 10 s inclusive (#365/#376)
        if report.time.is_none_or(|time| time <= cutoff) {
            eligible.push(report);
        } else {
            future.push(report);
        }
    }
    (eligible, future)
}

/// Append `future` reports to `pending`, coalescing equal normalized-hex/UTC-timestamp entries so
/// the first received wins and a repeating feed cannot grow the queue (#376). All entries in this
/// call share one arrival instant.
fn coalesce_into(pending: &mut Vec<PendingEntry>, future: Vec<FeedPlane>, received_at: Instant) {
    if future.is_empty() {
        return;
    }
    let mut seen: HashSet<(String, Option<DateTime<Utc>>)> = pending
        .iter()
        .map(|e| (pending_hex(&e.plane), e.plane.time))
        .collect();
    for plane in future {
        if seen.insert((pending_hex(&plane), plane.time)) {
            pending.push(PendingEntry { plane, received_at });
        }
    }
}

/// Pure pending-queue drain: promote reports eligible at `receipt_time`, drop component-expired
/// (#366) and residence-expired entries, then cap-trim to the earliest source timestamps (#376).
/// Returns the promoted reports, the retained pending entries and the dropped count.
fn drain_pending(
    pending: Vec<PendingEntry>,
    receipt_time: DateTime<Utc>,
    now: Instant,
) -> (Vec<FeedPlane>, Vec<PendingEntry>, usize) {
    let cutoff = receipt_time
        .checked_add_signed(ADMISSION_SKEW)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    let mut promoted = Vec::new();
    let mut kept = Vec::new();
    let mut dropped = 0usize;
    for entry in pending {
        // component expiry is a hard drop, whatever the source time (#366)
        if entry
            .plane
            .components_valid_until
            .is_some_and(|expiry| receipt_time > expiry)
        {
            dropped += 1;
            continue;
        }
        // drain eligible reports before applying expiry and the cap (#376)
        if entry.plane.time.is_none_or(|t| t <= cutoff) {
            promoted.push(entry.plane);
            continue;
        }
        if now.duration_since(entry.received_at) > PENDING_RESIDENCE {
            dropped += 1;
            continue;
        }
        kept.push(entry);
    }
    if kept.len() > PENDING_CAP {
        // retain the earliest source timestamps, normalized hex as the overflow tie-break
        kept.sort_by(|a, b| {
            a.plane
                .time
                .cmp(&b.plane.time)
                .then_with(|| pending_hex(&a.plane).cmp(&pending_hex(&b.plane)))
        });
        dropped += kept.len() - PENDING_CAP;
        kept.truncate(PENDING_CAP);
    }
    (promoted, kept, dropped)
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_into, drain_pending, pending_valid, split_future_reports, PendingEntry,
        PENDING_CAP,
    };
    use crate::{FeedArgs, FeedPlane};
    use as_aircraft::db::{Aircraft, AircraftDb, AircraftRegistration};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use std::time::Instant;

    fn seeded_aircraft_db() -> AircraftDb {
        let db = AircraftDb::open_in_memory().unwrap();
        let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        db.registration_insert_or_update_if_newer(&AircraftRegistration {
            prefix: "D".into(),
            country: "Germany".into(),
            remarks: String::new(),
            last_updated: old,
        })
        .unwrap();
        for (hex, reg) in [("3C0001", "D-AIMA"), ("3C0002", "D-AIMB")] {
            db.aircraft_insert_or_update_if_newer(&Aircraft {
                hex: hex.into(),
                call: "DLH459".into(),
                reg: reg.into(),
                type_code: "A388".into(),
                last_updated: old,
            })
            .unwrap();
        }
        db
    }

    fn test_args() -> FeedArgs {
        FeedArgs {
            min_lat: 47.5,
            max_lat: 54.25,
            min_lon: 4.0,
            max_lon: 16.5,
            ..FeedArgs::default()
        }
    }

    fn future_plane(time: DateTime<Utc>, hex: &str) -> FeedPlane {
        FeedPlane {
            time: Some(time),
            hex: hex.into(),
            call: "DLH459".into(),
            reg: "D-AIMA".into(),
            lat: 50.0,
            lon: 8.0,
            alt_ft: 35000.0,
            track: 90.0,
            speed_kts: 450.0,
            type_code: "A388".into(),
            ..FeedPlane::default()
        }
    }

    #[test]
    fn finding_376_invalid_or_ground_report_never_shadows_a_valid_one() {
        let db = seeded_aircraft_db();
        let args = test_args();
        let t = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let valid = future_plane(t, "3C0001");
        let mut ground = valid.clone();
        ground.ground = true;
        let mut bad_lat = valid.clone();
        bad_lat.lat = 999.0;
        let mut out_of_area = valid.clone();
        out_of_area.lat = 30.0;

        assert!(pending_valid(&ground, &args, &db).is_none());
        assert!(pending_valid(&bad_lat, &args, &db).is_none());
        assert!(pending_valid(&out_of_area, &args, &db).is_none());
        let kept = pending_valid(&valid, &args, &db).unwrap();
        assert_eq!(kept.hex, "3C0001");
    }

    #[test]
    fn finding_376_recovery_keys_coalescing_on_recovered_hex() {
        // Two distinct registrations with an empty hex must recover distinct hexes before
        // coalescing, so they do not collapse into one pending entry (#376).
        let db = seeded_aircraft_db();
        let args = test_args();
        let t = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let mut a = future_plane(t, "");
        a.reg = "D-AIMA".into();
        let mut b = future_plane(t, "");
        b.reg = "D-AIMB".into();

        let a = pending_valid(&a, &args, &db).unwrap();
        let b = pending_valid(&b, &args, &db).unwrap();
        assert_eq!(a.hex, "3C0001");
        assert_eq!(b.hex, "3C0002");

        let mut pending = Vec::new();
        coalesce_into(&mut pending, vec![a, b], Instant::now());
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn finding_376_component_timed_reports_drop_out_of_range_motion() {
        // A component-timed report never borrows motion or height from the cache, so an
        // out-of-range alt or speed is statelessly dropped before coalescing (#376).
        let db = seeded_aircraft_db();
        let args = test_args();
        let t = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let mut bad_alt = future_plane(t, "3C0001");
        bad_alt.components_valid_until = Some(t);
        bad_alt.alt_ft = 200000.0;
        let mut bad_speed = future_plane(t, "3C0001");
        bad_speed.components_valid_until = Some(t);
        bad_speed.speed_kts = 900.0;
        let mut good = future_plane(t, "3C0001");
        good.components_valid_until = Some(t);

        assert!(pending_valid(&bad_alt, &args, &db).is_none());
        assert!(pending_valid(&bad_speed, &args, &db).is_none());
        assert!(pending_valid(&good, &args, &db).is_some());
    }

    #[test]
    fn receipt_time_admission_is_inclusive_through_10s_and_later_reports_wait() {
        let receipt = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let times = [
            receipt,
            receipt + Duration::nanoseconds(1),
            receipt + Duration::seconds(10),
            receipt + Duration::seconds(10) + Duration::nanoseconds(1),
        ];
        let reports = times
            .into_iter()
            .map(|time| FeedPlane {
                time: Some(time),
                ..FeedPlane::default()
            })
            .collect();

        // through receipt + 10 s inclusive everything is eligible (#365/#376)
        let (eligible, pending) = split_future_reports(reports, receipt);
        assert_eq!(eligible.len(), 3);
        assert_eq!(eligible[0].time, Some(receipt));
        assert_eq!(eligible[1].time, Some(receipt + Duration::nanoseconds(1)));
        assert_eq!(eligible[2].time, Some(receipt + Duration::seconds(10)));
        assert_eq!(
            pending.iter().map(|r| r.time.unwrap()).collect::<Vec<_>>(),
            vec![times[3]]
        );

        // once receipt advances past the 10 s window, the last report promotes
        let (promoted, still_pending) = split_future_reports(
            pending,
            receipt + Duration::seconds(10) + Duration::nanoseconds(1),
        );
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].time, Some(times[3]));
        assert!(still_pending.is_empty());
    }

    #[test]
    fn finding_376_coalesces_equal_hex_and_timestamp_keeping_the_first() {
        let t = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let mut pending = Vec::new();
        let mut a = FeedPlane {
            time: Some(t),
            hex: "3c6444".into(),
            ..FeedPlane::default()
        };
        a.lon = 8.0;
        let mut b = FeedPlane {
            time: Some(t),
            hex: "3C6444".into(),
            ..FeedPlane::default()
        };
        b.lon = 9.0;
        coalesce_into(&mut pending, vec![a, b], Instant::now());
        assert_eq!(pending.len(), 1);
        // the first received wins
        assert_eq!(pending[0].plane.lon, 8.0);
    }

    #[test]
    fn finding_376_expires_entries_after_residence_and_drains_before_expiry() {
        let receipt = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let now = Instant::now();
        let resident = PendingEntry {
            plane: FeedPlane {
                time: Some(receipt + Duration::seconds(20)),
                hex: "3C0001".into(),
                ..FeedPlane::default()
            },
            received_at: now,
        };
        // eligible now, though also residence-expired: drained before expiry (#376)
        let eligible_but_old = PendingEntry {
            plane: FeedPlane {
                time: Some(receipt),
                hex: "3C0002".into(),
                ..FeedPlane::default()
            },
            received_at: now - std::time::Duration::from_secs(601),
        };
        // residence-expired and still future: dropped
        let expired = PendingEntry {
            plane: FeedPlane {
                time: Some(receipt + Duration::seconds(20)),
                hex: "3C0003".into(),
                ..FeedPlane::default()
            },
            received_at: now - std::time::Duration::from_secs(601),
        };
        let (promoted, kept, dropped) =
            drain_pending(vec![resident, eligible_but_old, expired], receipt, now);
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].hex, "3C0002");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].plane.hex, "3C0001");
        assert_eq!(dropped, 1);
    }

    #[test]
    fn finding_376_component_expiry_drops_pending_before_promotion() {
        let receipt = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let now = Instant::now();
        let stale = PendingEntry {
            plane: FeedPlane {
                time: Some(receipt),
                components_valid_until: Some(receipt - Duration::seconds(1)),
                hex: "3C0001".into(),
                ..FeedPlane::default()
            },
            received_at: now,
        };
        let (promoted, kept, dropped) = drain_pending(vec![stale], receipt, now);
        assert!(promoted.is_empty());
        assert!(kept.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn finding_376_cap_keeps_the_earliest_source_timestamps() {
        let receipt = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let now = Instant::now();
        let mut pending = Vec::with_capacity(PENDING_CAP + 1);
        for i in 0..(PENDING_CAP + 1) as i64 {
            pending.push(PendingEntry {
                plane: FeedPlane {
                    time: Some(receipt + Duration::seconds(20) + Duration::nanoseconds(i)),
                    hex: format!("{i:06X}"),
                    ..FeedPlane::default()
                },
                received_at: now,
            });
        }
        let (promoted, kept, dropped) = drain_pending(pending, receipt, now);
        assert!(promoted.is_empty());
        assert_eq!(kept.len(), PENDING_CAP);
        assert_eq!(dropped, 1);
        let max_kept = kept.iter().map(|e| e.plane.time.unwrap()).max().unwrap();
        assert_eq!(
            max_kept,
            receipt + Duration::seconds(20) + Duration::nanoseconds((PENDING_CAP - 1) as i64)
        );
    }
}
