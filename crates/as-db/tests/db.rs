use as_db::{
    migrate, open, open_in_memory_versioned, open_in_memory_versioned_with, open_versioned,
    open_versioned_with, user_version, DbError, InitOptions,
};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::error::Error;
use std::sync::{Arc, Barrier};
use std::time::Duration;

fn journal_mode(conn: &Connection) -> String {
    conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
        .unwrap()
        .to_lowercase()
}

fn auto_vacuum(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))
        .unwrap()
}

fn synchronous(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn migrate_applies_pending_statements_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db3");
    let steps = ["CREATE TABLE a (x INTEGER)", "CREATE TABLE b (y INTEGER)"];
    {
        let conn = open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        migrate(&conn, &steps).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 2);
    }
    let conn = open(&path).unwrap();
    // re-running must not fail on "table already exists"
    migrate(&conn, &steps).unwrap();
    assert_eq!(user_version(&conn).unwrap(), 2);
    let more = [
        "CREATE TABLE a (x INTEGER)",
        "CREATE TABLE b (y INTEGER)",
        "CREATE TABLE c (z INTEGER)",
    ];
    migrate(&conn, &more).unwrap();
    assert_eq!(user_version(&conn).unwrap(), 3);
}

#[test]
fn open_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/dir/t.db3");
    open(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn finding_372_schema_ahead_rejected_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ahead.db3");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE future_tbl (x INTEGER);
         INSERT INTO future_tbl VALUES (7);
         PRAGMA user_version = 2;",
    )
    .unwrap();
    assert_eq!(journal_mode(&raw), "delete");
    assert_eq!(auto_vacuum(&raw), 0);
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let v1 = ["CREATE TABLE legacy_tbl (x INTEGER)"];

    for err in [
        open_versioned(&path, &v1).unwrap_err(),
        open_versioned_with(&path, &v1, InitOptions::default().with_auto_vacuum_full())
            .unwrap_err(),
    ] {
        assert!(
            matches!(
                err,
                DbError::SchemaAhead {
                    actual: 2,
                    supported: 1
                }
            ),
            "unexpected error: {err:?}"
        );
        let text = err.to_string();
        assert!(text.contains('2') && text.contains('1'), "{text}");
        assert!(err.source().is_none());
    }

    let raw = Connection::open(&path).unwrap();
    let err = migrate(&raw, &v1).unwrap_err();
    assert!(matches!(
        err,
        DbError::SchemaAhead {
            actual: 2,
            supported: 1
        }
    ));
    drop(raw);

    assert_eq!(std::fs::read(&path).unwrap(), before);
    let check = Connection::open(&path).unwrap();
    assert_eq!(journal_mode(&check), "delete");
    assert_eq!(auto_vacuum(&check), 0);
    assert_eq!(user_version(&check).unwrap(), 2);
    let val: i64 = check
        .query_row("SELECT x FROM future_tbl", [], |r| r.get(0))
        .unwrap();
    assert_eq!(val, 7);
    let legacy_exists: i64 = check
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'legacy_tbl'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(legacy_exists, 0);
}

#[test]
fn finding_372_sqlite_error_propagates_with_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db3");
    std::fs::write(&path, b"not a sqlite database file").unwrap();
    let err = open_versioned(&path, &["CREATE TABLE a (x INTEGER)"]).unwrap_err();
    assert!(matches!(err, DbError::Sqlite(_)), "{err:?}");
    assert!(err.source().is_some());
}

