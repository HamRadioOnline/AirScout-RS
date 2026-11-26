//! Feed worker loop (`PlaneFeed.OnDoWork`).
use as_aircraft::db::{AircraftDb, AircraftRegistration, AircraftType, Airline};
use as_aircraft::positions::PositionDb;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_feeds::pipeline::PipelineConfig;
use as_feeds::worker::{FeedEvent, FeedWorker, SyncPoint, WorkerConfig, WorkerSync};
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct FakeFeed {
    script: VecDeque<Result<Vec<FeedPlane>, FeedError>>,
}

impl PlaneFeed for FakeFeed {
    fn name(&self) -> &'static str {
        "Fake"
    }
    fn info(&self) -> &'static str {
        "scripted"
    }
    fn disclaimer(&self) -> Option<&'static str> {
        None
    }
    fn enabled_by_default(&self) -> bool {
        true
    }
    fn settings(&self) -> toml::Table {
        toml::Table::new()
    }
    fn load_settings(&mut self, _: &toml::Table) -> Result<(), FeedError> {
        Ok(())
    }
    fn start(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
    fn get_planes(&mut self, _: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        self.script.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }
    fn stop(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}

fn args() -> FeedArgs {
    FeedArgs {
        min_lat: 47.5,
        max_lat: 54.25,
        min_lon: 4.0,
        max_lon: 16.5,
        min_alt_m: 5000,
        max_alt_m: 12200,
        ..FeedArgs::default()
    }
}

fn plane() -> FeedPlane {
    plane_for("3C0001")
}

fn plane_for(hex: &str) -> FeedPlane {
    FeedPlane {
        time: Some(chrono::Utc::now()),
        hex: hex.into(),
        call: "DLH459".into(),
        reg: "D-AIMA".into(),
        lat: 50.0,
        lon: 8.0,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 450.0,
        type_code: "A388".into(),
        manufacturer: "AIRBUS".into(),
        model: "A380".into(),
        ..FeedPlane::default()
    }
}

fn seeded_db() -> AircraftDb {
    let db = AircraftDb::open_in_memory().unwrap();
    let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    db.airline_insert_or_update_if_newer(&Airline {
        icao: "DLH".into(),
        iata: "LH".into(),
        airline: "Lufthansa".into(),
        country: "Germany".into(),
        last_updated: old,
    })
    .unwrap();
    db.registration_insert_or_update_if_newer(&AircraftRegistration {
        prefix: "D".into(),
        country: "Germany".into(),
        remarks: String::new(),
        last_updated: old,
    })
    .unwrap();
    db.aircraft_type_insert_or_update_if_newer(&AircraftType {
        icao: "A388".into(),
        iata: "388".into(),
        manufacturer: "AIRBUS".into(),
        model: "AIRBUS A-380-800".into(),
        category: PlaneCategory::Superheavy,
        last_updated: old,
    })
    .unwrap();
    db
}

fn cfg(interval_s: u64) -> WorkerConfig {
    WorkerConfig {
        interval_s,
        pipeline: PipelineConfig {
            extended_plausibility: true,
            max_error_dist: 30.0,
            log_errors: false,
        },
    }
}

fn next(rx: &Receiver<FeedEvent>) -> FeedEvent {
    rx.recv_timeout(Duration::from_secs(5)).expect("event")
}

/// Read events until the next [`FeedEvent::Planes`], skipping worker lifecycle/error events.
fn next_planes(rx: &Receiver<FeedEvent>) -> Vec<PlaneInfo> {
    loop {
        if let FeedEvent::Planes(planes) = next(rx) {
            return planes;
        }
    }
}

#[test]
fn worker_error_then_recovers() {
    let feed = FakeFeed {
        script: VecDeque::from([Err(FeedError::Http("500".into())), Ok(vec![plane()])]),
    };
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    let h = FeedWorker::spawn(
        Box::new(feed),
        args(),
        cfg(1),
        cache.clone(),
        Arc::new(AircraftDb::open_in_memory().unwrap()),
        None,
        tx,
    );
    assert!(matches!(next(&rx), FeedEvent::Started));
    assert!(matches!(next(&rx), FeedEvent::Error(_)));
    match next(&rx) {
        FeedEvent::Planes(p) => assert_eq!(p.len(), 1),
        e => panic!("{e:?}"),
    }
    assert_eq!(cache.len(), 1);
    h.stop();
    h.join();
    let rest: Vec<FeedEvent> = rx.try_iter().collect();
    assert!(matches!(rest.last(), Some(FeedEvent::Stopped)));
}

#[test]
fn worker_invalid_area_stops() {
    let mut a = args();
    a.max_lat = a.min_lat;
    let (tx, rx) = channel();
    let h = FeedWorker::spawn(
        Box::new(FakeFeed {
            script: VecDeque::new(),
        }),
        a,
        cfg(1),
        Arc::new(PlaneInfoCache::new()),
        Arc::new(AircraftDb::open_in_memory().unwrap()),
        None,
        tx,
    );
    h.join();
    let ev: Vec<FeedEvent> = rx.try_iter().collect();
    assert!(matches!(ev.first(), Some(FeedEvent::Started)));
    assert!(matches!(&ev[1], FeedEvent::Error(m) if m.contains("Area boundaries")));
    assert!(matches!(ev.last(), Some(FeedEvent::Stopped)));
    assert_eq!(ev.len(), 3);
}

#[test]
fn worker_stop_within_1s() {
    let (tx, rx) = channel();
    let h = FeedWorker::spawn(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![plane()])]),
        }),
        args(),
        cfg(60),
        Arc::new(PlaneInfoCache::new()),
        Arc::new(AircraftDb::open_in_memory().unwrap()),
        None,
        tx,
    );
    assert!(matches!(next(&rx), FeedEvent::Started));
    assert!(matches!(next(&rx), FeedEvent::Planes(_)));
    let t = Instant::now();
    h.stop();
    h.join();
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
    assert!(matches!(rx.try_iter().last(), Some(FeedEvent::Stopped)));
}

