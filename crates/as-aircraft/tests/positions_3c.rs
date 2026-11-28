//! Stage 3c core gaps: position history queries, bulk insert, cache set/clear.
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::sync::atomic::{AtomicBool, Ordering};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

fn pos(hex: &str, call: &str, secs: i64) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: call.into(),
        lat: 50.0,
        lon: 8.0,
        alt_ft: 30000.0,
        track: 90.0,
        speed_kts: 400.0,
        last_updated: t0() + Duration::seconds(secs),
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[test]
fn hex_and_calls_in_range_inclusive() {
    let db = PositionDb::open_in_memory().unwrap();
    for p in [
        pos("3C0001", "TEST01", -1),
        pos("3C0002", "TEST02", 0),
        pos("3C0002", "TEST02", 5),
        pos("3C0003", "TEST03", 10),
        pos("3C0004", "TEST04", 11),
    ] {
        db.insert_or_update_if_newer(&p).unwrap();
    }
    let (from, to) = (t0(), t0() + Duration::seconds(10));
    assert_eq!(
        sorted(db.get_all_hex(from, to).unwrap()),
        ["3C0002", "3C0003"]
    );
    assert_eq!(
        sorted(db.get_all_calls(from, to).unwrap()),
        ["TEST02", "TEST03"]
    );
    assert!(db
        .get_all_hex(to + Duration::days(1), to + Duration::days(2))
        .unwrap()
        .is_empty());
}

#[test]
fn by_hex_in_range_sorted() {
    let db = PositionDb::open_in_memory().unwrap();
    for p in [
        pos("3C0002", "TEST02", 30),
        pos("3C0001", "TEST01", 20),
        pos("3C0001", "TEST01", 10),
        pos("3C0001", "TEST01", 0),
        pos("3C0001", "TEST01", 40),
    ] {
        db.insert_or_update_if_newer(&p).unwrap();
    }
    let (from, to) = (t0(), t0() + Duration::seconds(30));
    let got = db.get_by_hex_in("3C0001", from, to).unwrap();
    let secs: Vec<i64> = got
        .iter()
        .map(|p| (p.last_updated - t0()).num_seconds())
        .collect();
    assert_eq!(secs, [0, 10, 20]);
    let got = db.get_by_call_in("TEST02", from, to).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].hex, "3C0002");
    // bound parameters: a quote in the argument is data, not SQL
    assert!(db
        .get_by_hex_in("x' OR '1'='1", from, to)
        .unwrap()
        .is_empty());
}

#[test]
fn get_all_with_reports_progress_and_cancels() {
    let db = PositionDb::open_in_memory().unwrap();
    let rows: Vec<_> = (0..2500)
        .map(|i| pos("3C0001", "TEST01", 2500 - i))
        .collect();
    assert_eq!(db.bulk_insert(&rows).unwrap(), 2500);

    let cancel = AtomicBool::new(false);
    let mut seen = Vec::new();
    let all = db.get_all_with(&cancel, &mut |n| seen.push(n)).unwrap();
    assert_eq!(all.len(), 2500);
    assert_eq!(seen, [1000, 2000]);
    assert!(all
        .windows(2)
        .all(|w| w[0].last_updated <= w[1].last_updated));

    cancel.store(true, Ordering::Relaxed);
    let mut seen = Vec::new();
    let all = db.get_all_with(&cancel, &mut |n| seen.push(n)).unwrap();
    assert!(all.is_empty());
    assert!(seen.is_empty());
}

#[test]
fn get_all_with_cancel_mid_run_returns_empty() {
    let db = PositionDb::open_in_memory().unwrap();
    let rows: Vec<_> = (0..2500).map(|i| pos("3C0001", "TEST01", i)).collect();
    db.bulk_insert(&rows).unwrap();
    let cancel = AtomicBool::new(false);
    let all = db
        .get_all_with(&cancel, &mut |n| {
            if n == 1000 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
    assert!(all.is_empty());
}

#[test]
fn delete_all_counts() {
    let db = PositionDb::open_in_memory().unwrap();
    db.bulk_insert(&[pos("3C0001", "TEST01", 0), pos("3C0002", "TEST02", 1)])
        .unwrap();
    assert_eq!(db.delete_all().unwrap(), 2);
    assert_eq!(db.count().unwrap(), 0);
    assert_eq!(db.delete_all().unwrap(), 0);
}

#[test]
fn bulk_insert_rolls_back_on_error() {
    let dir = tempfile::tempdir().unwrap();
    let db = PositionDb::open(dir.path()).unwrap();
    assert_eq!(db.bulk_insert(&[pos("3C0001", "TEST01", 0)]).unwrap(), 1);
    // a second connection adds a trigger that rejects one call
    let other = rusqlite::Connection::open(dir.path().join("aircraftpositions.db3")).unwrap();
    other
        .execute_batch(
            "CREATE TRIGGER reject BEFORE INSERT ON AircraftPositions WHEN NEW.Call = 'BAD00'
             BEGIN SELECT RAISE(ABORT, 'rejected'); END",
        )
        .unwrap();
    drop(other);
    let rows = [
        pos("3C0002", "TEST02", 1),
        pos("3C0003", "TEST03", 2),
        pos("3C0004", "BAD00", 3),
    ];
    assert!(db.bulk_insert(&rows).is_err());
    assert_eq!(db.count().unwrap(), 1);
    // duplicates are ignored and not counted
    assert_eq!(db.bulk_insert(&[pos("3C0001", "TEST01", 0)]).unwrap(), 0);
}

fn plane(hex: &str, secs: i64, lat: f64) -> PlaneInfo {
    PlaneInfo::new(
        t0() + Duration::seconds(secs),
        "TEST12",
        "D-TEST",
        hex,
        lat,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A320",
        "Maker",
        "A320",
        PlaneCategory::Medium,
    )
}

#[test]
fn cache_set_replaces_without_old_values() {
    let cache = PlaneInfoCache::new();
    cache.insert_or_update_if_newer(&plane("3C1234", 0, 50.0));
    cache.insert_or_update_if_newer(&plane("3C1234", 10, 50.1));
    assert!(cache.latest("3C1234").unwrap().old_time.is_some());

    // set replaces unconditionally, even with an older report, and keeps no old_* data
    cache.set(plane("3C1234", -100, 51.0));
    let p = cache.latest("3C1234").unwrap();
    assert_eq!(p.lat, 51.0);
    assert_eq!(p.time, t0() - Duration::seconds(100));
    assert!(p.old_time.is_none());
    assert_eq!(cache.len(), 1);

    cache.set(plane("3C9999", 0, 52.0));
    assert_eq!(cache.len(), 2);
}

#[test]
fn cache_clear_empties() {
    let cache = PlaneInfoCache::new();
    cache.set(plane("3C1234", 0, 50.0));
    cache.set(plane("3C5678", 0, 50.0));
    cache.clear();
    assert!(cache.is_empty());
    assert!(cache.latest("3C1234").is_none());
}
