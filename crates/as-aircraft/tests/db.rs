use as_aircraft::db::{
    aircraft_registrations_from_json, aircraft_types_from_json, aircrafts_from_json,
    airlines_from_json, airports_from_json, Aircraft, AircraftDb, AircraftType,
};
use as_aircraft::PlaneCategory;
use chrono::{TimeZone, Utc};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        as_golden::repo_root().join(format!("tests/fixtures/aircraft/{name}.json")),
    )
    .unwrap()
}

fn loaded() -> AircraftDb {
    let db = AircraftDb::open_in_memory().unwrap();
    db.aircraft_bulk_insert_or_update_if_newer(
        &aircrafts_from_json(&fixture("Aircrafts")).unwrap(),
    )
    .unwrap();
    db.aircraft_type_bulk_insert_or_update_if_newer(
        &aircraft_types_from_json(&fixture("AircraftTypes")).unwrap(),
    )
    .unwrap();
    db.airline_bulk_insert_or_update_if_newer(&airlines_from_json(&fixture("Airlines")).unwrap())
        .unwrap();
    db.airport_bulk_insert_or_update_if_newer(&airports_from_json(&fixture("Airports")).unwrap())
        .unwrap();
    db.registration_bulk_insert_or_update_if_newer(
        &aircraft_registrations_from_json(&fixture("AircraftRegistrations")).unwrap(),
    )
    .unwrap();
    db
}

#[test]
fn import_fixtures_counts() {
    let db = loaded();
    assert_eq!(db.aircraft_count().unwrap(), 16);
    assert_eq!(db.aircraft_type_count().unwrap(), 26);
    assert_eq!(db.airline_count().unwrap(), 10);
    assert_eq!(db.airport_count().unwrap(), 8);
    assert_eq!(db.registration_count().unwrap(), 12);
    assert_eq!(db.registration_min_len(), 1);
    assert_eq!(db.type_icao_min_len(), 4);
}

#[test]
fn finders() {
    let db = loaded();
    let a = db.aircraft_find_by_hex("3C6444").unwrap().unwrap();
    assert_eq!(
        (a.call.as_str(), a.reg.as_str(), a.type_code.as_str()),
        ("DLH2LC", "D-AIBD", "A319")
    );
    assert_eq!(a.last_updated.format("%Y").to_string(), "2019");
    assert_eq!(
        db.aircraft_find_by_reg("D-AIBD").unwrap().unwrap().hex,
        "3C6444"
    );
    let t = db.aircraft_type_find_by_icao("A388").unwrap().unwrap();
    assert_eq!(t.category, PlaneCategory::Superheavy);
    assert_eq!(db.airline_find_by_icao("DLH").unwrap().unwrap().iata, "LH");
    assert_eq!(db.airline_find_by_iata("LH").unwrap().unwrap().icao, "DLH");
    let ap = db.airport_find_by_icao("EDDF").unwrap().unwrap();
    assert!((ap.lat - 50.0).abs() < 0.1);
    assert!(db.aircraft_find_by_hex("FFFFFF").unwrap().is_none());
}

#[test]
fn find_by_call_returns_newest() {
    let db = loaded();
    // DLH459 exists twice: 3C65A7 (2019) and 3C0001 (2010)
    assert_eq!(
        db.aircraft_find_by_call("DLH459").unwrap().unwrap().hex,
        "3C65A7"
    );
}

#[test]
fn insert_if_newer_keeps_newer() {
    let db = AircraftDb::open_in_memory().unwrap();
    let t1 = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let t0 = Utc.with_ymd_and_hms(2019, 1, 1, 0, 0, 0).unwrap();
    let mk = |call: &str, t| Aircraft {
        hex: "ABCDEF".into(),
        call: call.into(),
        reg: "D-ABCD".into(),
        type_code: "A320".into(),
        last_updated: t,
    };
    assert!(db
        .aircraft_insert_or_update_if_newer(&mk("NEW", t1))
        .unwrap());
    assert!(!db
        .aircraft_insert_or_update_if_newer(&mk("OLD", t0))
        .unwrap());
    assert!(!db
        .aircraft_insert_or_update_if_newer(&mk("SAME", t1))
        .unwrap());
    assert_eq!(
        db.aircraft_find_by_hex("ABCDEF").unwrap().unwrap().call,
        "NEW"
    );
    assert_eq!(db.aircraft_count().unwrap(), 1);
}

