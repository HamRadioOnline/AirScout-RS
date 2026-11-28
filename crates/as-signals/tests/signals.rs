//! Signal level database (`SignalDatabase.cs`) and the Spectrum Lab parts of
//! `MapDlg.cs:7620-7710`.
use as_signals::{parse_speclab_fft, speclab_url, SignalDb, SignalError};
use chrono::{DateTime, Duration, Utc};

fn t(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap()
}

#[test]
fn find_exact_second_only() {
    let db = SignalDb::open_in_memory().unwrap();
    db.insert_or_update_if_newer(-87.5, t(1_795_438_420))
        .unwrap();
    assert_eq!(db.find(t(1_795_438_420)).unwrap(), Some(-87.5));
    // stored as unix seconds: a time within the same second finds it
    assert_eq!(
        db.find(t(1_795_438_420) + Duration::milliseconds(900))
            .unwrap(),
        Some(-87.5)
    );
    assert_eq!(db.find(t(1_795_438_421)).unwrap(), None);
    assert_eq!(db.find(t(1_795_438_419)).unwrap(), None);
}

#[test]
fn upsert_returns_changed_rows_only() {
    let db = SignalDb::open_in_memory().unwrap();
    assert_eq!(db.insert_or_update_if_newer(-80.0, t(100)).unwrap(), 1);
    // The strongest level within the second wins.
    assert_eq!(db.insert_or_update_if_newer(-70.0, t(100)).unwrap(), 1);
    assert_eq!(db.insert_or_update_if_newer(-80.0, t(100)).unwrap(), 0);
    assert_eq!(db.find(t(100)).unwrap(), Some(-70.0));
    let later = t(100) + Duration::milliseconds(500);
    assert_eq!(db.insert_or_update_if_newer(-60.0, later).unwrap(), 1);
    assert_eq!(db.find(t(100)).unwrap(), Some(-60.0));
}

#[test]
fn bulk_insert_and_count() {
    let db = SignalDb::open_in_memory().unwrap();
    let rows: Vec<(f64, DateTime<Utc>)> =
        (0..61).map(|i| (-90.0 + i as f64, t(1000 + i))).collect();
    assert_eq!(db.bulk_insert(&rows).unwrap(), 0);
    assert_eq!(db.count().unwrap(), 61);
    // duplicates fail one by one: C# returns -errors and keeps the rest
    let more = [(-1.0, t(1000)), (-2.0, t(2000)), (-3.0, t(1001))];
    assert_eq!(db.bulk_insert(&more).unwrap(), -2);
    assert_eq!(db.count().unwrap(), 62);
    assert_eq!(db.find(t(1000)).unwrap(), Some(-90.0));
    // the poller's bulk path
    assert_eq!(
        db.bulk_insert_or_update_if_newer(&[(5.0, t(1000)), (6.0, t(3000))])
            .unwrap(),
        2
    );
    assert_eq!(db.count().unwrap(), 63);
    assert_eq!(db.find(t(1000)).unwrap(), Some(5.0));
    // housekeeping
    assert_eq!(db.delete_older_than(t(1060)).unwrap(), 60);
    assert_eq!(db.count().unwrap(), 3);
}

#[test]
fn signals_file_persists() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = SignalDb::open(dir.path()).unwrap();
        db.insert_or_update_if_newer(-50.0, t(42)).unwrap();
    }
    assert!(dir.path().join("signals.db3").exists());
    let db = SignalDb::open(dir.path()).unwrap();
    assert_eq!(db.find(t(42)).unwrap(), Some(-50.0));
}

#[test]
fn speclab_url_joins_with_ampersand() {
    // C# joins the second parameter with '?' (GUI bug, not reproduced; logic-findings #78)
    assert_eq!(
        speclab_url("http://localhost:10080", "_fft_expt.json", 400, 1600),
        "http://localhost:10080/_fft_expt.json?f1=400&f2=1600"
    );
    // clamping as in the poller: f1 in 0..=3000, f2 in 1..=3000, f1 < f2
    assert_eq!(
        speclab_url("http://h/", "x.json", -5, 5000),
        "http://h/x.json?f1=0&f2=3000"
    );
    assert_eq!(
        speclab_url("http://h/", "x.json", 2000, 1000),
        "http://h/x.json?f1=999&f2=1000"
    );
}

