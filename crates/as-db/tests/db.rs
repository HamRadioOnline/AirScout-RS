use as_db::{migrate, open, user_version};

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
