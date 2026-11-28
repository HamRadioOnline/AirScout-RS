use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_stations::{
    default_antenna_gain, default_antenna_height, default_power, GeoSource, Location, Qrv,
    StationDb,
};
use chrono::{DateTime, Duration, TimeZone, Utc};

fn t(s: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + s, 0).unwrap()
}

fn loc(call: &str, l: &str, lat: f64, lon: f64, hits: i32, s: i64) -> Location {
    Location::new(call, l, lat, lon, GeoSource::FromUser, hits, t(s))
}

#[test]
fn location_roundtrip_and_normalised_call() {
    let db = StationDb::open_in_memory().unwrap();
    let l = loc(" dl2alf ", "JO50WC", 50.1, 11.9, 3, 0);
    assert_eq!(l.call, "DL2ALF");
    assert!(db.location_insert_or_update_if_newer(&l).unwrap());
    let f = db
        .location_find("DL2ALF", Some("JO50WC"))
        .unwrap()
        .expect("found");
    assert_eq!(f, l);
    assert_eq!(db.location_count().unwrap(), 1);
    assert!(db
        .location_find("DL2ALF", Some("JO50WD"))
        .unwrap()
        .is_none());
    assert!(db.location_find("", None).unwrap().is_none());
}

#[test]
fn insert_older_is_ignored() {
    let db = StationDb::open_in_memory().unwrap();
    db.location_insert_or_update_if_newer(&loc("DL2ALF", "JO50WC", 50.1, 11.9, 0, 10))
        .unwrap();
    // same key, older -> ignored
    assert!(!db
        .location_insert_or_update_if_newer(&loc("DL2ALF", "JO50WC", 1.0, 1.0, 0, 5))
        .unwrap());
    // same timestamp -> ignored (C#: dt < new)
    assert!(!db
        .location_insert_or_update_if_newer(&loc("DL2ALF", "JO50WC", 2.0, 2.0, 0, 10))
        .unwrap());
    assert_eq!(
        db.location_find("DL2ALF", Some("JO50WC"))
            .unwrap()
            .unwrap()
            .lat,
        50.1
    );
    // newer -> updated
    assert!(db
        .location_insert_or_update_if_newer(&loc("DL2ALF", "JO50WC", 50.2, 11.8, 0, 11))
        .unwrap());
    assert_eq!(
        db.location_find("DL2ALF", Some("JO50WC"))
            .unwrap()
            .unwrap()
            .lat,
        50.2
    );
}

/// C# `LocationFind(call)` returns the most recently updated entry (not the most hit one).
#[test]
fn location_find_without_loc_is_last_recent() {
    let db = StationDb::open_in_memory().unwrap();
    db.location_insert_or_update_if_newer(&loc("DL2ALF", "JO50WC", 50.1, 11.9, 99, 0))
        .unwrap();
    db.location_insert_or_update_if_newer(&loc("DL2ALF", "JO61AA", 51.0, 12.0, 1, 20))
        .unwrap();
    db.location_insert_or_update_if_newer(&loc("DL2ALF", "JN59AA", 49.0, 10.0, 5, 10))
        .unwrap();
    assert_eq!(
        db.location_find("DL2ALF", None).unwrap().unwrap().loc,
        "JO61AA"
    );
    let all: Vec<_> = db
        .locations_by_call("DL2ALF")
        .unwrap()
        .into_iter()
        .map(|l| l.loc)
        .collect();
    assert_eq!(all, ["JO61AA", "JN59AA", "JO50WC"]);
    assert!(db.locations_by_call("DL9XXX").unwrap().is_empty());
}

#[test]
fn bulk_insert_is_one_transaction_and_counts() {
    let db = StationDb::open_in_memory().unwrap();
    let ls: Vec<_> = (0..50)
        .map(|i| loc(&format!("DL{i}ABC"), "JO50WC", 50.0, 11.0, 0, i))
        .collect();
    assert_eq!(db.location_bulk_insert_or_update_if_newer(&ls).unwrap(), 50);
    assert_eq!(db.location_count().unwrap(), 50);
    assert_eq!(db.location_bulk_insert_or_update_if_newer(&ls).unwrap(), 0);
}