#[test]
fn worker_holds_future_reports_out_of_every_sink() {
    // beyond the 10 s admission window (#376), so the report is held, not admitted
    let mut future = plane();
    future.time = Some(Utc::now() + ChronoDuration::seconds(30));
    let feed = FakeFeed {
        script: VecDeque::from([Ok(vec![future])]),
    };
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    let db = Arc::new(AircraftDb::open_in_memory().unwrap());
    let positions = Arc::new(PositionDb::open_in_memory().unwrap());
    let mut a = args();
    a.keep_history = true;
    let h = FeedWorker::spawn(
        Box::new(feed),
        a,
        cfg(1),
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx,
    );

    assert!(matches!(next(&rx), FeedEvent::Started));
    match next(&rx) {
        FeedEvent::Planes(planes) => assert!(planes.is_empty()),
        e => panic!("{e:?}"),
    }
    assert!(cache.latest("3C0001").is_none());
    assert!(db.aircraft_find_by_hex("3C0001").unwrap().is_none());
    assert!(positions.get_by_hex("3C0001").unwrap().is_empty());

    h.stop();
    h.join();
}

#[test]
fn worker_promotes_pending_reports_on_error_and_quiet_cycles_without_regressing() {
    let t0 = Utc::now();
    let mut baseline_same_hex = plane();
    baseline_same_hex.time = Some(t0 - ChronoDuration::seconds(1));
    // beyond the 10 s admission window (#376): held on the first cycle, then eligible on the
    // error cycle (receipt ~t0 + 1 s, cutoff ~t0 + 11 s) and the quiet cycle respectively.
    let mut during_error = plane_for("3C0002");
    during_error.time = Some(t0 + ChronoDuration::milliseconds(10500));
    let mut pending_quiet = plane_for("3C0003");
    pending_quiet.time = Some(t0 + ChronoDuration::milliseconds(11500));

    let feed = FakeFeed {
        script: VecDeque::from([
            Ok(vec![baseline_same_hex.clone(), during_error, pending_quiet]),
            Err(FeedError::Http("500".into())),
            Ok(Vec::new()),
        ]),
    };
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    let db = Arc::new(seeded_db());
    let positions = Arc::new(PositionDb::open_in_memory().unwrap());
    let mut a = args();
    a.keep_history = true;
    let h = FeedWorker::spawn(
        Box::new(feed),
        a,
        cfg(1),
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx,
    );

    assert!(matches!(next(&rx), FeedEvent::Started));
    // cycle 1: the past report is admitted; both future reports are held out of every sink
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].time, baseline_same_hex.time.unwrap());
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(
        cache.latest("3C0001").unwrap().time,
        baseline_same_hex.time.unwrap()
    );
    assert!(cache.latest("3C0002").is_none());
    assert!(cache.latest("3C0003").is_none());
    assert_eq!(db.aircraft_count().unwrap(), 1);
    assert!(db.aircraft_find_by_hex("3C0002").unwrap().is_none());
    assert!(db.aircraft_find_by_hex("3C0003").unwrap().is_none());
    assert_eq!(positions.get_by_hex("3C0001").unwrap().len(), 1);
    assert!(positions.get_by_hex("3C0002").unwrap().is_empty());
    assert!(positions.get_by_hex("3C0003").unwrap().is_empty());

    // cycle 2 (error): the now-eligible pending report drains and promotes (#376)
    assert!(matches!(next(&rx), FeedEvent::Error(_)));
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].hex, "3C0002");
        }
        e => panic!("{e:?}"),
    }
    assert!(cache.latest("3C0002").is_some());
    assert!(db.aircraft_find_by_hex("3C0002").unwrap().is_some());
    assert_eq!(positions.get_by_hex("3C0002").unwrap().len(), 1);
    assert!(cache.latest("3C0003").is_none());

    // cycle 3 (empty/quiet): the second pending report drains and promotes
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].hex, "3C0003");
        }
        e => panic!("{e:?}"),
    }
    assert!(cache.latest("3C0003").is_some());
    assert!(db.aircraft_find_by_hex("3C0003").unwrap().is_some());
    assert_eq!(positions.get_by_hex("3C0003").unwrap().len(), 1);

    h.stop();
    h.join();
}