/// A Spectrum Lab export header followed by its FFT bin array.
fn fft(bins: &str) -> String {
    format!(
        "[{{\"t_unix\":1795438420.75,\"f0\":0.0,\"df\":5.3833,\"ch\":1,\"f1\":400,\"f2\":1600,\"nbins\":8,\"unit\":\"dB\"}},\r\n[{bins}]\r\n]"
    )
}

#[test]
fn parse_fft_max() {
    let (at, max) =
        parse_speclab_fft(&fft("-87.5,-90.1,-60.25,-75.0,-99.9,-88.8,-70.5,-95.1")).unwrap();
    // the fractional seconds are truncated
    assert_eq!(at, t(1_795_438_420));
    assert_eq!(max, -60.25);
}

#[test]
fn parse_fft_errors() {
    let bad = [
        String::new(),
        "{\"t\":1}".to_string(),
        "[{\"t_unix\":1795438420}]".to_string(),
        fft("-87.5,abc,-1.0"),
        fft("-87.5,,-1.0"),
        fft("1.0,2.0 ").replace("\"nbins\":8", "\"nbins\":\"x\""),
        // no character before ']'
        fft(""),
        "[{\"t_unix\":\"x\",\"f0\":0,\"df\":1,\"ch\":1,\"f1\":1,\"f2\":2,\"nbins\":2,\"u\":1},[1.0,2.0]]"
            .to_string(),
    ];
    for b in bad {
        assert!(
            matches!(parse_speclab_fft(&b), Err(SignalError::Parse(_))),
            "{b:?}"
        );
    }
}

#[test]
fn finding_79_parses_the_entire_last_bin() {
    for bins in ["-87.5,-90.1,-5.5", "-87.5,-90.1,-5.5 "] {
        let response = fft(bins).replace("\"nbins\":8", "\"nbins\":3");
        assert_eq!(parse_speclab_fft(&response).unwrap().1, -5.5);
    }
}

#[test]
fn finding_379_requires_a_positive_count_and_finite_numeric_bins() {
    let response =
        |count, bins| format!("[{{\"nbins\":{count},\"unit\":\"dB\",\"t_unix\":100.75}},[{bins}]]");
    assert_eq!(
        parse_speclab_fft(&response(2, "-87.5,-5.5")).unwrap(),
        (t(100), -5.5)
    );
    assert!(parse_speclab_fft(&fft("-87.5,-5.5")).is_err());
    assert!(parse_speclab_fft(&response(3, "-87.5,1e400,-5.5")).is_err());
    assert!(parse_speclab_fft(&response(0, "")).is_err());
    assert!(parse_speclab_fft(&response(2, "-87.5,\"-5.5\"")).is_err());
    assert!(parse_speclab_fft(&response(2, "-87.5,-5.5").replace("100.75", "1e400")).is_err());
}

#[test]
fn finding_81_keeps_the_maximum_for_each_second() {
    for levels in [[-87.0, -60.0, -75.0], [-60.0, -87.0, -75.0]] {
        let db = SignalDb::open_in_memory().unwrap();
        for level in levels {
            db.insert_or_update_if_newer(level, t(100)).unwrap();
        }
        db.insert_or_update_if_newer(-75.0, t(100) + Duration::milliseconds(500))
            .unwrap();
        db.bulk_insert_or_update_if_newer(&[(-40.0, t(101))])
            .unwrap();
        assert_eq!(db.find(t(100)).unwrap(), Some(-60.0));
        assert_eq!(db.find(t(101)).unwrap(), Some(-40.0));
    }
}