#[test]
fn qrv_per_band() {
    let db = StationDb::open_in_memory().unwrap();
    let a = Qrv::new("dl2alf", "jo50wc", Band::B144M, 20.0, 15.0, 500.0, t(0)).unwrap();
    let b = Qrv::new("DL2ALF", "JO50WC", Band::B10G, 12.0, 30.0, 10.0, t(0)).unwrap();
    assert_eq!(a.call, "DL2ALF");
    assert_eq!(a.loc, "JO50WC");
    db.qrv_insert_or_update_if_newer(&a).unwrap();
    db.qrv_insert_or_update_if_newer(&b).unwrap();
    assert_eq!(
        db.qrv_find("DL2ALF", "JO50WC", Band::B144M)
            .unwrap()
            .unwrap(),
        a
    );
    assert_eq!(
        db.qrv_find("DL2ALF", "JO50WC", Band::B10G)
            .unwrap()
            .unwrap(),
        b
    );
    assert!(db
        .qrv_find("DL2ALF", "JO50WC", Band::B432M)
        .unwrap()
        .is_none());
    let mut all = db.qrv_get_all("DL2ALF", "JO50WC").unwrap();
    all.sort_by_key(|q| q.band);
    assert_eq!(all, vec![a.clone(), b]);
    // older update ignored, newer applied
    let older = Qrv::new("DL2ALF", "JO50WC", Band::B144M, 1.0, 1.0, 1.0, t(-1)).unwrap();
    assert!(!db.qrv_insert_or_update_if_newer(&older).unwrap());
    let newer = Qrv::new(
        "DL2ALF",
        "JO50WC",
        Band::B144M,
        30.0,
        16.0,
        750.0,
        t(0) + Duration::seconds(1),
    )
    .unwrap();
    assert!(db.qrv_insert_or_update_if_newer(&newer).unwrap());
    assert_eq!(
        db.qrv_find("DL2ALF", "JO50WC", Band::B144M)
            .unwrap()
            .unwrap(),
        newer
    );
}

/// C# `QRVDesignator` does `loc.ToUpper().Trim().Substring(0, 6)`: longer locators are cut,
/// shorter ones throw.
#[test]
fn qrv_locator_is_cut_to_six_chars() {
    let q = Qrv::new("DL2ALF", "JO50WC12", Band::B144M, 0.0, 0.0, 0.0, t(0)).unwrap();
    assert_eq!(q.loc, "JO50WC");
    assert!(Qrv::new("DL2ALF", "JO50", Band::B144M, 0.0, 0.0, 0.0, t(0)).is_err());
}

#[test]
fn qrv_defaults_like_original() {
    let db = StationDb::open_in_memory().unwrap();
    let d = db
        .qrv_find_or_create_default("DL2ALF", "JO50WC", Band::B24G, None)
        .unwrap();
    assert_eq!(
        (d.antenna_height, d.antenna_gain, d.power),
        (10.0, 40.0, 0.2)
    );
    assert_eq!(as_stations::default_antenna_gain(Band::B144M), 12.0);
    assert_eq!(as_stations::default_power(Band::B1_2G), 50.0);
    assert_eq!(as_stations::default_power(Band::None), 0.0);
}