#[test]
fn registration_find() {
    let db = loaded();
    let f = |r: &str| db.registration_find_by_reg(r).unwrap().map(|x| x.prefix);
    assert_eq!(f("D-AIZZ").as_deref(), Some("D"));
    assert_eq!(f("d-aizz").as_deref(), Some("D"));
    assert_eq!(f("N12345").as_deref(), Some("N"));
    assert_eq!(f("HB-JZA").as_deref(), Some("HB"));
    assert_eq!(f("NABC"), None);
    assert_eq!(f("-ABC"), None);
    assert_eq!(f(""), None);
    assert_eq!(f("XX-ABC"), None);
}

/// C# throws (`Substring(0, -1)` / `reg[1]`); the port returns None (ruling, logic-findings).
#[test]
fn registration_without_dash_is_none_not_panic() {
    let db = loaded();
    assert_eq!(db.registration_find_by_reg("DAIZZ").unwrap(), None);
    assert_eq!(db.registration_find_by_reg("N").unwrap(), None);
}

#[test]
fn sql_injection_inert() {
    let db = loaded();
    assert!(db.aircraft_find_by_call("x' OR '1'='1").unwrap().is_none());
    assert!(db.airline_find_by_icao("x' OR '1'='1").unwrap().is_none());
}

#[test]
fn file_db_persists() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = AircraftDb::open(dir.path()).unwrap();
        db.airline_bulk_insert_or_update_if_newer(
            &airlines_from_json(&fixture("Airlines")).unwrap(),
        )
        .unwrap();
    }
    assert!(dir.path().join("aircrafts.db3").exists());
    let db = AircraftDb::open(dir.path()).unwrap();
    assert_eq!(db.airline_count().unwrap(), 10);
}

/// `PlaneInfoBulkInsertOrUpdateIfNewer`: typed feed rows keep `IATA = ""`, but lookup prefers
/// populated-IATA reference rows for the same ICAO (#18).
#[test]
fn finding_18_aircraft_type_lookup_prefers_populated_iata() {
    use as_aircraft::PlaneInfo;
    let db = AircraftDb::open_in_memory().unwrap();
    let t0 = Utc.with_ymd_and_hms(2023, 12, 31, 23, 59, 0).unwrap();
    let t = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    db.aircraft_type_insert_or_update_if_newer(&AircraftType {
        icao: "A388".into(),
        iata: "388".into(),
        manufacturer: "Airbus".into(),
        model: "A380 861".into(),
        category: PlaneCategory::Superheavy,
        last_updated: t0,
    })
    .unwrap();
    let full = PlaneInfo::new(
        t,
        "DLH1A",
        "D-AIZZ",
        "3CFFFF",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "A388",
        "FeedMaker",
        "FeedModel",
        PlaneCategory::None,
    );
    let noreg = PlaneInfo::new(
        t,
        "BAW1",
        "",
        "400000",
        50.0,
        8.0,
        90.0,
        35000.0,
        450.0,
        "B744",
        "Boeing",
        "747",
        PlaneCategory::Heavy,
    );
    db.plane_info_bulk_upsert(&[full, noreg]).unwrap();
    assert_eq!(
        db.aircraft_find_by_hex("3CFFFF")
            .unwrap()
            .unwrap()
            .type_code,
        "A388"
    );
    assert!(db.aircraft_find_by_hex("400000").unwrap().is_none());
    let a388 = db.aircraft_type_find_by_icao("A388").unwrap().unwrap();
    assert_eq!(
        (a388.iata.as_str(), a388.category),
        ("388", PlaneCategory::Superheavy)
    );
    assert_eq!(
        db.aircraft_type_find_by_icao("B744")
            .unwrap()
            .unwrap()
            .model,
        "747"
    );
}

#[test]
fn aircraft_db_is_a_lookup() {
    use as_aircraft::check::Lookup;
    let db = loaded();
    assert!(db.airline_by_icao("DLH"));
    assert!(db.registration_by_reg("D-AIZZ"));
    assert!(!db.registration_by_reg("QQ-AIZZ"));
    assert!(Lookup::type_icao_min_len(&db) == 4);
}

#[test]
fn airport_get_all_in_table_order() {
    let db = AircraftDb::open_in_memory().unwrap();
    assert!(db.airport_get_all().unwrap().is_empty());
    let airports = airports_from_json(&fixture("Airports")).unwrap();
    db.airport_bulk_insert_or_update_if_newer(&airports)
        .unwrap();
    assert_eq!(db.airport_get_all().unwrap(), airports);
}

#[test]
fn finding_372_aircraft_db_guards_schema_version() {
    let mem = AircraftDb::open_in_memory().unwrap();
    let ver: i64 = mem.with_conn(|c| {
        c.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(ver, 1);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aircrafts.db3");
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE FutureAircraft (Hex TEXT PRIMARY KEY);
         INSERT INTO FutureAircraft VALUES ('3C0001');
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let err = AircraftDb::open(dir.path()).err().unwrap();
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
}
