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

fn hex(i: usize) -> String {
    format!("{i:06X}")
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
fn finding_30_maintain_trims_to_count_with_stable_ties() {
    let db = PositionDb::open_in_memory().unwrap();
    let at = t0();
    for i in 0..3005 {
        db.insert_or_update_if_newer(&pos(&hex(i), 50.0 + i as f64 / 10_000.0, at))
            .unwrap();
    }
    let cfg = MaintainerConfig {
        max_count: 1_000,
        max_size_mb: 1.0e9,
        max_days: 36500,
    };
    let mut last = 0.0;
    let stats = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(db.count().unwrap(), 1_000);
    assert_eq!(stats.deleted_count, 2_005);

    let kept: Vec<String> = db.with_conn(|conn| {
        let mut st = conn
            .prepare("SELECT Hex FROM AircraftPositions ORDER BY LastUpdated ASC, rowid ASC")
            .unwrap();
        st.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    });
    let expected: Vec<String> = (2005..3005).map(hex).collect();
    assert_eq!(kept, expected);
}

#[test]
fn finding_31_size_limit_uses_current_occupied_pages_each_pass() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = PositionDb::open(dir.path()).unwrap();
        let now = Utc::now();
        for i in 0..3005 {
            db.insert_or_update_if_newer(&pos(&hex(i), 50.0, now - Duration::seconds(i as i64)))
                .unwrap();
        }
    }
    let db = PositionDb::open(dir.path()).unwrap();
    let now = Utc::now();
    let cfg = MaintainerConfig {
        max_count: 10_000_000,
        max_size_mb: 0.0,
        max_days: 36500,
    };
    let mut last = now.timestamp() as f64;
    let s1 = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(s1.deleted_size, 1000);
    let s2 = maintain(&db, &cfg, &mut last).unwrap();
    assert_eq!(s2.deleted_size, 1000);
    assert_eq!(db.count().unwrap(), 1005);
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

#[test]
fn finding_372_position_db_guards_schema_version_and_auto_vacuum() {
    let fresh_dir = tempfile::tempdir().unwrap();
    let fresh = PositionDb::open(fresh_dir.path()).unwrap();
    let (av, journal, ver): (i64, String, i64) = fresh.with_conn(|c| {
        (
            c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap(),
            c.query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap(),
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
        )
    });
    assert_eq!(av, 1);
    assert_eq!(journal.to_lowercase(), "wal");
    assert_eq!(ver, 1);

    let mem = PositionDb::open_in_memory().unwrap();
    let (mem_av, mem_ver): (i64, i64) = mem.with_conn(|c| {
        (
            c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap(),
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
        )
    });
    assert_eq!((mem_av, mem_ver), (1, 1));

    let ahead_dir = tempfile::tempdir().unwrap();
    let path = ahead_dir.path().join("aircraftpositions.db3");
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE FuturePositions (Hex TEXT PRIMARY KEY);
         INSERT INTO FuturePositions VALUES ('3C0001');
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let err = PositionDb::open(ahead_dir.path()).err().unwrap();
    assert!(
        matches!(
            err,
            as_aircraft::AircraftError::SchemaAhead {
                actual: 2,
                supported: 1
            }
        ),
        "unexpected error: {err:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let check = rusqlite::Connection::open(&path).unwrap();
    let journal: String = check
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal.to_lowercase(), "delete");
    let av: i64 = check
        .query_row("PRAGMA auto_vacuum", [], |r| r.get(0))
        .unwrap();
    assert_eq!(av, 0);
}
