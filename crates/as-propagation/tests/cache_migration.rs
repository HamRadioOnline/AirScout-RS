mod common;

use as_elevation::ElevationModel;
use as_propagation::{HorizonParams, PathParams, PropagationEngine};

#[test]
fn pre_stage_6_propagation_results_are_recomputed() {
    let dir = tempfile::tempdir().unwrap();
    let model = ElevationModel::Globe;
    let store = common::fixture_store(dir.path());
    let engine = PropagationEngine::open(store.clone(), dir.path()).unwrap();
    engine
        .with_conn(model, |conn| {
            conn.execute_batch(
                "DROP TABLE PropagationPath;
                 CREATE TABLE PropagationPath (
                   Lat1 REAL, Lon1 REAL, h1 REAL, Lat2 REAL, Lon2 REAL, h2 REAL, QRG REAL,
                   Radius REAL, F1_Clearance REAL, StepWidth REAL, Eps1_Min REAL, Eps2_Min REAL,
                   LastUpdated INTEGER,
                   PRIMARY KEY(Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth));
                 INSERT INTO PropagationPath
                   VALUES (51.0013, 11.5027, 10, 51.1, 11.6, 10, 0.144, 8473.43, 0.6, 1000, 0, 0, 1);
                 DROP TABLE PropagationHorizon;
                 CREATE TABLE PropagationHorizon (
                   Lat REAL, Lon REAL, h REAL, Dist REAL, QRG REAL, Radius REAL,
                   F1_Clearance REAL, StepWidth REAL, Horizon BLOB, LastUpdated INTEGER,
                   PRIMARY KEY(Lat, Lon, h, Dist, QRG, Radius, F1_Clearance, StepWidth));
                 INSERT INTO PropagationHorizon
                   VALUES (51.0013, 11.5027, 10, 50, 0.144, 8473.43, 0.6, 1000, zeroblob(6480), 1);
                 PRAGMA user_version = 2;",
            )
        })
        .unwrap()
        .unwrap();
    drop(engine);

    let engine = PropagationEngine::open(store, dir.path()).unwrap();
    let path = PathParams {
        lat1: 51.0013,
        lon1: 11.5027,
        h1: 10.0,
        lat2: 51.1,
        lon2: 11.6,
        h2: 10.0,
        qrg_ghz: 0.144,
        radius_km: 8473.43,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    };
    let horizon = HorizonParams {
        lat: path.lat1,
        lon: path.lon1,
        h: path.h1,
        dist_km: 50.0,
        qrg_ghz: path.qrg_ghz,
        radius_km: path.radius_km,
        f1_clearance: path.f1_clearance,
        step_width_m: path.step_width_m,
    };
    let found_path = engine.find_path(model, &path).unwrap();
    let found_horizon = engine.find_horizon(model, &horizon, None).unwrap();
    assert_eq!(
        (found_path.is_some(), found_horizon.is_some()),
        (false, false)
    );

    let path_new = engine
        .find_or_create_path(model, &path, true, None)
        .unwrap()
        .unwrap();
    let horizon_new = engine
        .find_or_create_horizon(model, &horizon, None, true, None, &|_, _| {})
        .unwrap()
        .unwrap();
    assert_eq!(engine.find_path(model, &path).unwrap(), Some(path_new));
    assert_eq!(
        engine.find_horizon(model, &horizon, None).unwrap(),
        Some(horizon_new)
    );
    engine
        .with_conn(model, |conn| {
            conn.execute_batch(
            "UPDATE PropagationPath SET AlgorithmVersion = 0, LastUpdated = 9223372036854775807;
             UPDATE PropagationHorizon SET AlgorithmVersion = 0, LastUpdated = 9223372036854775807;"
        )
        })
        .unwrap()
        .unwrap();
    assert!(engine.find_path(model, &path).unwrap().is_none());
    assert!(engine
        .find_horizon(model, &horizon, None)
        .unwrap()
        .is_none());
    let path_new = engine
        .find_or_create_path(model, &path, true, None)
        .unwrap()
        .unwrap();
    let horizon_new = engine
        .find_or_create_horizon(model, &horizon, None, true, None, &|_, _| {})
        .unwrap()
        .unwrap();
    assert_eq!(engine.find_path(model, &path).unwrap(), Some(path_new));
    assert_eq!(
        engine.find_horizon(model, &horizon, None).unwrap(),
        Some(horizon_new)
    );
}

#[test]
fn finding_372_propagation_engine_guards_schema_version_and_migrates_older() {
    let ahead_dir = tempfile::tempdir().unwrap();
    let store = common::fixture_store(ahead_dir.path());
    let path = ahead_dir.path().join(format!(
        "propagation_{}",
        ElevationModel::Globe.db_file_name()
    ));
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE PropagationPath (Id INTEGER PRIMARY KEY);
         INSERT INTO PropagationPath VALUES (1);
         PRAGMA user_version = 5;",
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();

    let err = PropagationEngine::open(store, ahead_dir.path())
        .err()
        .unwrap();
    assert!(
        matches!(
            err,
            as_propagation::PropagationError::SchemaAhead {
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
        let store = common::fixture_store(dir.path());
        let path = dir.path().join(format!(
            "propagation_{}",
            ElevationModel::Globe.db_file_name()
        ));
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch(
            "CREATE TABLE PropagationPath (
                Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, h1 REAL NOT NULL,
                Lat2 REAL NOT NULL, Lon2 REAL NOT NULL, h2 REAL NOT NULL,
                QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL,
                StepWidth REAL NOT NULL, Eps1_Min REAL, Eps2_Min REAL, LastUpdated INTEGER NOT NULL,
                PRIMARY KEY (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth));",
        )
        .unwrap();
        if start_ver >= 2 {
            raw.execute_batch(
                "CREATE TABLE PropagationHorizon (
                    Lat REAL NOT NULL, Lon REAL NOT NULL, h REAL NOT NULL, Dist REAL NOT NULL,
                    QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL,
                    StepWidth REAL NOT NULL, Horizon BLOB NOT NULL, LastUpdated INTEGER NOT NULL,
                    PRIMARY KEY (Lat, Lon, h, Dist, QRG, Radius, F1_Clearance, StepWidth));",
            )
            .unwrap();
        }
        if start_ver >= 3 {
            raw.execute_batch(
                "DROP TABLE PropagationPath;
                 CREATE TABLE PropagationPath (
                    Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, h1 REAL NOT NULL,
                    Lat2 REAL NOT NULL, Lon2 REAL NOT NULL, h2 REAL NOT NULL,
                    QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL,
                    StepWidth REAL NOT NULL, NearField REAL NOT NULL, Eps1_Min REAL, Eps2_Min REAL,
                    LastUpdated INTEGER NOT NULL, AlgorithmVersion INTEGER NOT NULL,
                    PRIMARY KEY (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth, NearField));",
            )
            .unwrap();
        }
        raw.pragma_update(None, "user_version", start_ver).unwrap();
        drop(raw);

        let engine = PropagationEngine::open(store, dir.path()).unwrap();
        let ver: i64 = engine
            .with_conn(ElevationModel::Globe, |c| {
                c.query_row("PRAGMA user_version", [], |r| r.get(0))
                    .unwrap()
            })
            .unwrap();
        assert_eq!(ver, 4);
    }
}
