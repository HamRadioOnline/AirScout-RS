use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use chrono::{Duration, TimeZone, Utc};
use std::sync::Arc;

fn plane(hex: &str, secs: i64) -> PlaneInfo {
    let t = Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap() + Duration::seconds(secs);
    PlaneInfo::new(
        t,
        "DLH1A",
        "D-AIZZ",
        hex,
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

#[test]
fn cache_returns_copies() {
    let cache = PlaneInfoCache::new();
    let mut p = plane("3C6444", 0);
    cache.insert_or_update_if_newer(&p);
    // changing the caller's object afterwards does not change the cache
    p.lat = 10.0;
    let mut got = cache.get("3C6444", p.time, 5).unwrap();
    assert_eq!(got.lat, 50.0);
    got.lat = 20.0;
    assert_eq!(cache.get_all(p.time, 5)[0].lat, 50.0);
}

#[test]
fn delete_and_len() {
    let cache = PlaneInfoCache::new();
    assert!(cache.is_empty());
    assert_eq!(
        cache.bulk_insert_or_update_if_newer(&[plane("A", 0), plane("B", 0), plane("A", 10)]),
        3
    );
    assert_eq!(cache.len(), 2);
    assert!(cache.delete("A"));
    assert!(!cache.delete("A"));
    assert_eq!(cache.len(), 1);
}

/// Review Focus 4: a feed thread inserts while a reader calls get_all.
#[test]
fn cache_concurrent_insert_and_get_all() {
    let cache = Arc::new(PlaneInfoCache::new());
    let w = {
        let cache = cache.clone();
        std::thread::spawn(move || {
            for i in 0..10_000 {
                cache.insert_or_update_if_newer(&plane(&format!("{:06X}", i % 500), i));
            }
        })
    };
    let at = plane("X", 10_000).time;
    for _ in 0..200 {
        let all = cache.get_all(at, 1000);
        assert!(all.len() <= 500);
    }
    w.join().unwrap();
    assert_eq!(cache.len(), 500);
}

#[test]
fn latest_returns_stored_report_without_extrapolation() {
    let c = as_aircraft::PlaneInfoCache::new();
    assert!(c.latest("3C0001").is_none());
    let t = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 23, 12, 0, 0).unwrap();
    let p = as_aircraft::PlaneInfo::new(
        t,
        "DLH459",
        "D-AIMA",
        "3C0001",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "",
        "",
        as_aircraft::PlaneCategory::Superheavy,
    );
    c.insert_or_update_if_newer(&p);
    let mut q = p.clone();
    q.time = t + chrono::Duration::seconds(60);
    q.lat = 50.1;
    c.insert_or_update_if_newer(&q);
    let l = c.latest("3C0001").unwrap();
    assert_eq!(l.lat, 50.1);
    assert_eq!(l.old_lat, 50.0);
    assert_eq!(l.time, q.time);
}

#[test]
fn extreme_times_do_not_panic() {
    // a feed may report any time; `at - ttl` must not overflow (final review #1)
    let cache = PlaneInfoCache::new();
    cache.insert_or_update_if_newer(&plane("3C6444", 0));
    let early = chrono::DateTime::<Utc>::MIN_UTC + Duration::minutes(1);
    assert!(cache.get("3C6444", early, 5).is_none());
    assert!(cache.get_all(early, 5).is_empty());
    let t = plane("3C6444", 0).time;
    assert!(cache.get("3C6444", t, i64::MAX).is_some());
    assert_eq!(cache.get_all(t, i64::MAX).len(), 1);
}
