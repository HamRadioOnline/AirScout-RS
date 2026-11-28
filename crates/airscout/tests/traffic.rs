//! Traffic dialog (Task 11): route splitting, the job and the default range. Synthetic data.
use airscout::analysis::{run, AnalysisJob};
use airscout::jobs::{JobCtx, JobEvent};
use airscout::services::ServiceEvent;
use airscout::traffic::{route_alpha, split_routes, traffic_range};
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_geo::latlon::GPoint;
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()
}

fn pos(hex: &str, secs: i64, lat: f64, lon: f64) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: "TST1".into(),
        lat,
        lon,
        alt_ft: 30000.0,
        track: 0.0,
        speed_kts: 400.0,
        last_updated: t0() + Duration::seconds(secs),
    }
}

#[test]
fn split_keeps_trigger_point() {
    // 0.1 deg steps are ~11 km; 5.0 deg is ~550 km: the jump starts a new segment
    let v = vec![
        pos("abc123", 0, 50.0, 8.0),
        pos("abc123", 1, 50.1, 8.0),
        pos("abc123", 2, 55.0, 8.0),
        pos("abc123", 3, 55.1, 8.0),
    ];
    let r = split_routes(&v);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0], vec![GPoint::new(50.0, 8.0), GPoint::new(50.1, 8.0)]);
    // the C# drops the trigger point; here it starts the second segment
    assert_eq!(r[1], vec![GPoint::new(55.0, 8.0), GPoint::new(55.1, 8.0)]);
}

#[test]
fn single_point_routes_dropped() {
    assert!(split_routes(&[pos("abc123", 0, 50.0, 8.0)]).is_empty());
    assert!(split_routes(&[]).is_empty());
    // a lone point after a jump is dropped, the first segment stays
    let v = vec![
        pos("abc123", 0, 50.0, 8.0),
        pos("abc123", 1, 50.1, 8.0),
        pos("abc123", 2, 55.0, 8.0),
    ];
    assert_eq!(split_routes(&v).len(), 1);
}

fn job_events(rx: &std::sync::mpsc::Receiver<ServiceEvent>) -> Vec<JobEvent> {
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

fn routes_of(ev: &[JobEvent]) -> Vec<Vec<GPoint>> {
    ev.iter()
        .filter_map(|e| match e {
            JobEvent::Routes(r) => Some(r.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn traffic_job_groups_by_hex() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    // two hexes interleaved in time, plus one row outside the range
    db.bulk_insert(&[
        pos("abc123", 0, 50.0, 8.0),
        pos("def456", 1, 40.0, 5.0),
        pos("abc123", 2, 50.1, 8.0),
        pos("def456", 3, 40.1, 5.0),
        pos("abc123", 99_999, 51.0, 8.0),
    ])
    .unwrap();
    let (tx, rx) = channel();
    let ctx = JobCtx::new(7, Arc::new(AtomicBool::new(false)), tx);
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Traffic {
            from: t0(),
            to: t0() + Duration::seconds(10),
        },
    );
    let ev = job_events(&rx);
    assert!(
        !ev.iter().any(|e| matches!(e, JobEvent::Failed(_))),
        "{ev:?}"
    );
    let mut routes = routes_of(&ev);
    routes.sort_by(|a, b| a[0].lat.total_cmp(&b[0].lat));
    assert_eq!(
        routes,
        vec![
            vec![GPoint::new(40.0, 5.0), GPoint::new(40.1, 5.0)],
            vec![GPoint::new(50.0, 8.0), GPoint::new(50.1, 8.0)],
        ]
    );
    assert!(matches!(ev.last(), Some(JobEvent::Done(_))), "{ev:?}");
}

#[test]
fn traffic_job_cancel_sends_no_routes() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    db.bulk_insert(&[pos("abc123", 0, 50.0, 8.0), pos("abc123", 1, 50.1, 8.0)])
        .unwrap();
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    cancel.store(true, Ordering::Relaxed);
    let ctx = JobCtx::new(7, cancel, tx);
    run(
        &ctx,
        dir.path(),
        AnalysisJob::Traffic {
            from: t0(),
            to: t0() + Duration::seconds(10),
        },
    );
    let ev = job_events(&rx);
    assert!(routes_of(&ev).is_empty(), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, JobEvent::Done(_))), "{ev:?}");
}

#[test]
fn empty_db_defaults_to_now() {
    let now = t0();
    assert_eq!(traffic_range(None, None, now), (now, now));
    let (a, b) = (now - Duration::hours(3), now - Duration::hours(1));
    assert_eq!(traffic_range(Some(a), Some(b), now), (a, b));
    // one end missing: the other falls back to now
    assert_eq!(traffic_range(Some(a), None, now), (a, now));
}

#[test]
fn opacity_to_alpha() {
    assert_eq!(route_alpha(10), 25);
    assert_eq!(route_alpha(100), 255);
    assert_eq!(route_alpha(50), 127);
}
