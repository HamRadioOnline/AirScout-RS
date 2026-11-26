use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use chrono::{Duration, TimeZone, Utc};
use std::sync::Arc;

const TOL: f64 = 1e-6;

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
    q.altitude_provenance = as_aircraft::AltitudeProvenance::PressureEstimate;
    c.insert_or_update_if_newer(&q);
    let l = c.latest("3C0001").unwrap();
    assert_eq!(l.lat, 50.1);
    assert_eq!(
        l.altitude_provenance,
        as_aircraft::AltitudeProvenance::PressureEstimate
    );
    assert_eq!(l.old_lat, 50.0);
    assert_eq!(l.time, q.time);
}

#[test]
fn finding_365_cache_reads_remain_past_only() {
    let cache = PlaneInfoCache::new();
    let receipt = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
    let mut future = plane("3C0001", 0);
    future.time = receipt + Duration::nanoseconds(1);
    cache.insert_or_update_if_newer(&future);

    assert!(cache.get("3C0001", receipt, 5).is_none());
    assert!(cache.get_all(receipt, 5).is_empty());
    assert!(cache.get("3C0001", future.time, 5).is_some());
    assert_eq!(cache.get_all(future.time, 5).len(), 1);
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

#[test]
fn finding_23_track_wrap_uses_shortest_turn_and_normalizes() {
    let cache = PlaneInfoCache::new();
    let base = Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap();
    let mut p = PlaneInfo::new(
        base + Duration::seconds(60),
        "DLH1A",
        "D-AIZZ",
        "3C0001",
        50.0,
        8.0,
        10.0,
        35000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    p.old_time = Some(base);
    p.old_track = 350.0;
    p.old_speed_kts = 400.0;
    p.old_alt_ft = 35000.0;
    cache.set(p.clone());
    let got = cache
        .get("3C0001", base + Duration::seconds(120), 5)
        .unwrap();
    assert!((got.track - 30.0).abs() < 0.1, "{}", got.track);

    p.track = 350.0;
    p.old_track = 10.0;
    cache.set(p.clone());
    let got = cache
        .get("3C0001", base + Duration::seconds(120), 5)
        .unwrap();
    assert!((got.track - 330.0).abs() < 0.1, "{}", got.track);

    p.track = 0.0;
    p.old_track = 355.0;
    cache.set(p);
    let got = cache
        .get("3C0001", base + Duration::seconds(120), 5)
        .unwrap();
    assert!((got.track - 5.0).abs() < 0.1, "{}", got.track);
}

#[test]
fn finding_355_speed_ramp_uses_interval_average() {
    let cache = PlaneInfoCache::new();
    let base = Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap();
    let mut p = PlaneInfo::new(
        base + Duration::seconds(60),
        "DLH1A",
        "D-AIZZ",
        "3C0001",
        0.0,
        0.0,
        90.0,
        35000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    p.old_time = Some(base);
    p.old_speed_kts = 340.0;
    p.old_track = 90.0;
    p.old_alt_ft = 35000.0;
    cache.set(p);
    let got = cache
        .get("3C0001", base + Duration::seconds(120), 5)
        .unwrap();
    let traveled_km = as_geo::latlon::distance(0.0, 0.0, got.lat, got.lon);
    assert!(
        (traveled_km - 13.272666666666666).abs() < 1e-3,
        "{traveled_km}"
    );
    assert!((got.speed_kts - 460.0).abs() < TOL, "{}", got.speed_kts);
}

#[test]
fn finding_356_projection_transports_tangent() {
    let cache = PlaneInfoCache::new();
    let base = Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap();
    let p = PlaneInfo::new(
        base,
        "DLH1A",
        "D-AIZZ",
        "3C0001",
        80.0,
        0.0,
        90.0,
        35000.0,
        53.99568034557236, // 100 km in 1 hour
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    cache.set(p.clone());
    let full = cache
        .get("3C0001", base + Duration::hours(1), 120)
        .expect("full-step projection");
    assert!((full.lat - 79.960052).abs() < 1e-6, "{}", full.lat);
    assert!((full.lon - 5.165373).abs() < 1e-6, "{}", full.lon);
    assert!((full.track - 95.086690).abs() < 1e-6, "{}", full.track);

    let half = cache
        .get("3C0001", base + Duration::minutes(30), 120)
        .expect("first half-step projection");
    let second_cache = PlaneInfoCache::new();
    second_cache.set(half);
    let split = second_cache
        .get("3C0001", base + Duration::hours(1), 120)
        .expect("second half-step projection");
    assert!(
        (split.lat - full.lat).abs() < 1e-6,
        "{} vs {}",
        split.lat,
        full.lat
    );
    assert!(
        (split.lon - full.lon).abs() < 1e-6,
        "{} vs {}",
        split.lon,
        full.lon
    );
    assert!(
        (split.track - full.track).abs() < 1e-6,
        "{} vs {}",
        split.track,
        full.track
    );
}