#[test]
fn finding_372_concurrent_same_version_and_ahead_version_recheck() {
    let dir = tempfile::tempdir().unwrap();

    // 1. Concurrent same-version opener blocked behind a v0 -> v2 migration lock:
    //    must see v2 under the immediate lock and skip re-running non-idempotent DDL.
    let same_path = dir.path().join("same_race.db3");
    let v2_steps = [
        "CREATE TABLE items (id INTEGER PRIMARY KEY)",
        "ALTER TABLE items ADD COLUMN note TEXT",
    ];
    let upgrader = Connection::open(&same_path).unwrap();
    let tx = Transaction::new_unchecked(&upgrader, TransactionBehavior::Immediate).unwrap();
    tx.execute_batch(v2_steps[0]).unwrap();
    tx.execute_batch(v2_steps[1]).unwrap();
    tx.pragma_update(None, "user_version", 2i64).unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let p2 = same_path.clone();
    let handle = std::thread::spawn(move || {
        b2.wait();
        let conn = open_versioned(&p2, &v2_steps).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 2);
        assert_eq!(journal_mode(&conn), "wal");
    });

    barrier.wait();
    std::thread::sleep(Duration::from_millis(60));
    tx.commit().unwrap();
    drop(upgrader);
    handle.join().unwrap();

    // 2. Concurrent ahead-version opener starting from v0 while lock holder upgrades to v2:
    //    must reject with SchemaAhead without running old migration steps or altering journal mode.
    let ahead_v0_path = dir.path().join("ahead_v0_race.db3");
    let upgrader = Connection::open(&ahead_v0_path).unwrap();
    let tx = Transaction::new_unchecked(&upgrader, TransactionBehavior::Immediate).unwrap();
    tx.execute_batch(
        "CREATE TABLE future_v2 (x INTEGER);
         INSERT INTO future_v2 VALUES (11);
         PRAGMA user_version = 2;",
    )
    .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let p2 = ahead_v0_path.clone();
    let handle = std::thread::spawn(move || {
        b2.wait();
        open_versioned(&p2, &["CREATE TABLE old_v1 (x INTEGER)"]).unwrap_err()
    });

    barrier.wait();
    std::thread::sleep(Duration::from_millis(60));
    tx.commit().unwrap();
    drop(upgrader);

    let err = handle.join().unwrap();
    assert!(matches!(
        err,
        DbError::SchemaAhead {
            actual: 2,
            supported: 1
        }
    ));
    let check = Connection::open(&ahead_v0_path).unwrap();
    assert_eq!(journal_mode(&check), "delete");
    assert_eq!(user_version(&check).unwrap(), 2);
    let old_exists: i64 = check
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'old_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old_exists, 0);

    // 3. Concurrent ahead-version opener when initial version == supported (v1 -> v2 behind lock):
    //    must NOT short-circuit before the immediate lock; must observe v2 and reject untouched.
    let ahead_v1_path = dir.path().join("ahead_v1_race.db3");
    let setup = Connection::open(&ahead_v1_path).unwrap();
    setup
        .execute_batch(
            "CREATE TABLE items (x INTEGER);
             INSERT INTO items VALUES (1);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(setup);

    let upgrader = Connection::open(&ahead_v1_path).unwrap();
    let tx = Transaction::new_unchecked(&upgrader, TransactionBehavior::Immediate).unwrap();
    tx.execute_batch(
        "INSERT INTO items VALUES (2);
         PRAGMA user_version = 2;",
    )
    .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let p2 = ahead_v1_path.clone();
    let handle = std::thread::spawn(move || {
        b2.wait();
        open_versioned(&p2, &["CREATE TABLE items (x INTEGER)"]).unwrap_err()
    });

    barrier.wait();
    std::thread::sleep(Duration::from_millis(60));
    tx.commit().unwrap();
    drop(upgrader);

    let err = handle.join().unwrap();
    assert!(matches!(
        err,
        DbError::SchemaAhead {
            actual: 2,
            supported: 1
        }
    ));
    let check = Connection::open(&ahead_v1_path).unwrap();
    assert_eq!(journal_mode(&check), "delete");
    assert_eq!(auto_vacuum(&check), 0);
    assert_eq!(user_version(&check).unwrap(), 2);
    let count: i64 = check
        .query_row("SELECT count(*) FROM items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn finding_372_fresh_auto_vacuum_and_existing_preservation() {
    let dir = tempfile::tempdir().unwrap();
    let steps = [
        "CREATE TABLE t (id INTEGER PRIMARY KEY)",
        "ALTER TABLE t ADD COLUMN val REAL",
    ];
    let opts = InitOptions::default().with_auto_vacuum_full();

    // Fresh file gets auto_vacuum = FULL (=1), WAL, synchronous = NORMAL (=1).
    let fresh = dir.path().join("fresh_av.db3");
    let conn = open_versioned_with(&fresh, &steps, opts).unwrap();
    assert_eq!(auto_vacuum(&conn), 1);
    assert_eq!(journal_mode(&conn), "wal");
    assert_eq!(synchronous(&conn), 1);
    assert_eq!(user_version(&conn).unwrap(), 2);
    drop(conn);

    // Concurrent fresh openers all succeed and preserve auto_vacuum = FULL (=1).
    let race = dir.path().join("fresh_av_race.db3");
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let b = barrier.clone();
            let p = race.clone();
            std::thread::spawn(move || {
                b.wait();
                let conn = open_versioned_with(&p, &steps, opts).unwrap();
                assert_eq!(auto_vacuum(&conn), 1);
                assert_eq!(user_version(&conn).unwrap(), 2);
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    // Fresh opener with auto_vacuum=FULL blocked behind a v2 rollback creator rejects untouched.
    let blocked = dir.path().join("blocked_av_v2.db3");
    let creator = Connection::open(&blocked).unwrap();
    let tx = Transaction::new_unchecked(&creator, TransactionBehavior::Immediate).unwrap();
    tx.execute_batch(
        "CREATE TABLE future_tbl (x INTEGER);
         INSERT INTO future_tbl VALUES (9);
         PRAGMA user_version = 2;",
    )
    .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let p2 = blocked.clone();
    let handle = std::thread::spawn(move || {
        b2.wait();
        open_versioned_with(&p2, &steps[..1], opts).unwrap_err()
    });
    barrier.wait();
    std::thread::sleep(Duration::from_millis(60));
    tx.commit().unwrap();
    drop(creator);
    let err = handle.join().unwrap();
    assert!(matches!(
        err,
        DbError::SchemaAhead {
            actual: 2,
            supported: 1
        }
    ));
    let check = Connection::open(&blocked).unwrap();
    assert_eq!(journal_mode(&check), "delete");
    assert_eq!(auto_vacuum(&check), 0);
    assert_eq!(user_version(&check).unwrap(), 2);

    // Existing v1 DB with auto_vacuum = 0 migrates to v2 and keeps auto_vacuum = 0.
    let existing = dir.path().join("existing_av0.db3");
    let raw = Connection::open(&existing).unwrap();
    raw.execute_batch(steps[0]).unwrap();
    raw.pragma_update(None, "user_version", 1i64).unwrap();
    drop(raw);
    let conn = open_versioned_with(&existing, &steps, opts).unwrap();
    assert_eq!(auto_vacuum(&conn), 0);
    assert_eq!(user_version(&conn).unwrap(), 2);
}

#[test]
fn finding_372_in_memory_versioned_setup() {
    let steps = [
        "CREATE TABLE t (x INTEGER PRIMARY KEY)",
        "ALTER TABLE t ADD COLUMN y TEXT",
    ];
    let conn = open_in_memory_versioned(&steps).unwrap();
    assert_eq!(user_version(&conn).unwrap(), 2);
    let err = migrate(&conn, &steps[..1]).unwrap_err();
    assert!(matches!(
        err,
        DbError::SchemaAhead {
            actual: 2,
            supported: 1
        }
    ));

    let av_conn =
        open_in_memory_versioned_with(&steps, InitOptions::default().with_auto_vacuum_full())
            .unwrap();
    assert_eq!(user_version(&av_conn).unwrap(), 2);
    assert_eq!(auto_vacuum(&av_conn), 1);
}

#[test]
fn finding_372_migration_from_each_older_version() {
    let steps = [
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER NOT NULL)",
        "ALTER TABLE t ADD COLUMN b INTEGER NOT NULL DEFAULT 10",
        "ALTER TABLE t ADD COLUMN c INTEGER NOT NULL DEFAULT 20",
    ];

    for start_ver in 0..=steps.len() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("from_v{start_ver}.db3"));
        if start_ver > 0 {
            let raw = Connection::open(&path).unwrap();
            for step in &steps[..start_ver] {
                raw.execute_batch(step).unwrap();
            }
            raw.execute("INSERT INTO t (id, a) VALUES (1, 5)", [])
                .unwrap();
            raw.pragma_update(None, "user_version", start_ver as i64)
                .unwrap();
        }

        let conn = open_versioned(&path, &steps).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 3);
        if start_ver > 0 {
            let row: (i64, i64, i64) = conn
                .query_row("SELECT a, b, c FROM t WHERE id = 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            assert_eq!(row, (5, 10, 20));
        } else {
            conn.execute("INSERT INTO t (id, a, b, c) VALUES (1, 5, 10, 20)", [])
                .unwrap();
        }
    }
}