/// #13: a coarse locator is a valid Maidenhead locator but can never match a stored
/// six-character QRV key. Only the exact six-character locator, or a trusted position inside the
/// coarse cell, selects the stored row; a coarse locator alone uses the band defaults, and a
/// malformed locator or position cannot select a stored row.
#[test]
fn qrv_coarse_locator_resolution() {
    let db = StationDb::open_in_memory().unwrap();
    let row = Qrv::new("TE1ST", "JO50AA", Band::B144M, 20.0, 15.0, 500.0, t(0)).unwrap();
    db.qrv_insert_or_update_if_newer(&row).unwrap();

    // the exact six-character locator matches the stored row
    assert_eq!(
        db.qrv_find("TE1ST", "JO50AA", Band::B144M).unwrap(),
        Some(row.clone())
    );
    assert_eq!(
        db.qrv_find_or_create_default("TE1ST", "JO50AA", Band::B144M, None)
            .unwrap(),
        row
    );

    // a valid coarse locator is no match, not an error
    assert!(db.qrv_find("TE1ST", "JO50", Band::B144M).unwrap().is_none());
    assert!(db.qrv_get_all("TE1ST", "JO50").unwrap().is_empty());

    // a trusted position inside JO50AA derives the six-character key and matches the row
    let inside = maidenhead::gpoint_from_loc("JO50AA", PositionInRectangle::MiddleMiddle).unwrap();
    let via = db
        .qrv_find_or_create_default("TE1ST", "JO50", Band::B144M, Some(inside))
        .unwrap();
    assert_eq!(via, row.clone());

    // the coarse locator alone uses the band defaults even with the row present, and stores nothing
    let before = db.qrv_count().unwrap();
    let d = db
        .qrv_find_or_create_default("TE1ST", "JO50", Band::B144M, None)
        .unwrap();
    assert_eq!(d.loc, "JO50");
    assert_eq!(
        (d.antenna_height, d.antenna_gain, d.power),
        (
            default_antenna_height(Band::B144M),
            default_antenna_gain(Band::B144M),
            default_power(Band::B144M)
        )
    );
    assert_eq!(db.qrv_count().unwrap(), before);

    // a position outside the coarse cell cannot select a stored row
    let outside = maidenhead::gpoint_from_loc("JO51AA", PositionInRectangle::MiddleMiddle).unwrap();
    let d = db
        .qrv_find_or_create_default("TE1ST", "JO50", Band::B144M, Some(outside))
        .unwrap();
    assert_eq!(d.antenna_height, default_antenna_height(Band::B144M));

    // malformed locator text cannot select a stored row
    assert!(db.qrv_find("TE1ST", "JO50AAXX", Band::B144M).is_err());
    assert!(db
        .qrv_find_or_create_default("TE1ST", "JO50AAXX", Band::B144M, None)
        .is_err());

    // a malformed trusted position cannot select a stored row
    let d = db
        .qrv_find_or_create_default(
            "TE1ST",
            "JO50",
            Band::B144M,
            Some(GPoint::new(f64::NAN, 10.0)),
        )
        .unwrap();
    assert_eq!(d.antenna_height, default_antenna_height(Band::B144M));
}

#[test]
fn location_json_like_server_file() {
    let json = r#"[{"Elevation":0,"BestCaseElevation":false,"Call":"DA0TST","Loc":"JO50WC",
        "Lat":50.104167938232422,"Lon":11.958333253860474,"Source":1,"Hits":0,
        "LastUpdated":"2013-05-12T19:18:12Z"}]"#;
    let l = as_stations::locations_from_json(json).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].call, "DA0TST");
    assert_eq!(l[0].source, GeoSource::FromLoc);
    assert_eq!(
        l[0].last_updated,
        Utc.with_ymd_and_hms(2013, 5, 12, 19, 18, 12).unwrap()
    );
    let q = as_stations::qrvs_from_json(
        r#"[{"Call":"DA0TST","Loc":"JO50WC","Band":1296,
        "AntennaHeight":0.0,"AntennaGain":0.0,"Power":0.0,"LastUpdated":"2010-09-04T00:00:00Z"}]"#,
    )
    .unwrap();
    assert_eq!(q[0].band, Band::B1_2G);
}

#[test]
fn table_timestamps_roundtrip() {
    let db = StationDb::open_in_memory().unwrap();
    assert_eq!(db.table_timestamp("Location").unwrap(), None);
    db.set_table_timestamp("Location", t(5)).unwrap();
    assert_eq!(db.table_timestamp("Location").unwrap(), Some(t(5)));
}

/// #385: the three station-import bookkeeping tables are internal and a clear is refused, so
/// the maintenance dialog cannot wipe the import state by mistake.
#[test]
fn clear_internal_table_is_refused() {
    let db = StationDb::open_in_memory().unwrap();
    for table in [
        "StationImportState",
        "StationArchiveValidators",
        "StationImport",
    ] {
        let err = db.clear_table(table).unwrap_err();
        assert!(err.to_string().contains(table), "{table}: {err}");
    }
    // the user tables still clear
    db.location_insert_or_update_if_newer(&loc("TE1ST", "JO50AA", 50.0, 8.0, 0, 0))
        .unwrap();
    assert_eq!(db.clear_table("Location").unwrap(), 1);
    assert_eq!(db.location_count().unwrap(), 0);
}

#[test]
fn finding_372_station_db_guards_schema_version() {
    let mem = StationDb::open_in_memory().unwrap();
    let ver: i64 = mem.with_conn(|c| {
        c.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(ver, 2);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stations.db3");
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE Location (Call TEXT PRIMARY KEY);
         INSERT INTO Location VALUES ('TE1ST');
         PRAGMA user_version = 3;",
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let err = StationDb::open(&path).err().unwrap();
    assert!(
        matches!(
            err,
            as_stations::StationError::SchemaAhead {
                actual: 3,
                supported: 2
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