#[test]
fn finding_49_zero_interval_retries_start_no_faster_than_one_second() {
    use std::sync::Mutex;
    struct FailingStart {
        starts: Arc<Mutex<Vec<Instant>>>,
    }
    impl PlaneFeed for FailingStart {
        fn name(&self) -> &'static str {
            "FailingStart"
        }
        fn info(&self) -> &'static str {
            "scripted"
        }
        fn disclaimer(&self) -> Option<&'static str> {
            None
        }
        fn enabled_by_default(&self) -> bool {
            true
        }
        fn settings(&self) -> toml::Table {
            toml::Table::new()
        }
        fn load_settings(&mut self, _: &toml::Table) -> Result<(), FeedError> {
            Ok(())
        }
        fn start(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
            self.starts.lock().unwrap().push(Instant::now());
            Err(FeedError::Config("always fails".into()))
        }
        fn get_planes(&mut self, _: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
            Ok(Vec::new())
        }
        fn stop(&mut self, _: &FeedArgs) -> Result<(), FeedError> {
            Ok(())
        }
    }

    let starts = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = channel();
    let h = FeedWorker::spawn(
        Box::new(FailingStart {
            starts: starts.clone(),
        }),
        args(),
        cfg(0),
        Arc::new(PlaneInfoCache::new()),
        Arc::new(AircraftDb::open_in_memory().unwrap()),
        None,
        tx,
    );
    assert!(matches!(next(&rx), FeedEvent::Started));
    assert!(matches!(next(&rx), FeedEvent::Error(_)));
    assert!(matches!(next(&rx), FeedEvent::Error(_)));
    h.stop();
    h.join();

    let starts = starts.lock().unwrap();
    assert!(starts.len() >= 2);
    for w in starts.windows(2) {
        let delta = w[1].duration_since(w[0]);
        assert!(delta >= Duration::from_millis(900), "{delta:?}");
    }
}

