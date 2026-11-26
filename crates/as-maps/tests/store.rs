//! The GMap `.gmdb` tile store: schema, round trip, foreign and damaged files.

use as_maps::{is_png_or_jpeg, MapsError, TileInfo, TileKey, TileStore, MAX_TILE_BYTES};
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{params, Connection};
use std::path::Path;

/// `MapDatabase.cs:110` (`TilesCreateTable`), verbatim.
const TILES_DDL: &str = "CREATE TABLE Tiles (id INTEGER NOT NULL PRIMARY KEY, X INTEGER NOT NULL, Y INTEGER NOT NULL, Zoom INTEGER NOT NULL, Type UNSIGNED INTEGER  NOT NULL, CacheTime DATETIME)";
/// `MapDatabase.cs:124` (`TilesDataCreateTable`), verbatim.
const TILES_DATA_DDL: &str = "CREATE TABLE TilesData (id INTEGER NOT NULL PRIMARY KEY CONSTRAINT fk_Tiles_id REFERENCES Tiles(id) ON DELETE CASCADE, Tile BLOB NULL)";

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-synthetic-tile";

fn key(x: u32, y: u32, zoom: u8, db_id: i32) -> TileKey {
    TileKey { x, y, zoom, db_id }
}

fn dt(h: u32, m: u32, s: u32, nano: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2019, 12, 14)
        .unwrap()
        .and_hms_nano_opt(h, m, s, nano)
        .unwrap()
}

/// A database as the C# MapDatabase creates it (no index, rollback journal).
fn csharp_db(path: &Path) -> Connection {
    let c = Connection::open(path).unwrap();
    c.execute_batch(&format!("{TILES_DDL}; {TILES_DATA_DDL};"))
        .unwrap();
    c
}

fn table_sql(c: &Connection, name: &str) -> Option<String> {
    c.query_row(
        "SELECT sql FROM sqlite_master WHERE type IN ('table', 'index') AND name = ?1",
        [name],
        |r| r.get(0),
    )
    .ok()
}

#[test]
fn store_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<TileStore>();
}

#[test]
fn open_creates_gmap_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("maptiles.gmdb");
    let store = TileStore::open(&path).unwrap();
    assert_eq!(store.count().unwrap(), 0);
    drop(store);
    let c = Connection::open(&path).unwrap();
    assert_eq!(table_sql(&c, "Tiles").as_deref(), Some(TILES_DDL));
    assert_eq!(table_sql(&c, "TilesData").as_deref(), Some(TILES_DATA_DDL));
    assert_eq!(
        table_sql(&c, "IndexOfTiles").as_deref(),
        Some("CREATE INDEX IndexOfTiles ON Tiles (X, Y, Zoom, Type)")
    );
    let mode: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    // Opening again keeps the schema.
    TileStore::open(&path).unwrap();
    TileStore::open_read_only(&path).unwrap();
}

#[test]
fn open_accepts_an_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.gmdb");
    std::fs::write(&path, b"").unwrap();
    let store = TileStore::open(&path).unwrap();
    assert!(store.insert(key(1, 2, 3, 7), None, PNG).unwrap());
    drop(store);
    let c = Connection::open(&path).unwrap();
    let mode: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

fn wal_sidecar(path: &Path) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push("-wal");
    p.into()
}

/// An existing GMap file (such as a C# `Data.gmdb` given as `import --db`) keeps its rollback
/// journal: GMap left WAL off (`SQLitePureImageCache.cs:176-180`).
#[test]
fn open_keeps_the_journal_mode_of_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("csharp.gmdb");
    drop(csharp_db(&path));
    let store = TileStore::open(&path).unwrap();
    assert!(store.insert(key(1, 2, 3, 7), None, PNG).unwrap());
    let c = Connection::open(&path).unwrap();
    let mode: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_ne!(mode, "wal");
    drop(c);
    drop(store);
    assert!(!wal_sidecar(&path).exists());
    assert_eq!(
        TileStore::open_read_only(&path).unwrap().count().unwrap(),
        1
    );
}

/// `Tiles` and `TilesData` are written in one transaction: a failing `TilesData` insert leaves
/// no `Tiles` row behind.
#[test]
fn insert_is_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    let store = TileStore::open(&path).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER no_data BEFORE INSERT ON TilesData BEGIN SELECT RAISE(ABORT, 'x'); END",
        )
        .unwrap();
    let k = key(1, 2, 3, 7);
    assert!(store.insert(k, None, PNG).is_err());
    assert_eq!(store.count().unwrap(), 0);
    assert!(!store.exists(k).unwrap());
}

