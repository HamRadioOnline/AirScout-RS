//! Table helpers of the database maintenance dialog (ScoutBaseDatabase.cs:124-163).
use as_db::{clear_table, open_in_memory, row_count, select_non_blob, table_list};

fn fixture() -> rusqlite::Connection {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE a (id INTEGER, name TEXT, data BLOB);
         CREATE TABLE b (x INTEGER);
         CREATE TABLE \"we\"\"ird\" (x INTEGER);
         INSERT INTO a VALUES (1, 'one', x'0102'), (2, 'two', x'03'), (3, 'three', NULL);
         INSERT INTO b VALUES (1), (2);
         INSERT INTO \"we\"\"ird\" VALUES (9);",
    )
    .unwrap();
    conn
}

#[test]
fn table_list_row_count_clear() {
    let conn = fixture();
    let tables = table_list(&conn).unwrap();
    assert_eq!(tables, vec!["a", "b", "we\"ird"]);
    assert_eq!(row_count(&conn, "a").unwrap(), 3);
    assert_eq!(row_count(&conn, "we\"ird").unwrap(), 1);
    assert_eq!(clear_table(&conn, "a").unwrap(), 3);
    assert_eq!(row_count(&conn, "a").unwrap(), 0);
    // other tables are untouched
    assert_eq!(row_count(&conn, "b").unwrap(), 2);
    assert_eq!(clear_table(&conn, "we\"ird").unwrap(), 1);
}

#[test]
fn unknown_table_is_rejected() {
    let conn = fixture();
    for bad in [
        "x; DROP TABLE b",
        "nope",
        "sqlite_master",
        "a; DELETE FROM b",
        "",
    ] {
        assert!(row_count(&conn, bad).is_err(), "{bad}");
        assert!(clear_table(&conn, bad).is_err(), "{bad}");
        assert!(select_non_blob(&conn, bad, 10).is_err(), "{bad}");
    }
    assert_eq!(row_count(&conn, "b").unwrap(), 2);
}

#[test]
fn select_skips_blob_columns_and_limits() {
    let conn = fixture();
    let (cols, rows) = select_non_blob(&conn, "a", 2).unwrap();
    assert_eq!(cols, vec!["id", "name"]);
    assert_eq!(rows, vec![vec!["1", "one"], vec!["2", "two"]]);
    let (_, all) = select_non_blob(&conn, "a", 100).unwrap();
    assert_eq!(all.len(), 3);
    // read-only: nothing changed
    assert_eq!(row_count(&conn, "a").unwrap(), 3);
}

#[test]
fn only_blob_columns_gives_no_rows_error_free() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE t (d BLOB); INSERT INTO t VALUES (x'00');")
        .unwrap();
    let (cols, rows) = select_non_blob(&conn, "t", 10).unwrap();
    assert!(cols.is_empty() && rows.is_empty());
}