#[test]
fn finding_50_live_events_use_winners_and_older_reports_stay_history_only() {
    let base = Utc::now() - ChronoDuration::seconds(300);
    let mut accepted = plane();
    accepted.time = Some(base);
    let mut older = plane();
    older.time = Some(base - ChronoDuration::seconds(1));
    older.lon = 8.01;
    let mut equal = plane();
    equal.time = Some(base);
    equal.lon = 8.02;
    let mut newer = plane();
    newer.time = Some(base + ChronoDuration::seconds(30));
    newer.lon = 8.03;

    let feed = FakeFeed {
        script: VecDeque::from([
            Ok(vec![accepted.clone()]),
            Ok(vec![older.clone(), equal.clone(), newer.clone()]),
        ]),
    };
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    let db = Arc::new(seeded_db());
    let positions = Arc::new(PositionDb::open_in_memory().unwrap());
    let mut a = args();
    a.keep_history = true;
    let mut c = cfg(1);
    c.pipeline.extended_plausibility = false;
    let h = FeedWorker::spawn(
        Box::new(feed),
        a,
        c,
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx,
    );

    assert!(matches!(next(&rx), FeedEvent::Started));
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].time, accepted.time.unwrap());
        }
        e => panic!("{e:?}"),
    }
    // only the strictly newer sample becomes a live successor (#50)
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].time, newer.time.unwrap());
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(cache.latest("3C0001").unwrap().time, newer.time.unwrap());
    assert_eq!(
        db.aircraft_find_by_hex("3C0001")
            .unwrap()
            .unwrap()
            .last_updated
            .timestamp(),
        newer.time.unwrap().timestamp()
    );
    // the older and equal reports stay history-eligible under #28's first-write-wins key
    let history = positions.get_by_hex("3C0001").unwrap();
    assert_eq!(history.len(), 3);
    let seconds: Vec<i64> = history.iter().map(|p| p.last_updated.timestamp()).collect();
    assert_eq!(
        seconds,
        vec![
            older.time.unwrap().timestamp(),
            accepted.time.unwrap().timestamp(),
            newer.time.unwrap().timestamp(),
        ]
    );

    h.stop();
    h.join();
}

#[test]
fn finding_50_rejected_report_never_reaches_history() {
    // A candidate rejected by the atomic plausibility re-validation inside
    // `bulk_insert_validated` must not enter PositionDb history, while a valid
    // older report still does (#50 round 2). Seed the cache at t=100, then one
    // cycle delivers older (t=99), a plausible successor A (t=110) and B
    // (t=130) that is only implausible once A has been admitted.
    let base = Utc::now() - ChronoDuration::seconds(300);
    let mut older = plane();
    older.time = Some(base - ChronoDuration::seconds(1));
    older.lon = 8.01;
    let mut accepted = plane();
    accepted.time = Some(base + ChronoDuration::seconds(10));
    accepted.lon = 8.03;
    let mut rejected = plane();
    rejected.time = Some(base + ChronoDuration::seconds(30));
    rejected.lon = 8.0;

    let feed = FakeFeed {
        script: VecDeque::from([Ok(vec![older.clone(), accepted.clone(), rejected.clone()])]),
    };
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    cache.insert_or_update_if_newer(&PlaneInfo::new(
        base,
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    ));
    let db = Arc::new(seeded_db());
    let positions = Arc::new(PositionDb::open_in_memory().unwrap());
    let mut a = args();
    a.keep_history = true;
    let h = FeedWorker::spawn(
        Box::new(feed),
        a,
        cfg(1),
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx,
    );

    assert!(matches!(next(&rx), FeedEvent::Started));
    match next(&rx) {
        FeedEvent::Planes(planes) => {
            assert_eq!(planes.len(), 1);
            assert_eq!(planes[0].time, accepted.time.unwrap());
        }
        e => panic!("{e:?}"),
    }
    h.stop();
    h.join();

    // the plausibility-rejected report must not reach history; the valid older
    // report still does
    let history = positions.get_by_hex("3C0001").unwrap();
    let seconds: Vec<i64> = history.iter().map(|p| p.last_updated.timestamp()).collect();
    assert_eq!(
        seconds,
        vec![
            older.time.unwrap().timestamp(),
            accepted.time.unwrap().timestamp(),
        ]
    );
    assert_eq!(
        db.aircraft_find_by_hex("3C0001")
            .unwrap()
            .unwrap()
            .last_updated
            .timestamp(),
        accepted.time.unwrap().timestamp()
    );
}

