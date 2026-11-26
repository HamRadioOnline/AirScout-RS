mod common;
use as_elevation::{DbStatus, ElevationError, ElevationModel, ElevationStore, TILE_MISSING};

fn store_with_fixtures() -> (tempfile::TempDir, ElevationStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    for ((model, _), t) in common::fixture_tiles() {
        store.insert_or_update_if_newer(model, &t).unwrap();
    }
    (dir, store)
}

#[test]
fn store_roundtrip() {
    let (_d, store) = store_with_fixtures();
    let tiles = common::fixture_tiles();
    let (key, t) = tiles.iter().next().unwrap();
    let back = store.find(key.0, &t.index).unwrap().expect("tile stored");
    assert_eq!(back, *t);
    assert_eq!(store.count(ElevationModel::Globe).unwrap(), 4 * 576);
    assert_eq!(store.count(ElevationModel::Srtm3).unwrap(), 1);
    assert!(store
        .find(ElevationModel::Globe, "AA00AA")
        .unwrap()
        .is_none());
}

#[test]
fn insert_older_is_ignored_newer_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let tiles = common::fixture_tiles();
    let t = tiles.values().next().unwrap().clone();
    store
        .insert_or_update_if_newer(ElevationModel::Globe, &t)
        .unwrap();
    let mut older = t.clone();
    older.last_updated -= chrono::Duration::days(1);
    older.elv[0] = 1234;
    store
        .insert_or_update_if_newer(ElevationModel::Globe, &older)
        .unwrap();
    assert_eq!(
        store
            .find(ElevationModel::Globe, &t.index)
            .unwrap()
            .unwrap()
            .elv[0],
        t.elv[0]
    );
    let mut newer = t.clone();
    newer.last_updated += chrono::Duration::days(1);
    newer.elv[0] = 4321;
    store
        .insert_or_update_if_newer(ElevationModel::Globe, &newer)
        .unwrap();
    assert_eq!(
        store
            .find(ElevationModel::Globe, &t.index)
            .unwrap()
            .unwrap()
            .elv[0],
        4321
    );
}

#[test]
fn elevation_matches_tile_golden() {
    let (_d, store) = store_with_fixtures();
    let g = as_golden::load("elevation_tile.json");
    let mut n = 0;
    for c in g.by_fn("get_elevation") {
        let model = common::model_from(&c.s("model"));
        let (lat, lon) = (c.f("lat"), c.f("lon"));
        let expected = c.result.as_i64().unwrap() as i16;
        // The store picks the tile by locator; only check points that belong to the golden's tile.
        let loc = as_geo::maidenhead::loc_from_lat_lon(lat, lon, false, 3, false)
            .unwrap()
            .unwrap();
        if loc != c.s("tile") {
            continue;
        }
        let r = store.elevation(model, lat, lon, false).unwrap();
        assert_eq!(r, expected, "{c:?}");
        n += 1;
    }
    assert!(n > 5000, "{n}");
}

#[test]
fn missing_tile_returns_tile_missing_or_zero() {
    let (_d, store) = store_with_fixtures();
    // JN58 is not in the fixture set
    assert_eq!(
        store
            .elevation(ElevationModel::Globe, 48.1, 11.5, false)
            .unwrap(),
        TILE_MISSING
    );
    assert_eq!(
        store
            .elevation(ElevationModel::Globe, 48.1, 11.5, true)
            .unwrap(),
        0
    );
}

#[test]
fn invalid_coordinate_is_err() {
    let (_d, store) = store_with_fixtures();
    for (lat, lon) in [
        (90.0001, 0.0),
        (-90.0001, 0.0),
        (0.0, 180.0001),
        (0.0, -180.0001),
        (f64::NAN, 0.0),
    ] {
        let r = store.elevation(ElevationModel::Globe, lat, lon, true);
        assert!(
            matches!(r, Err(ElevationError::InvalidCoordinate { .. })),
            "{lat},{lon}: {r:?}"
        );
    }
}

#[test]
fn status_bits() {
    let (_d, store) = store_with_fixtures();
    assert!(!store.status_bit(ElevationModel::Globe, DbStatus::COMPLETE));
    store.set_status_bit(ElevationModel::Globe, DbStatus::COMPLETE);
    assert!(store.status_bit(ElevationModel::Globe, DbStatus::COMPLETE));
    assert!(!store.status_bit(ElevationModel::Srtm3, DbStatus::COMPLETE));
    store.reset_status_bit(ElevationModel::Globe, DbStatus::COMPLETE);
    assert!(!store.status_bit(ElevationModel::Globe, DbStatus::COMPLETE));
}

#[test]
fn finding_5_cache_keeps_the_two_most_recent_insertions() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open_with_cache_capacity(dir.path(), 2).unwrap();
    let model = ElevationModel::Globe;
    for (index, value) in [("JO50AA", 1), ("JO51AA", 2), ("JO60AA", 3), ("JO61AA", 4)] {
        store
            .insert_or_update_if_newer(model, &constant_tile(index, value))
            .unwrap();
    }
    let points = [
        (50.01, 10.01),
        (51.01, 10.01),
        (50.01, 12.01),
        (51.01, 12.01),
    ];
    for ((lat, lon), expected) in points.into_iter().zip(1..=4) {
        assert_eq!(store.elevation(model, lat, lon, false).unwrap(), expected);
    }
    assert_eq!(store.cache_len(model), 2);
    // Remove the backing rows to distinguish a cache hit from a fresh read.
    store
        .with_conn(model, |conn| {
            conn.execute("DELETE FROM Elevation", [])?;
            Ok(())
        })
        .unwrap();
    for ((lat, lon), expected) in points.into_iter().zip([TILE_MISSING, TILE_MISSING, 3, 4]) {
        assert_eq!(store.elevation(model, lat, lon, false).unwrap(), expected);
    }
}

