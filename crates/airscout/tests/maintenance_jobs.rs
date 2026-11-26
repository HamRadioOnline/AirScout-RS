//! Maintenance jobs on real temp databases and the map tile cache. Synthetic data only.
mod maint_common;
use airscout::jobs::JobEvent;
use airscout::maintenance::{
    clear_tile_cache, locations_pattern, table_description, DbId, MaintenanceJob, VIEW_LIMIT,
};
use airscout::services::open_data;
use as_elevation::ElevationModel;
use as_geo::band::Band;
use maint_common::*;
// ---- jobs on real databases ----

#[test]
fn delete_paths_all_five_models() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    for m in ElevationModel::ALL {
        paths
            .store
            .with_conn(m, |c| {
                c.execute(
                    "INSERT INTO ElevationPath VALUES (50.0, 8.0, 50.1, 8.1, 100.0, 1, x'0100', 1)",
                    [],
                )
                .map_err(Into::into)
            })
            .unwrap();
        paths
            .engine
            .with_conn(m, |c| {
                c.execute(
                    "INSERT INTO PropagationPath VALUES (50.0, 8.0, 10.0, 50.1, 8.1, 10.0, 10368.0, 8500.0, 0.6, 100.0, 0.0, 0.0, 1)",
                    [],
                )
                .unwrap();
            })
            .unwrap();
        // a table that must survive
        paths
            .store
            .with_conn(m, |c| {
                c.execute(
                    "INSERT INTO LocalObstruction (Lat, Lon, Obstruction, LastUpdated) VALUES (50.0, 8.0, 1.0, 1)",
                    [],
                )
                .map_err(Into::into)
            })
            .ok();
    }
    let ev = run_job(&paths, MaintenanceJob::DeleteElevationPaths);
    assert!(
        matches!(&ev[..], [JobEvent::Done(s)] if s.starts_with("5 ")),
        "{ev:?}"
    );
    let ev = run_job(&paths, MaintenanceJob::DeletePropagationPaths);
    assert!(
        matches!(&ev[..], [JobEvent::Done(s)] if s.starts_with("5 ")),
        "{ev:?}"
    );
    for m in ElevationModel::ALL {
        let n = paths
            .store
            .with_conn(m, |c| Ok(as_db::row_count(c, "ElevationPath")?))
            .unwrap();
        assert_eq!(n, 0, "{m:?}");
        let n = paths
            .engine
            .with_conn(m, |c| as_db::row_count(c, "PropagationPath").unwrap())
            .unwrap();
        assert_eq!(n, 0, "{m:?}");
    }
}

#[test]
fn delete_station_removes_location_and_qrv() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let st = &paths.stations;
    st.location_bulk_insert_or_update_if_newer(&[
        loc("TE1ST", "JO00AA"),
        loc("TE1ST", "JO00AB"),
        loc("TE2ND", "JO00AA"),
    ])
    .unwrap();
    st.qrv_bulk_insert_or_update_if_newer(&[
        qrv("TE1ST", "JO00AA", Band::B10G),
        qrv("TE1ST", "JO00AA", Band::B144M),
        qrv("TE1ST", "JO00AB", Band::B10G),
        qrv("TE2ND", "JO00AA", Band::B10G),
    ])
    .unwrap();
    let ev = run_job(
        &paths,
        MaintenanceJob::DeleteStation {
            call: "TE1ST".into(),
            loc: "JO00AA".into(),
        },
    );
    assert!(matches!(&ev[..], [JobEvent::Done(_)]), "{ev:?}");
    assert!(st.location_find("TE1ST", Some("JO00AA")).unwrap().is_none());
    assert!(st.qrv_get_all("TE1ST", "JO00AA").unwrap().is_empty());
    // the other locator and the other station stay
    assert!(st.location_find("TE1ST", Some("JO00AB")).unwrap().is_some());
    assert_eq!(st.qrv_get_all("TE1ST", "JO00AB").unwrap().len(), 1);
    assert!(st.location_find("TE2ND", Some("JO00AA")).unwrap().is_some());
    assert_eq!(st.qrv_get_all("TE2ND", "JO00AA").unwrap().len(), 1);
    // empty fields delete nothing
    let ev = run_job(
        &paths,
        MaintenanceJob::DeleteStation {
            call: "".into(),
            loc: "".into(),
        },
    );
    assert!(failed(&ev).is_some());
    assert_eq!(st.location_count().unwrap(), 2);
}