#[test]
fn finding_50_atomic_admission_validates_each_candidate_against_the_accepted_winner() {
    let base = Utc::now() - ChronoDuration::seconds(300);
    let cache = PlaneInfoCache::new();
    let mk = |t: chrono::DateTime<Utc>, lon: f64| {
        PlaneInfo::new(
            t,
            "DLH459",
            "D-AIMA",
            "3C0001",
            50.0,
            lon,
            90.0,
            35000.0,
            450.0,
            "A388",
            "AIRBUS",
            "A380",
            PlaneCategory::Superheavy,
        )
    };
    cache.insert_or_update_if_newer(&mk(base, 8.0));

    let cfg = PipelineConfig {
        extended_plausibility: true,
        max_error_dist: 30.0,
        log_errors: false,
    };
    // candidate A (t=110) moves east and is valid against t=100; candidate B (t=130) sits west
    // of A while still reporting east, so it is only invalid once A has been admitted — the
    // TOCTOU that a plain pre-validation against t=100 misses (#50).
    let a = mk(base + ChronoDuration::seconds(10), 8.03);
    let b = mk(base + ChronoDuration::seconds(30), 8.0);

    let (winners, _) = cache.bulk_insert_validated(
        &[a, b],
        |p, cur| as_feeds::pipeline::plausibility_pass(p, cur, &cfg).is_ok(),
        |_| {},
    );
    assert_eq!(winners.len(), 1);
    assert_eq!(winners[0].time, base + ChronoDuration::seconds(10));
    assert_eq!(
        cache.latest("3C0001").unwrap().time,
        base + ChronoDuration::seconds(10)
    );
}