fn constant_tile(index: &str, value: i16) -> as_elevation::ElevationTile {
    as_elevation::ElevationTile::new(
        index,
        value.into(),
        0.0,
        0.0,
        value.into(),
        0.0,
        0.0,
        5,
        10,
        vec![value; 50],
        chrono::DateTime::UNIX_EPOCH,
    )
    .unwrap()
}

#[test]
fn finding_10_shared_corner_is_independent_of_the_previous_tile() {
    let (_d, store) = store_with_fixtures();
    let model = ElevationModel::Globe;
    for (lat, lon) in [(50.49, 10.99), (50.51, 10.99), (50.49, 11.01)] {
        assert!(store.elevation(model, lat, lon, false).unwrap() > TILE_MISSING);
        assert_eq!(store.elevation(model, 50.5, 11.0, false).unwrap(), 706);
    }
}

#[test]
fn finding_10_pole_and_antimeridian_have_a_sample() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let model = ElevationModel::Globe;
    store
        .insert_or_update_if_newer(model, &constant_tile("AR09AX", 123))
        .unwrap();
    assert_eq!(store.elevation(model, 90.0, 180.0, false).unwrap(), 123);
    assert_eq!(store.elevation(model, 90.0, -180.0, false).unwrap(), 123);
}

#[test]
fn store_is_send_and_sync() {
    fn assert_ss<T: Send + Sync>() {}
    assert_ss::<ElevationStore>();
}

#[test]
fn min_max_info_reads_tile_extremes_without_the_grid() {
    let (_d, store) = store_with_fixtures();
    let tiles = common::fixture_tiles();
    let ((model, _), t) = tiles.iter().next().unwrap();
    let info = store.min_max_info(*model, &t.index).unwrap().unwrap();
    assert_eq!(info.min_elv, t.min_elv);
    assert_eq!(info.max_elv, t.max_elv);
    assert_eq!((info.max_lat, info.max_lon), (t.max_lat, t.max_lon));
    assert_eq!((info.min_lat, info.min_lon), (t.min_lat, t.min_lon));
    // GetMaxElvLoc: case-insensitive, 6 characters, stored tiles only
    let lower = t.index.to_lowercase();
    assert_eq!(store.max_elv_loc(*model, &lower).unwrap(), Some(info));
    assert!(store.max_elv_loc(*model, &t.index[..4]).unwrap().is_none());
    assert!(store.max_elv_loc(*model, "ZZ99ZZ").unwrap().is_none());
    assert!(store.max_elv_loc(*model, "AA00AA").unwrap().is_none());
    assert!(store.min_max_info(*model, "AA00AA").unwrap().is_none());
}

#[test]
fn finding_372_elevation_store_guards_schema_version_and_migrates_older() {
    let ahead_dir = tempfile::tempdir().unwrap();
    let path = ahead_dir.path().join(ElevationModel::Globe.db_file_name());
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE Elevation (TileIndex TEXT PRIMARY KEY);
         INSERT INTO Elevation VALUES ('JO50AA');
         PRAGMA user_version = 5;",
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let err = ElevationStore::open(ahead_dir.path()).err().unwrap();
    assert!(
        matches!(
            err,
            as_elevation::ElevationError::SchemaAhead {
                actual: 5,
                supported: 4
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

    // Migrate from each older version (1, 2, 3) -> 4.
    for start_ver in 1..=3i64 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ElevationModel::Globe.db_file_name());
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch(
            "CREATE TABLE Elevation (
                TileIndex TEXT NOT NULL PRIMARY KEY,
                MinElv INTEGER, MinLat REAL, MinLon REAL,
                MaxElv INTEGER, MaxLat REAL, MaxLon REAL,
                Rows INTEGER NOT NULL, Columns INTEGER NOT NULL,
                Elv BLOB NOT NULL, LastUpdated INTEGER NOT NULL);",
        )
        .unwrap();
        if start_ver >= 2 {
            raw.execute_batch(
                "CREATE TABLE ElevationPath (
                    Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, Lat2 REAL NOT NULL, Lon2 REAL NOT NULL,
                    StepWidth REAL NOT NULL, Count INTEGER NOT NULL, Path BLOB NOT NULL,
                    LastUpdated INTEGER NOT NULL,
                    PRIMARY KEY (Lat1, Lon1, Lat2, Lon2, StepWidth));",
            )
            .unwrap();
        }
        if start_ver >= 3 {
            raw.execute_batch(
                "CREATE TABLE LocalObstruction (
                    Lat REAL NOT NULL, Lon REAL NOT NULL, Distance BLOB NOT NULL,
                    Height BLOB NOT NULL, LastUpdated INTEGER NOT NULL,
                    PRIMARY KEY (Lat, Lon));",
            )
            .unwrap();
        }
        raw.pragma_update(None, "user_version", start_ver).unwrap();
        drop(raw);

        let store = ElevationStore::open(dir.path()).unwrap();
        let ver: i64 = store
            .with_conn(ElevationModel::Globe, |c| {
                Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(ver, 4);
    }
}