#[test]
fn locations_like_needs_two_chars() {
    assert_eq!(locations_pattern("t"), None);
    assert_eq!(locations_pattern("  "), None);
    assert_eq!(locations_pattern(" t "), None);
    assert_eq!(locations_pattern(" te1 ").as_deref(), Some("%TE1%"));
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_bulk_insert_or_update_if_newer(&[
            loc("TE1ST", "JO00AA"),
            loc("TE1ST", "JO00AB"),
            loc("XX9", "JO00AA"),
        ])
        .unwrap();
    let ev = run_job(&paths, MaintenanceJob::Locations("%TE1%".into()));
    match &ev[..] {
        [JobEvent::Locations(v)] => assert_eq!(
            v,
            &vec![
                ("TE1ST".to_string(), "JO00AA".to_string()),
                ("TE1ST".to_string(), "JO00AB".to_string())
            ]
        ),
        other => panic!("{other:?}"),
    }
    // the pattern is bound, not interpolated
    let ev = run_job(&paths, MaintenanceJob::Locations("x' OR '1'='1".into()));
    assert!(
        matches!(&ev[..], [JobEvent::Locations(v)] if v.is_empty()),
        "{ev:?}"
    );
    assert_eq!(paths.stations.location_count().unwrap(), 3);
}

#[test]
fn sizes_listed_for_all_dbs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let ev = run_job(&paths, MaintenanceJob::Sizes);
    let [JobEvent::Sizes(v)] = &ev[..] else {
        panic!("{ev:?}")
    };
    // stations, aircraft, positions, signals, 5 elevation, 5 propagation
    assert_eq!(v.len(), 14);
    let mut names: Vec<_> = v.iter().map(|(d, _, _)| d.file_name()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 14);
    assert!(v
        .iter()
        .any(|(d, _, _)| *d == DbId::Propagation(ElevationModel::Aster1)));
    for (d, p, mb) in v {
        assert!(p.ends_with(&d.file_name()), "{p}");
        assert!(*mb >= 0.0);
    }
    // nothing was created for the databases that do not exist
    assert!(!dir.path().join("aircrafts.db3").exists());
}

#[test]
fn table_descriptions_verbatim() {
    let cases = [
        (
            "Elevation",
            "Holds elevation information per 6digit Maidenhead Locator.",
        ),
        (
            "ElevationPath",
            "Holds elevation path information between two locations.",
        ),
        (
            "ElevationHorizon",
            "Holds elevation horizon information for one location.",
        ),
        (
            "LocalObstruction",
            "Holds local obstruction information for one location.",
        ),
        ("Location", "Holds location information."),
        ("QRV", "Holds QRV information."),
        ("Table_last_updated", "Holds Timestamps of last update."),
        ("PropagationPath", "Holds propagation path information."),
        (
            "PropagationHorizon",
            "Holds propagation horizon information.",
        ),
        (
            "AircraftRegistrations",
            "Holds aircraft registration information.",
        ),
        ("AircraftTypes", "Holds aircraft type information."),
        ("Aircrafts", "Holds basic aircraft information."),
        ("Airlines", "Holds airline information."),
        ("Airports", "Holds airport information."),
    ];
    for (t, d) in cases {
        assert_eq!(table_description(t), d, "{t}");
    }
    assert_eq!(table_description("Nope"), "[not found]");
    assert_eq!(table_description("AircraftPositions"), "[not found]");
}

#[test]
fn tables_job_lists_descriptions_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_bulk_insert_or_update_if_newer(&[loc("TE1ST", "JO00AA")])
        .unwrap();
    let ev = run_job(&paths, MaintenanceJob::Tables(DbId::Stations));
    let [JobEvent::Tables(DbId::Stations, t)] = &ev[..] else {
        panic!("{ev:?}")
    };
    let l = t.iter().find(|r| r.0 == "Location").unwrap();
    assert_eq!((l.1.as_str(), l.2), ("Holds location information.", 1));
    assert!(t.iter().any(|r| r.0 == "QRV" && r.2 == 0));
}