#[test]
fn finding_50_cross_worker_admission_serializes_live_events() {
    // The #50 cross-worker scenarios through the real worker path.
    //
    // Phase (a): a report 110 whose source time is ahead of receipt by more than the 10 s
    // admission skew is held in the pending queue on the first cycle and promoted through
    // `drain_pending` on a later cycle; it parks at admission, and a strictly newer 130 admitted
    // by a second worker beats it — 110 emits no live event, does not overwrite 130 in the cache
    // or `AircraftDb`, but still reaches `PositionDb` history.
    //
    // Phase (b): two eligible successors 140 then 150 have their DB writes complete in reverse
    // order (150 first) by holding 140's writer at `at_db_write` until 150's worker has finished,
    // so the `AircraftDb` source-time guard must keep 150's full row (call and last_updated).
    // Synthetic hex 3C0001, localhost only.
    let mk = |t: chrono::DateTime<Utc>, call: &str, lon: f64| FeedPlane {
        time: Some(t),
        hex: "3C0001".into(),
        call: call.into(),
        reg: "D-AIMA".into(),
        lat: 50.0,
        lon,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 450.0,
        type_code: "A388".into(),
        manufacturer: "AIRBUS".into(),
        model: "A380".into(),
        ..FeedPlane::default()
    };

    // ---- phase (a): 130 (worker B) beats the delayed pending promotion 110 (worker A) ----
    let cache = Arc::new(PlaneInfoCache::new());
    let db = Arc::new(seeded_db());
    let positions = Arc::new(PositionDb::open_in_memory().unwrap());
    let (tx, rx) = channel();
    let mut a = args();
    a.keep_history = true;

    // Capture `base` as late as possible so the source-time offsets below are measured from the
    // worker's real start, not the test setup. The accepted live state is anchored at `base`.
    let base = Utc::now();
    cache.insert_or_update_if_newer(&PlaneInfo::new(
        base,
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    ));

    // 110 is ~1 s beyond the 10 s admission skew, so cycle 1 holds it in `pending`; worker A's
    // 3 s interval brings it inside the window on cycle 2, when it is promoted. Both timestamps
    // are whole-second offsets from `base` so their history keys (integer seconds) stay distinct.
    let t110 = base + ChronoDuration::seconds(11);
    let t130 = base + ChronoDuration::seconds(12);

    let admission_a = SyncPoint::new();
    let worker_a = FeedWorker::spawn_with_sync(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![mk(t110, "DLH459", 8.03)])]),
        }),
        a.clone(),
        cfg(3),
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx.clone(),
        WorkerSync {
            at_admission: Some(admission_a.clone()),
            ..WorkerSync::default()
        },
    );

    // cycle 1 of worker A holds 110 (empty live event), then it parks at admission on cycle 2.
    assert!(matches!(next(&rx), FeedEvent::Started));
    let held = next_planes(&rx);
    assert!(held.is_empty(), "110 must be held out of cycle 1");
    // wait until worker A is genuinely paused at admission with the promoted 110.
    admission_a.wait_arrived();

    // spawn worker B only now, so its receipt is late enough that 130 is eligible directly.
    let worker_b = FeedWorker::spawn_with_sync(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![mk(t130, "DLH459", 8.06)])]),
        }),
        a.clone(),
        cfg(1),
        cache.clone(),
        db.clone(),
        Some(positions.clone()),
        tx.clone(),
        WorkerSync::default(),
    );

    // worker B admits 130 first and emits its live event.
    let first = next_planes(&rx);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].time.timestamp(), t130.timestamp());
    // release worker A: 110 loses to 130 — no live event, no cache/metadata change — but stays
    // eligible for history.
    admission_a.release();
    worker_a.stop();
    worker_b.stop();
    worker_a.join();
    worker_b.join();

    // The losing 110 promotion must not emit a live event, and no worker error may pass.
    for ev in rx.try_iter() {
        match ev {
            FeedEvent::Planes(p) => {
                assert!(p.is_empty(), "110 promotion must not emit a live event")
            }
            FeedEvent::Error(m) => panic!("unexpected worker error in phase (a): {m}"),
            _ => {}
        }
    }

    assert_eq!(cache.latest("3C0001").unwrap().time, t130);
    assert_eq!(
        db.aircraft_find_by_hex("3C0001")
            .unwrap()
            .unwrap()
            .last_updated
            .timestamp(),
        t130.timestamp()
    );
    let history = positions.get_by_hex("3C0001").unwrap();
    let seconds: Vec<i64> = history.iter().map(|p| p.last_updated.timestamp()).collect();
    assert_eq!(seconds, vec![t110.timestamp(), t130.timestamp()]);

    // ---- phase (b): successors 140 then 150; 150's DB writes complete before 140's ----
    // Fresh past-anchored state so 140/150 are eligible the moment their workers run; they only
    // need to be strictly newer than each other with distinct integer seconds for the history key.
    let base_b = Utc::now() - ChronoDuration::seconds(300);
    let t140 = base_b + ChronoDuration::seconds(40);
    let t150 = base_b + ChronoDuration::seconds(50);
    let cache_b = Arc::new(PlaneInfoCache::new());
    cache_b.insert_or_update_if_newer(&PlaneInfo::new(
        base_b,
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "AIRBUS",
        "A380",
        PlaneCategory::Superheavy,
    ));
    let db_b = Arc::new(seeded_db());
    let positions_b = Arc::new(PositionDb::open_in_memory().unwrap());
    let (tx_b, rx_b) = channel();

    let write_a = SyncPoint::new();
    let admission_b = SyncPoint::new();
    let worker_a2 = FeedWorker::spawn_with_sync(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![mk(t140, "DLH140", 8.09)])]),
        }),
        a.clone(),
        cfg(1),
        cache_b.clone(),
        db_b.clone(),
        Some(positions_b.clone()),
        tx_b.clone(),
        WorkerSync {
            at_db_write: Some(write_a.clone()),
            ..WorkerSync::default()
        },
    );
    let worker_b2 = FeedWorker::spawn_with_sync(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![mk(t150, "DLH150", 8.12)])]),
        }),
        a.clone(),
        cfg(1),
        cache_b.clone(),
        db_b.clone(),
        Some(positions_b.clone()),
        tx_b.clone(),
        WorkerSync {
            at_admission: Some(admission_b.clone()),
            ..WorkerSync::default()
        },
    );

    // worker A2 admits 140 (emits), then parks before its DB writes.
    let second = next_planes(&rx_b);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].time.timestamp(), t140.timestamp());
    // release worker B2: 150 wins and its DB writes complete first.
    admission_b.release();
    let third = next_planes(&rx_b);
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].time.timestamp(), t150.timestamp());
    // wait until 150's DB writes are committed before releasing 140's, so the earlier writer's
    // writes demonstrably run last.
    worker_b2.stop();
    worker_b2.join();
    write_a.release();
    worker_a2.stop();
    worker_a2.join();

    for ev in rx_b.try_iter() {
        match ev {
            FeedEvent::Planes(p) => assert!(p.is_empty(), "no further live events expected"),
            FeedEvent::Error(m) => panic!("unexpected worker error in phase (b): {m}"),
            _ => {}
        }
    }

    // 140 writes last with an older source time and a distinct callsign; the guard keeps 150's
    // full row.
    let winner = db_b.aircraft_find_by_hex("3C0001").unwrap().unwrap();
    assert_eq!(winner.call, "DLH150");
    assert_eq!(winner.last_updated.timestamp(), t150.timestamp());
    let history_b = positions_b.get_by_hex("3C0001").unwrap();
    let seconds_b: Vec<i64> = history_b
        .iter()
        .map(|p| p.last_updated.timestamp())
        .collect();
    assert_eq!(seconds_b, vec![t140.timestamp(), t150.timestamp()]);
}