#[test]
fn insert_get_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = TileStore::open(&dir.path().join("t.gmdb")).unwrap();
    let k = key(33, 21, 6, 1_492_776_782);
    assert!(!store.exists(k).unwrap());
    assert_eq!(store.get(k).unwrap(), None);
    let t = dt(10, 11, 12, 123_456_700);
    assert!(store.insert(k, Some(t), PNG).unwrap());
    assert!(store.exists(k).unwrap());
    assert_eq!(store.get(k).unwrap().as_deref(), Some(PNG));
    // A second insert of the same key writes nothing.
    assert!(!store.insert(k, Some(t), b"\xff\xd8\xffother").unwrap());
    assert_eq!(store.count().unwrap(), 1);
    assert_eq!(store.get(k).unwrap().as_deref(), Some(PNG));

    let list = store.list().unwrap();
    assert_eq!(
        list,
        vec![TileInfo {
            id: 1,
            key: k,
            cache_time: Some(t)
        }]
    );
    assert_eq!(store.get_by_id(1).unwrap().as_deref(), Some(PNG));
    assert_eq!(store.get_by_id(2).unwrap(), None);
    // Other keys differ in each field.
    for other in [
        key(34, 21, 6, k.db_id),
        key(33, 22, 6, k.db_id),
        key(33, 21, 7, k.db_id),
        key(33, 21, 6, 1),
    ] {
        assert!(!store.exists(other).unwrap(), "{other:?}");
    }
    // The id follows MAX(id)+1.
    assert!(store.insert(key(1, 1, 1, 1), None, PNG).unwrap());
    assert_eq!(store.list().unwrap()[1].id, 2);
}

#[test]
fn insert_writes_csharp_cache_time_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    let store = TileStore::open(&path).unwrap();
    assert!(store
        .insert(key(1, 1, 5, 9), Some(dt(10, 11, 12, 123_456_789)), PNG)
        .unwrap());
    assert!(store
        .insert(key(2, 1, 5, 9), Some(dt(0, 0, 0, 0)), PNG)
        .unwrap());
    assert!(store.insert(key(3, 1, 5, 9), None, PNG).unwrap());
    let c = Connection::open(&path).unwrap();
    let mut st = c
        .prepare("SELECT CacheTime FROM Tiles ORDER BY id")
        .unwrap();
    let texts: Vec<Option<String>> = st
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        texts,
        vec![
            Some("2019-12-14 10:11:12.1234567".to_string()),
            Some("2019-12-14 00:00:00.0000000".to_string()),
            None
        ]
    );
}

#[test]
fn list_and_types() {
    let dir = tempfile::tempdir().unwrap();
    let store = TileStore::open(&dir.path().join("t.gmdb")).unwrap();
    for x in 0..3 {
        assert!(store.insert(key(x, 0, 5, 100), None, PNG).unwrap());
    }
    assert!(store.insert(key(0, 0, 5, 7), None, PNG).unwrap());
    assert_eq!(store.count().unwrap(), 4);
    assert_eq!(store.types().unwrap(), vec![(7, 1), (100, 3)]);
    let ids: Vec<i64> = store.list().unwrap().iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn open_refuses_foreign_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foreign.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE foo (a INTEGER); INSERT INTO foo VALUES (1);")
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let names = |d: &Path| {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        v.sort();
        v
    };
    let files_before = names(dir.path());
    assert!(matches!(TileStore::open(&path), Err(MapsError::NotAGmdb(p)) if p == path));
    assert!(matches!(TileStore::open_read_only(&path), Err(MapsError::NotAGmdb(p)) if p == path));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(names(dir.path()), files_before);
}

