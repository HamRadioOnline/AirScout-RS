use as_elevation::{ElevationModel, ElevationStore, LocalObstruction};
use chrono::{Duration, TimeZone, Utc};

fn store() -> (tempfile::TempDir, ElevationStore) {
    let dir = tempfile::tempdir().unwrap();
    let s = ElevationStore::open(dir.path()).unwrap();
    (dir, s)
}

fn obstruction(lat: f64, lon: f64, d: f64, h: f64) -> LocalObstruction {
    LocalObstruction::new(lat, lon, Some([d; 360]), Some([h; 360]))
}

#[test]
fn obstruction_roundtrip_per_model() {
    let (_d, s) = store();
    let o = obstruction(50.1, 11.9, 1000.0, 25.5);
    let at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    assert!(s
        .local_obstruction_insert_or_update_if_newer(ElevationModel::Globe, &o, at)
        .unwrap());
    assert_eq!(
        s.local_obstruction_find(ElevationModel::Globe, 50.1, 11.9)
            .unwrap(),
        Some(o)
    );
    assert!(s
        .local_obstruction_find(ElevationModel::Srtm3, 50.1, 11.9)
        .unwrap()
        .is_none());
    assert!(s
        .local_obstruction_find(ElevationModel::Globe, 50.1, 12.0)
        .unwrap()
        .is_none());
    assert!(s
        .local_obstruction_delete(ElevationModel::Globe, 50.1, 11.9)
        .unwrap());
    assert!(!s
        .local_obstruction_delete(ElevationModel::Globe, 50.1, 11.9)
        .unwrap());
    assert!(s
        .local_obstruction_find(ElevationModel::Globe, 50.1, 11.9)
        .unwrap()
        .is_none());
}

#[test]
fn obstruction_default_is_zero_and_unsaved_when_save_false() {
    let (_d, s) = store();
    let m = ElevationModel::Srtm3;
    let o = s
        .local_obstruction_find_or_create_default(m, 48.0, 11.0, false)
        .unwrap();
    assert_eq!(o, LocalObstruction::new(48.0, 11.0, None, None));
    assert!(o.distance.iter().chain(o.height.iter()).all(|v| *v == 0.0));
    assert!(s.local_obstruction_find(m, 48.0, 11.0).unwrap().is_none());
    let o2 = s
        .local_obstruction_find_or_create_default(m, 48.0, 11.0, true)
        .unwrap();
    assert_eq!(o2, o);
    assert_eq!(s.local_obstruction_find(m, 48.0, 11.0).unwrap(), Some(o));
}

#[test]
fn obstruction_update_only_if_newer() {
    let (_d, s) = store();
    let m = ElevationModel::Globe;
    let t0 = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let a = obstruction(1.5, 2.5, 100.0, 1.0);
    let b = obstruction(1.5, 2.5, 200.0, 2.0);
    let c = obstruction(1.5, 2.5, 300.0, 3.0);
    assert!(s
        .local_obstruction_insert_or_update_if_newer(m, &a, t0)
        .unwrap());
    // same timestamp and older: ignored
    assert!(!s
        .local_obstruction_insert_or_update_if_newer(m, &b, t0)
        .unwrap());
    assert!(!s
        .local_obstruction_insert_or_update_if_newer(m, &b, t0 - Duration::days(1))
        .unwrap());
    assert_eq!(s.local_obstruction_find(m, 1.5, 2.5).unwrap(), Some(a));
    // newer: replaced
    assert!(s
        .local_obstruction_insert_or_update_if_newer(m, &c, t0 + Duration::seconds(1))
        .unwrap());
    assert_eq!(s.local_obstruction_find(m, 1.5, 2.5).unwrap(), Some(c));
}

#[test]
fn existing_db_without_table_gains_it() {
    let dir = tempfile::tempdir().unwrap();
    drop(ElevationStore::open(dir.path()).unwrap());
    for m in ElevationModel::ALL {
        let c = rusqlite::Connection::open(dir.path().join(m.db_file_name())).unwrap();
        c.execute("DROP TABLE LocalObstruction", []).unwrap();
        // stage-3a files: two schema steps applied
        c.pragma_update(None, "user_version", 2).unwrap();
    }
    let s = ElevationStore::open(dir.path()).unwrap();
    assert!(s
        .local_obstruction_find(ElevationModel::Globe, 0.0, 0.0)
        .unwrap()
        .is_none());
}
