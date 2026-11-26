//! Feed worker loop (`PlaneFeed.OnDoWork`).
use as_aircraft::db::AircraftDb;
use as_aircraft::PlaneInfoCache;
use as_feeds::pipeline::PipelineConfig;
use as_feeds::worker::{FeedEvent, FeedWorker, WorkerConfig};
use as_feeds::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
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
    FeedPlane {
        time: Some(chrono::Utc::now()),
        hex: "3C0001".into(),
        call: "DLH459".into(),
        reg: "D-AIMA".into(),
        lat: 50.0,
        lon: 8.0,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 450.0,
        ..FeedPlane::default()
    }
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