#[test]
fn open_refuses_half_a_gmdb() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("half.gmdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(TILES_DDL)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        TileStore::open(&path),
        Err(MapsError::NotAGmdb(_))
    ));
    assert!(matches!(
        TileStore::open_read_only(&path),
        Err(MapsError::NotAGmdb(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn open_refuses_a_file_that_is_not_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.gmdb");
    let text = b"this is not a database, it is a synthetic text file of some length..".repeat(20);
    std::fs::write(&path, &text).unwrap();
    let e = TileStore::open(&path).expect_err("error");
    assert!(matches!(e, MapsError::NotAGmdb(_)), "{e}");
    assert!(e.to_string().contains("text.gmdb"), "{e}");
    assert!(matches!(
        TileStore::open_read_only(&path),
        Err(MapsError::NotAGmdb(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), text);
}

#[test]
fn open_read_only_never_creates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.gmdb");
    let e = TileStore::open_read_only(&path).expect_err("error");
    assert!(e.to_string().contains("missing.gmdb"), "{e}");
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn open_read_only_refuses_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    TileStore::open(&path)
        .unwrap()
        .insert(key(1, 1, 1, 1), None, PNG)
        .unwrap();
    let ro = TileStore::open_read_only(&path).unwrap();
    assert_eq!(ro.get(key(1, 1, 1, 1)).unwrap().as_deref(), Some(PNG));
    assert!(matches!(
        ro.insert(key(2, 1, 1, 1), None, PNG),
        Err(MapsError::Sql { .. })
    ));
}

#[test]
fn get_refuses_oversized_blob() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.gmdb");
    let c = csharp_db(&path);
    let len = MAX_TILE_BYTES + 1;
    c.execute(
        "INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (5, 1, 2, 3, 4, NULL)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO TilesData (id, Tile) VALUES (5, zeroblob(?1))",
        [len as i64],
    )
    .unwrap();
    // Exactly the cap is fine.
    c.execute(
        "INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (6, 2, 2, 3, 4, NULL)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO TilesData (id, Tile) VALUES (6, zeroblob(?1))",
        [MAX_TILE_BYTES as i64],
    )
    .unwrap();
    drop(c);
    let store = TileStore::open_read_only(&path).unwrap();
    let k = key(1, 2, 3, 4);
    match store.get(k) {
        Err(MapsError::TooLarge { id, key, len: l }) => {
            assert_eq!((id, key, l), (Some(5), Some(k), len));
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    match store.get_by_id(5) {
        Err(MapsError::TooLarge { id, key, len: l }) => {
            assert_eq!((id, key, l), (Some(5), None, len));
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    assert_eq!(
        store.get(key(2, 2, 3, 4)).unwrap().map(|b| b.len()),
        Some(MAX_TILE_BYTES)
    );

    let rw = TileStore::open(&path).unwrap();
    let big = vec![0u8; len];
    match rw.insert(key(9, 9, 9, 9), None, &big) {
        Err(MapsError::TooLarge {
            id,
            key: k9,
            len: l,
        }) => {
            assert_eq!((id, k9, l), (None, Some(key(9, 9, 9, 9)), len));
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    assert!(!rw.exists(key(9, 9, 9, 9)).unwrap());
}

#[test]
fn tiles_row_without_data_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    let c = csharp_db(&path);
    c.execute_batch(
        "INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (1, 1, 1, 5, 7, NULL);
         INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (2, 2, 1, 5, 7, NULL);
         INSERT INTO TilesData (id, Tile) VALUES (2, NULL);
         INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (3, 3, 1, 5, 7, NULL);
         INSERT INTO TilesData (id, Tile) VALUES (3, 'text, not a blob');",
    )
    .unwrap();
    drop(c);
    let store = TileStore::open(&path).unwrap();
    for x in 1..=3 {
        let k = key(x, 1, 5, 7);
        assert!(store.exists(k).unwrap());
        assert_eq!(store.get(k).unwrap(), None, "x = {x}");
        assert_eq!(store.get_by_id(i64::from(x)).unwrap(), None);
    }
}

#[test]
fn cache_time_formats() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    let c = csharp_db(&path);
    let rows: Vec<(rusqlite::types::Value, Option<NaiveDateTime>)> = {
        use rusqlite::types::Value as V;
        vec![
            (
                V::Text("2019-12-14 10:11:12.1234567".into()),
                Some(dt(10, 11, 12, 123_456_700)),
            ),
            (
                V::Text("2019-12-14 10:11:12".into()),
                Some(dt(10, 11, 12, 0)),
            ),
            (
                V::Text("2019-12-14T10:11:12".into()),
                Some(dt(10, 11, 12, 0)),
            ),
            (
                V::Text("2019-12-14T10:11:12.5".into()),
                Some(dt(10, 11, 12, 500_000_000)),
            ),
            // Julian day 2458832.25 is 2019-12-14 18:00:00.
            (V::Real(2_458_832.25), Some(dt(18, 0, 0, 0))),
            (V::Integer(2_458_832), Some(dt(12, 0, 0, 0))),
            (V::Text("garbage".into()), None),
            (V::Text(String::new()), None),
            (V::Real(f64::INFINITY), None),
            (V::Real(1e300), None),
            // Outside the years 1601 to 9999: no file time, never handed out.
            (V::Real(97_466_824.49), None),
            (V::Real(13_000_000.0), None),
            (V::Text("1500-01-01 00:00:00".into()), None),
            (V::Text("10000-01-01 00:00:00".into()), None),
            (
                V::Text("1601-01-01 00:00:00".into()),
                Some(
                    NaiveDate::from_ymd_opt(1601, 1, 1)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                ),
            ),
            (
                V::Text("9999-12-31 23:59:59".into()),
                Some(
                    NaiveDate::from_ymd_opt(9999, 12, 31)
                        .unwrap()
                        .and_hms_opt(23, 59, 59)
                        .unwrap(),
                ),
            ),
            (V::Blob(vec![1, 2, 3]), None),
            (V::Null, None),
        ]
    };
    for (i, (v, _)) in rows.iter().enumerate() {
        let id = i as i64 + 1;
        c.execute(
            "INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (?1, ?1, 0, 5, 1, ?2)",
            params![id, v],
        )
        .unwrap();
    }
    drop(c);
    let store = TileStore::open_read_only(&path).unwrap();
    let got: Vec<Option<NaiveDateTime>> =
        store.list().unwrap().iter().map(|t| t.cache_time).collect();
    let want: Vec<Option<NaiveDateTime>> = rows.iter().map(|r| r.1).collect();
    assert_eq!(got, want);
}

#[test]
fn list_skips_rows_that_are_no_tile_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.gmdb");
    let c = csharp_db(&path);
    c.execute_batch(
        "INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (1, -1, 0, 5, 1);
         INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (2, 0, 0, 300, 1);
         INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (3, 'x', 0, 5, 1);
         INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (4, 0, 0, 5, 9999999999);
         INSERT INTO Tiles (id, X, Y, Zoom, Type) VALUES (5, 1, 2, 5, 1);",
    )
    .unwrap();
    drop(c);
    let store = TileStore::open_read_only(&path).unwrap();
    let list = store.list().unwrap();
    assert_eq!(
        list,
        vec![TileInfo {
            id: 5,
            key: key(1, 2, 5, 1),
            cache_time: None
        }]
    );
    assert_eq!(store.count().unwrap(), 5);
    assert_eq!(store.types().unwrap(), vec![(1, 4)]);
}

#[test]
fn opens_a_csharp_written_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("csharp.gmdb");
    let c = csharp_db(&path);
    // MapDatabase.cs:231 and :240: the C# column order, an explicit id, text time.
    c.execute(
        "INSERT INTO Tiles (id, X, Y, Zoom, Type, CacheTime) VALUES (@id, @x, @y, @z, @type, @cachetime)",
        params![1, 545, 342, 10, 1_492_776_782, "2019-12-14 10:11:12.1234567"],
    )
    .unwrap();
    c.execute(
        "INSERT INTO TilesData (id, Tile) VALUES (@id, @tile)",
        params![1, PNG],
    )
    .unwrap();
    drop(c);
    let k = key(545, 342, 10, 1_492_776_782);
    for store in [
        TileStore::open_read_only(&path).unwrap(),
        TileStore::open(&path).unwrap(),
    ] {
        assert!(store.exists(k).unwrap());
        assert_eq!(store.get(k).unwrap().as_deref(), Some(PNG));
        assert_eq!(
            store.list().unwrap(),
            vec![TileInfo {
                id: 1,
                key: k,
                cache_time: Some(dt(10, 11, 12, 123_456_700))
            }]
        );
    }
    let rw = TileStore::open(&path).unwrap();
    assert!(rw
        .insert(key(546, 342, 10, 1_492_776_782), None, PNG)
        .unwrap());
    assert_eq!(rw.list().unwrap()[1].id, 2);
    drop(rw);
    // An existing C# file gets no index added.
    let c = Connection::open(&path).unwrap();
    assert_eq!(table_sql(&c, "IndexOfTiles"), None);
}

#[test]
fn png_or_jpeg_signature() {
    assert!(is_png_or_jpeg(PNG));
    assert!(is_png_or_jpeg(b"\x89PNG\r\n\x1a\n"));
    assert!(is_png_or_jpeg(b"\xff\xd8\xff\xe0rest"));
    assert!(!is_png_or_jpeg(b"\xff\xd8"));
    assert!(!is_png_or_jpeg(b"\x89PNG\r\n"));
    assert!(!is_png_or_jpeg(b"<html><body>404</body></html>"));
    assert!(!is_png_or_jpeg(b""));
}
