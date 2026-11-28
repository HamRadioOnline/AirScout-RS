//! Analysis jobs and the history index: real database files in a temp data dir.
use airscout::analysis::{run, AnalysisJob};
use airscout::app::{AppState, Services};
use airscout::history::{HistoryData, HistoryIndex};
use airscout::jobs::{JobCtx, JobEvent};
use airscout::services::ServiceEvent;
use as_aircraft::db::{Aircraft, AircraftDb, AircraftType};
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_aircraft::{AltitudeProvenance, PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_settings::Settings;
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()
}

fn pos(hex: &str, secs: i64, lat: f64) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: format!("CALL{hex}"),
        lat,
        lon: 8.0,
        alt_ft: 30000.0,
        track: 90.0,
        speed_kts: 400.0,
        last_updated: t0() + Duration::seconds(secs),
    }
}

fn ctx() -> (JobCtx, Receiver<ServiceEvent>, Arc<AtomicBool>) {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    (JobCtx::new(7, cancel.clone(), tx), rx, cancel)
}

fn events(rx: &Receiver<ServiceEvent>) -> Vec<JobEvent> {
    rx.try_iter()
        .map(|e| match e {
            ServiceEvent::Job { id, event } => {
                assert_eq!(id, 7);
                event
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

fn loaded(ev: Vec<JobEvent>) -> Option<Box<HistoryData>> {
    ev.into_iter().find_map(|e| match e {
        JobEvent::HistoryLoaded(d) => Some(d),
        _ => None,
    })
}

fn last_done(ev: &[JobEvent]) -> String {
    match ev.last() {
        Some(JobEvent::Done(s)) => s.clone(),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn load_history_enriches_and_sorts() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    // inserted out of order
    db.bulk_insert(&[
        pos("AAAAAA", 20, 50.2),
        pos("BBBBBB", 10, 50.1),
        pos("AAAAAA", 0, 50.0),
    ])
    .unwrap();
    let adb = AircraftDb::open(dir.path()).unwrap();
    adb.aircraft_insert_or_update_if_newer(&Aircraft {
        hex: "AAAAAA".into(),
        call: String::new(),
        reg: "D-TEST".into(),
        type_code: "A320".into(),
        last_updated: t0(),
    })
    .unwrap();
    adb.aircraft_type_insert_or_update_if_newer(&AircraftType {
        icao: "A320".into(),
        iata: String::new(),
        manufacturer: "Airbus".into(),
        model: "A320".into(),
        category: PlaneCategory::Medium,
        last_updated: t0(),
    })
    .unwrap();
    let (c, rx, _) = ctx();
    run(&c, dir.path(), AnalysisJob::LoadHistory { ttl_min: 5 });
    let d = loaded(events(&rx)).expect("HistoryLoaded");
    assert_eq!(d.count, 3);
    assert_eq!(d.oldest, Some(t0()));
    assert_eq!(
        d.youngest,
        Some(t0() + Duration::seconds(20) + Duration::minutes(5))
    );
    let times: Vec<i64> = d
        .positions
        .iter()
        .map(|p| (p.time - t0()).num_seconds())
        .collect();
    assert_eq!(times, [0, 10, 20]);
    let a = &d.positions[0];
    assert_eq!(a.altitude_provenance, AltitudeProvenance::Unknown);
    assert_eq!(
        (a.hex.as_str(), a.reg.as_str(), a.type_code.as_str()),
        ("AAAAAA", "D-TEST", "A320")
    );
    assert_eq!(
        (a.manufacturer.as_str(), a.category),
        ("Airbus", PlaneCategory::Medium)
    );
    assert_eq!(
        (a.lat, a.lon, a.alt_ft, a.track, a.speed_kts),
        (50.0, 8.0, 30000.0, 90.0, 400.0)
    );
    assert_eq!(a.call, "CALLAAAAAA");
    let b = &d.positions[1];
    assert_eq!(b.reg, "[unknown]");
    assert_eq!(b.type_code, "[unknown]");
    assert_eq!(b.model, "[unknown]");
    assert_eq!(b.category, PlaneCategory::None);
}

#[test]
fn load_history_empty_db() {
    let dir = tempfile::tempdir().unwrap();
    let (c, rx, _) = ctx();
    run(&c, dir.path(), AnalysisJob::LoadHistory { ttl_min: 5 });
    let d = loaded(events(&rx)).expect("HistoryLoaded");
    assert_eq!((d.oldest, d.youngest, d.count), (None, None, 0));
    assert!(d.positions.is_empty());
}

#[test]
fn cancel_load_history_sends_no_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    db.bulk_insert(&[pos("AAAAAA", 0, 50.0), pos("BBBBBB", 1, 50.0)])
        .unwrap();
    let (c, rx, cancel) = ctx();
    cancel.store(true, Ordering::Relaxed);
    run(&c, dir.path(), AnalysisJob::LoadHistory { ttl_min: 5 });
    let ev = events(&rx);
    assert!(loaded(ev.clone()).is_none());
    assert!(
        !ev.iter().any(|e| matches!(e, JobEvent::Failed(_))),
        "{ev:?}"
    );
}

/// Small deterministic generator, so the test needs no `rand`.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

#[test]
fn latest_per_hex_matches_linear_scan() {
    let mut g = Lcg(42);
    let mut v: Vec<PlaneInfo> = (0..2000)
        .map(|_| PlaneInfo {
            hex: format!("{:06X}", g.next(60)),
            time: t0() + Duration::seconds(g.next(1000) as i64),
            lat: g.next(1000) as f64,
            ..PlaneInfo::default()
        })
        .collect();
    v.sort_by_key(|p| p.time); // stable: ties keep their order
    let idx = HistoryIndex::new(&v);
    for _ in 0..200 {
        let a = t0() + Duration::seconds(g.next(1100) as i64 - 50);
        let min = a - Duration::seconds(g.next(400) as i64);
        // the C# refill (MapDlg.cs:5384-5391)
        let mut want = std::collections::BTreeMap::new();
        for p in &v {
            if p.time >= min && p.time <= a {
                want.insert(p.hex.clone(), p.clone());
            }
        }
        let want: Vec<PlaneInfo> = want.into_values().collect();
        assert_eq!(idx.latest_per_hex(&v, min, a), want, "min {min} at {a}");
    }
    let m = idx.markers(&v);
    assert!(m.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        m.len(),
        v.iter()
            .map(|p| p.time)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
}

#[test]
fn save_then_load_roundtrip_json_and_csv() {
    for csv in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let rows = [
            pos("AAAAAA", 0, 50.0),
            pos("BBBBBB", 5, 50.5),
            pos("AAAAAA", 9, 51.0),
        ];
        PositionDb::open(dir.path())
            .unwrap()
            .bulk_insert(&rows)
            .unwrap();
        let path = dir
            .path()
            .join("Tmp")
            .join(if csv { "p.csv" } else { "p.json" });
        let (c, rx, _) = ctx();
        let job = AnalysisJob::SavePositions {
            path: path.clone(),
            from: t0(),
            to: t0() + Duration::seconds(9),
            csv,
        };
        run(&c, dir.path(), job);
        assert!(last_done(&events(&rx)).contains(&path.display().to_string()));
        run(&c, dir.path(), AnalysisJob::ClearPositions);
        assert_eq!(PositionDb::open(dir.path()).unwrap().count().unwrap(), 0);
        events(&rx);
        run(&c, dir.path(), AnalysisJob::LoadFiles(vec![path]));
        assert_eq!(last_done(&events(&rx)), "3 imported, 0 skipped");
        let back = PositionDb::open(dir.path())
            .unwrap()
            .get_all(t0(), t0() + Duration::seconds(9))
            .unwrap();
        assert_eq!(back, rows);
    }
}

#[test]
fn load_files_reports_counts() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("a.csv");
    std::fs::write(
        &good,
        "time[utc];hex;call;lat[deg];lon[deg];alt[ft];track[deg];speed[kts]\n\
         2024-05-01 12:00:00;AAAAAA;X1;50.0;8.0;100;0;0\n\
         2024-05-01 12:00:01;BBBBBB;X2;50.0;8.0;100;0;0\n\
         garbage;;;;;;;\n",
    )
    .unwrap();
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, "x").unwrap();
    let (c, rx, _) = ctx();
    run(&c, dir.path(), AnalysisJob::LoadFiles(vec![good, other]));
    let ev = events(&rx);
    assert_eq!(last_done(&ev), "2 imported, 1 skipped");
    assert!(ev
        .iter()
        .any(|e| matches!(e, JobEvent::Progress(s) if s.contains("notes.txt"))));
}

#[test]
fn clear_positions_empties_db() {
    let dir = tempfile::tempdir().unwrap();
    PositionDb::open(dir.path())
        .unwrap()
        .bulk_insert(&[pos("AAAAAA", 0, 50.0)])
        .unwrap();
    let (c, rx, _) = ctx();
    run(&c, dir.path(), AnalysisJob::ClearPositions);
    assert_eq!(last_done(&events(&rx)), "1 positions deleted");
    assert_eq!(PositionDb::open(dir.path()).unwrap().count().unwrap(), 0);
}

#[derive(Default)]
struct Log {
    runs: Vec<u64>,
    cancels: usize,
}

struct Fake(Rc<RefCell<Log>>);

impl Services for Fake {
    fn request_path(&mut self, _r: airscout::services::PathRequest) {}
    fn start_feed(
        &mut self,
        _i: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn run_analysis(&mut self, id: u64, _job: AnalysisJob) {
        self.0.borrow_mut().runs.push(id);
    }
    fn cancel_analysis(&mut self) {
        self.0.borrow_mut().cancels += 1;
    }
}

fn history_event(id: u64) -> ServiceEvent {
    ServiceEvent::Job {
        id,
        event: JobEvent::HistoryLoaded(Box::new(HistoryData {
            count: 1,
            positions: vec![PlaneInfo::default()],
            ..HistoryData::default()
        })),
    }
}

#[test]
fn off_while_loading_drops_stale_history() {
    let log = Rc::new(RefCell::new(Log::default()));
    let mut app = AppState::new(
        Settings::default(),
        None,
        Box::new(Fake(log.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    app.start_history_load();
    let id = log.borrow().runs[0];
    assert!(app.history.as_ref().unwrap().loading);
    app.stop_history();
    assert_eq!(log.borrow().cancels, 1, "OFF cancels at once");
    assert!(app.history.is_none());
    // the job's result was already queued
    app.on_event(history_event(id));
    assert!(app.history.is_none());
    // a new load is accepted, the old id still is not
    app.start_history_load();
    let id2 = log.borrow().runs[1];
    assert_ne!(id, id2);
    app.on_event(history_event(id));
    assert!(app.history.as_ref().unwrap().loading);
    app.on_event(history_event(id2));
    // the test data has no oldest/youngest: an empty database
    let h = app.history.as_ref().unwrap();
    assert_eq!(h.data.count, 1);
    assert!(!h.loading);
}

/// C# copies the hex's first-seen `PlaneInfo` and overwrites only time/lat/lon/track/alt/speed
/// (MapDlg.cs:9408-9415), so the call of the first row stays.
#[test]
fn load_history_keeps_first_call_per_hex() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = pos("AAAAAA", 0, 50.0);
    a.call = "FIRST1".into();
    let mut b = pos("AAAAAA", 10, 50.1);
    b.call = "SECOND".into();
    PositionDb::open(dir.path())
        .unwrap()
        .bulk_insert(&[a, b])
        .unwrap();
    let (c, rx, _) = ctx();
    run(&c, dir.path(), AnalysisJob::LoadHistory { ttl_min: 5 });
    let d = loaded(events(&rx)).unwrap();
    assert_eq!(d.positions.len(), 2);
    assert!(d.positions.iter().all(|p| p.call == "FIRST1"));
    assert_eq!(d.positions[1].lat, 50.1);
}