#[test]
fn worker_sync_points_are_one_shot_and_never_deadlock() {
    // A worker with a sync point configured must still stop and join promptly: the rendezvous is
    // one-shot (taken on first use) and timeout-bounded, so a later cycle cannot re-arm it and a
    // missing release cannot wedge the thread (#50 round 3).
    let (tx, rx) = channel();
    let cache = Arc::new(PlaneInfoCache::new());
    let admission = SyncPoint::new();
    let h = FeedWorker::spawn_with_sync(
        Box::new(FakeFeed {
            script: VecDeque::from([Ok(vec![plane()]), Ok(vec![plane()]), Ok(vec![plane()])]),
        }),
        args(),
        cfg(1),
        cache.clone(),
        Arc::new(seeded_db()),
        None,
        tx,
        WorkerSync {
            at_admission: Some(admission.clone()),
            ..WorkerSync::default()
        },
    );

    assert!(matches!(next(&rx), FeedEvent::Started));
    // The worker parks at the first non-empty cycle's admission point; the test never releases it.
    admission.wait_arrived();
    let t = Instant::now();
    h.stop();
    h.join();
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(matches!(rx.try_iter().last(), Some(FeedEvent::Stopped)));
}

#[test]
fn finding_374_worker_reports_safe_parse_failure() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/stations")
        .with_body("invalid result SyntheticCredential")
        .create();
    let mut feed = as_feeds::feeds::opensky::OpenSky::default();
    feed.url = format!("{}/stations", server.url());
    let (tx, rx) = channel();
    let handle = FeedWorker::spawn(
        Box::new(feed),
        args(),
        cfg(60),
        Arc::new(PlaneInfoCache::new()),
        Arc::new(AircraftDb::open_in_memory().unwrap()),
        None,
        tx,
    );
    assert!(matches!(next(&rx), FeedEvent::Started));
    let FeedEvent::Error(error) = next(&rx) else {
        panic!("invalid response should produce a worker error");
    };
    mock.assert();
    handle.stop();
    handle.join();
    assert!(error.contains("OpenSky JSON object"));
    assert!(!error.contains("SyntheticCredential"));
}