#[test]
fn clear_table_job_clears_only_that_table_and_refreshes() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_bulk_insert_or_update_if_newer(&[loc("TE1ST", "JO00AA")])
        .unwrap();
    paths
        .stations
        .qrv_bulk_insert_or_update_if_newer(&[qrv("TE1ST", "JO00AA", Band::B10G)])
        .unwrap();
    let ev = run_job(
        &paths,
        MaintenanceJob::ClearTable(DbId::Stations, "QRV".into()),
    );
    let tables = ev.iter().find_map(|e| match e {
        JobEvent::Tables(_, t) => Some(t),
        _ => None,
    });
    assert!(tables.unwrap().iter().any(|r| r.0 == "QRV" && r.2 == 0));
    assert_eq!(paths.stations.qrv_count().unwrap(), 0);
    assert_eq!(paths.stations.location_count().unwrap(), 1);
}

#[test]
fn hostile_table_names_are_rejected_and_nothing_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_bulk_insert_or_update_if_newer(&[loc("TE1ST", "JO00AA")])
        .unwrap();
    for bad in [
        "x; DROP TABLE Location",
        "Location; DELETE FROM QRV",
        "sqlite_master",
        "nope",
    ] {
        for job in [
            MaintenanceJob::ClearTable(DbId::Stations, bad.into()),
            MaintenanceJob::View(DbId::Stations, bad.into()),
        ] {
            let ev = run_job(&paths, job);
            assert!(failed(&ev).is_some(), "{bad}: {ev:?}");
        }
    }
    assert_eq!(paths.stations.location_count().unwrap(), 1);
}

#[test]
fn view_is_limited_and_read_only() {
    assert_eq!(VIEW_LIMIT, 10_000);
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .stations
        .location_bulk_insert_or_update_if_newer(&[loc("TE1ST", "JO00AA"), loc("TE2ND", "JO00AA")])
        .unwrap();
    let ev = run_job(
        &paths,
        MaintenanceJob::View(DbId::Stations, "Location".into()),
    );
    let [JobEvent::View { columns, rows }] = &ev[..] else {
        panic!("{ev:?}")
    };
    assert!(columns.iter().any(|c| c.eq_ignore_ascii_case("call")));
    assert_eq!(rows.len(), 2);
    assert_eq!(paths.stations.location_count().unwrap(), 2);
}

#[test]
fn missing_database_file_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let ev = run_job(&paths, MaintenanceJob::Tables(DbId::Positions));
    assert!(failed(&ev).is_some());
    assert!(!dir.path().join("aircraftpositions.db3").exists());
}

// ---- map tiles ----

#[test]
fn delete_map_tiles_removes_only_the_cache() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let cache = data.join("tiles");
    std::fs::create_dir_all(cache.join("osm/3")).unwrap();
    std::fs::write(cache.join("osm/3/1.png"), b"x").unwrap();
    std::fs::write(data.join("settings.toml"), b"keep").unwrap();
    std::fs::write(data.join("stations.db3"), b"keep").unwrap();
    // a foreign directory outside the cache, linked from inside it
    let foreign = root.path().join("foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("precious.txt"), b"keep").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&foreign, cache.join("osm/link")).unwrap();
    clear_tile_cache(&data).unwrap();
    assert!(!cache.exists());
    assert!(data.join("settings.toml").exists());
    assert!(data.join("stations.db3").exists());
    assert!(foreign.join("precious.txt").exists());
    // no cache: nothing to do
    clear_tile_cache(&data).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_cache_dir_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let foreign = root.path().join("foreign");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("precious.txt"), b"keep").unwrap();
    std::os::unix::fs::symlink(&foreign, data.join("tiles")).unwrap();
    assert!(clear_tile_cache(&data).is_err());
    assert!(foreign.join("precious.txt").exists());
}

#[test]
fn tile_deletion_refuses_unsafe_roots() {
    assert!(clear_tile_cache(std::path::Path::new("")).is_err());
    assert!(clear_tile_cache(std::path::Path::new("/")).is_err());
    let root = tempfile::tempdir().unwrap();
    // a file where the data directory should be: nothing is removed
    let f = root.path().join("file");
    std::fs::write(&f, b"x").unwrap();
    assert!(clear_tile_cache(&f).is_err(), "no cache below a file");
    assert!(f.exists());
}

#[test]
fn delete_map_tiles_job_reports_done() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("tiles")).unwrap();
    std::fs::write(dir.path().join("tiles/a.png"), b"x").unwrap();
    let ev = run_job(&paths, MaintenanceJob::DeleteMapTiles);
    assert!(matches!(&ev[..], [JobEvent::Done(_)]), "{ev:?}");
    assert!(!dir.path().join("tiles").exists());
    assert!(dir.path().join("stations.db3").exists());
}
