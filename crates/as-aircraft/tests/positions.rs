//! Position history DB and maintainer (`AirScout.AircraftPositions`).
use as_aircraft::check::Lookup;
use as_aircraft::positions::{maintain, AircraftPosition, MaintainerConfig, PositionDb};
use as_aircraft::{PlaneCategory, PlaneInfo};
use chrono::{DateTime, Duration, TimeZone, Utc};

struct AllAirlines;

impl Lookup for AllAirlines {
    fn airline_by_icao(&self, s: &str) -> bool {
        s == "DLH"
    }
    fn airline_by_iata(&self, s: &str) -> bool {
        s == "LH"
    }
    fn registration_by_reg(&self, _: &str) -> bool {
        false
    }
    fn airport_by_icao(&self, _: &str) -> bool {
        false
    }
    fn type_by_icao(&self, _: &str) -> bool {
        false
    }
    fn registration_min_len(&self) -> usize {
        1
    }
    fn type_icao_min_len(&self) -> usize {
        4
    }
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

fn pos(hex: &str, lat: f64, at: DateTime<Utc>) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: "DLH459".into(),
        lat,
        lon: 8.0,
        alt_ft: 35000.0,
        track: 90.0,
        speed_kts: 0.0,
        last_updated: at,
    }
}

#[test]
fn latest_per_hex() {
    let db = PositionDb::open_in_memory().unwrap();
    for (hex, lat, dt) in [
        ("3C0001", 50.0, 0),
        ("3C0001", 50.5, 60),
        ("3C0001", 51.0, 120),
        ("4B0002", 47.0, 30),
    ] {
        db.insert_or_update_if_newer(&pos(hex, lat, t0() + Duration::seconds(dt)))
            .unwrap();
    }
    let mut l = db.get_all_latest(t0() + Duration::seconds(120), 5).unwrap();
    l.sort_by(|a, b| a.hex.cmp(&b.hex));
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].hex, "3C0001");
    assert_eq!(l[0].lat, 51.0);
    assert_eq!(l[0].last_updated, t0() + Duration::seconds(120));
    assert_eq!(l[1].hex, "4B0002");
    // ttl excludes rows older than at - ttl
    let l = db.get_all_latest(t0() + Duration::seconds(400), 5).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(db.get_by_hex("3C0001").unwrap().len(), 3);
    assert_eq!(db.get_by_call("DLH459").unwrap().len(), 4);
    assert_eq!(
        db.get_all(t0(), t0() + Duration::seconds(60))
            .unwrap()
            .len(),
        3
    );
    assert_eq!(db.oldest().unwrap(), Some(t0()));
    assert_eq!(db.youngest().unwrap(), Some(t0() + Duration::seconds(120)));
}

#[test]
fn same_second_is_one_row() {
    let db = PositionDb::open_in_memory().unwrap();
    db.insert_or_update_if_newer(&pos("3C0001", 50.0, t0()))
        .unwrap();
    // sub-second difference collapses to the same unix second; the first row stays (#28)
    db.insert_or_update_if_newer(&pos("3C0001", 52.0, t0() + Duration::milliseconds(400)))
        .unwrap();
    assert_eq!(db.count().unwrap(), 1);
    assert_eq!(db.get_by_hex("3C0001").unwrap()[0].lat, 50.0);
}

#[test]
fn maintain_deletes_older_than_7_days() {
    let db = PositionDb::open_in_memory().unwrap();
    let now = Utc::now();
    db.insert_or_update_if_newer(&pos("3C0001", 50.0, now - Duration::days(8)))
        .unwrap();
    db.insert_or_update_if_newer(&pos("3C0001", 50.0, now - Duration::days(1)))
        .unwrap();
    let mut last = 0.0;
    let stats = maintain(&db, &MaintainerConfig::default(), &mut last).unwrap();
    assert_eq!(db.count().unwrap(), 1);
    assert_eq!(stats.deleted_old, 1);
}

#[test]
fn maintain_over_count_deletes_1000_oldest() {
    let db = PositionDb::open_in_memory().unwrap();
    let now = Utc::now();
    for i in 0..1500 {
        db.insert_or_update_if_newer(&pos("3C0001", 50.0, now - Duration::seconds(3000 - i)))
            .unwrap();
    }
    let cfg = MaintainerConfig {
        max_count: 10,
        max_size_mb: 1.0e9,
        max_days: 7,
    };
    let mut last = 0.0;
    let stats = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(db.count().unwrap(), 500);
    assert_eq!(stats.deleted_count, 1000);
    // the oldest rows went (rowid ASC = insertion order)
    assert_eq!(
        db.oldest().unwrap(),
        Some(DateTime::from_timestamp((now - Duration::seconds(2000)).timestamp(), 0).unwrap())
    );
}

#[test]
fn maintain_size_rule_uses_never_decreasing_last_size() {
    let db = PositionDb::open_in_memory().unwrap();
    let now = Utc::now();
    for i in 0..1500 {
        db.insert_or_update_if_newer(&pos("3C0001", 50.0, now - Duration::seconds(3000 - i)))
            .unwrap();
    }
    let cfg = MaintainerConfig {
        max_count: 10_000_000,
        max_size_mb: 0.0,
        max_days: 7,
    };
    let mut last = 0.0;
    let s1 = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(s1.deleted_size, 1000);
    assert!(last > 0.0);
    // size did not grow beyond `last`, so the rule stays quiet (#31)
    let s2 = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(s2.deleted_size, 0);
    assert_eq!(db.count().unwrap(), 500);
}

#[test]
fn bulk_insert_skips_invalid_hex() {
    let db = PositionDb::open_in_memory().unwrap();
    let mk = |hex: &str, call: &str| {
        PlaneInfo::new(
            t0(),
            call,
            "D-AIMA",
            hex,
            50.0,
            8.0,
            90.0,
            35000.0,
            450.0,
            "A388",
            "",
            "",
            PlaneCategory::Superheavy,
        )
    };
    let planes = vec![
        mk("3C0001", "DLH459"),
        mk("XYZ", "DLH459"),
        mk("3C0002", "XX1"),
        mk("3C0003", "DLH1"),
    ];
    let n = db.bulk_insert_plane_infos(&planes, &AllAirlines).unwrap();
    assert_eq!(n, 2);
    assert_eq!(db.count().unwrap(), 2);
    let p = &db.get_by_hex("3C0001").unwrap()[0];
    assert_eq!(p.alt_ft, 35000.0);
    assert_eq!(p.speed_kts, 450.0);
    assert!(db.file_size_mb().unwrap() > 0.0);
    db.delete_older_than(t0() + Duration::seconds(1)).unwrap();
    assert_eq!(db.count().unwrap(), 0);
}