#[test]
fn finding_81_removes_invalid_legacy_levels_and_repairs_invalid_keys() {
    let dir = tempfile::tempdir().unwrap();
    let raw = rusqlite::Connection::open(dir.path().join("signals.db3")).unwrap();
    raw.execute_batch(
        "CREATE TABLE SignalLevels (Level DOUBLE, LastUpdated INT32 PRIMARY KEY);
        INSERT INTO SignalLevels VALUES (NULL,1),(1e999,2),(-1e999,3),('bad',4),(-90,5);
        PRAGMA user_version=1;",
    )
    .unwrap();
    drop(raw);
    let db = SignalDb::open(dir.path()).unwrap();
    assert_eq!(db.count().unwrap(), 1);
    assert_eq!(db.find(t(5)).unwrap(), Some(-90.0));
    db.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO SignalLevels VALUES (NULL,1),(1e999,2),(-1e999,3),('bad',4);",
        )
    })
    .unwrap();
    for second in 1..=4 {
        assert_eq!(db.find(t(second)).unwrap(), None);
        db.insert_or_update_if_newer(-65.0, t(second)).unwrap();
        assert_eq!(db.find(t(second)).unwrap(), Some(-65.0));
    }
    assert!(db.insert_or_update_if_newer(f64::NAN, t(5)).is_err());
    assert!(db
        .bulk_insert_or_update_if_newer(&[(5.0, t(6)), (f64::INFINITY, t(5))])
        .is_err());
    assert_eq!(db.find(t(6)).unwrap(), None);
    assert_eq!(
        db.bulk_insert(&[(f64::NEG_INFINITY, t(6)), (-70.0, t(5))])
            .unwrap(),
        -2
    );
    assert_eq!(db.find(t(6)).unwrap(), None);
    drop(db);
    let db = SignalDb::open(dir.path()).unwrap();
    assert_eq!(db.count().unwrap(), 5);
    assert_eq!(db.find(t(2)).unwrap(), Some(-65.0));
}

#[test]
fn finding_372_signal_db_rejects_schema_ahead_and_sets_auto_vacuum_on_fresh() {
    // Fresh file gets auto_vacuum = FULL (=1), WAL and user_version = 2.
    let fresh_dir = tempfile::tempdir().unwrap();
    {
        let db = SignalDb::open(fresh_dir.path()).unwrap();
        let (mode, journal, ver): (i64, String, i64) = db.with_conn(|c| {
            (
                c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap(),
                c.query_row("PRAGMA journal_mode", [], |r| r.get(0))
                    .unwrap(),
                c.query_row("PRAGMA user_version", [], |r| r.get(0))
                    .unwrap(),
            )
        });
        assert_eq!(mode, 1, "fresh signals.db3 must have auto_vacuum=FULL");
        assert_eq!(journal.to_lowercase(), "wal");
        assert_eq!(ver, 2);
    }

    // Fresh in-memory DB gets auto_vacuum = FULL (=1) and user_version = 2.
    let mem = SignalDb::open_in_memory().unwrap();
    let (mem_av, mem_ver): (i64, i64) = mem.with_conn(|c| {
        (
            c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap(),
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
        )
    });
    assert_eq!((mem_av, mem_ver), (1, 2));

    // Existing v1 rollback DB migrates to v2 and keeps its existing auto_vacuum = 0.
    let legacy_dir = tempfile::tempdir().unwrap();
    let legacy_path = legacy_dir.path().join("signals.db3");
    let raw = rusqlite::Connection::open(&legacy_path).unwrap();
    raw.execute_batch(
        "CREATE TABLE SignalLevels (Level DOUBLE, LastUpdated INT32 PRIMARY KEY);
         INSERT INTO SignalLevels VALUES (-72.0, 50);
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(raw);
    let legacy = SignalDb::open(legacy_dir.path()).unwrap();
    let (legacy_av, legacy_ver): (i64, i64) = legacy.with_conn(|c| {
        (
            c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap(),
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
        )
    });
    assert_eq!((legacy_av, legacy_ver), (0, 2));
    assert_eq!(legacy.find(t(50)).unwrap(), Some(-72.0));

    // Schema-ahead rollback-journal v3 DB is rejected before mutation.
    let ahead_dir = tempfile::tempdir().unwrap();
    let path = ahead_dir.path().join("signals.db3");
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE SignalLevels (Level DOUBLE, LastUpdated INT32 PRIMARY KEY);
         INSERT INTO SignalLevels VALUES (-55.0, 123);
         PRAGMA user_version = 3;",
    )
    .unwrap();
    drop(raw);
    let bytes_before = std::fs::read(&path).unwrap();

    let err = SignalDb::open(ahead_dir.path()).unwrap_err();
    assert!(
        matches!(
            err,
            SignalError::SchemaAhead {
                actual: 3,
                supported: 2
            }
        ),
        "unexpected error: {err:?}"
    );

    assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
    let check = rusqlite::Connection::open(&path).unwrap();
    let journal: String = check
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal.to_lowercase(), "delete");
    let av: i64 = check
        .query_row("PRAGMA auto_vacuum", [], |r| r.get(0))
        .unwrap();
    assert_eq!(av, 0);
    let ver: i64 = check
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, 3);
    let level: f64 = check
        .query_row(
            "SELECT Level FROM SignalLevels WHERE LastUpdated = 123",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(level, -55.0);
}
