//! Maintenance (Task 12): the confirmation flow, texts and copyright. Synthetic data only.
mod maint_common;
use airscout::jobs::JobEvent;
use airscout::maintenance::{DbId, MaintenanceJob};
use airscout::services::ServiceEvent;
use as_aircraft::enums::LifeMode;
use as_elevation::ElevationModel;
use maint_common::*;
// ---- confirmation flow (pure state) ----

#[test]
fn delete_all_needs_confirmation() {
    let (mut st, rec) = state();
    let job = MaintenanceJob::ClearTable(DbId::Stations, "QRV".into());
    st.request_maintenance(job.clone()).unwrap();
    // asked, not sent
    assert_eq!(ids(&rec), 0);
    assert_eq!(st.maint.confirm, Some(job.clone()));
    // No (or closing) deletes nothing
    st.cancel_confirmation();
    assert!(st.maint.confirm.is_none());
    assert!(st.confirm_maintenance().is_err());
    assert_eq!(ids(&rec), 0);
    // Yes sends it, once
    st.request_maintenance(job.clone()).unwrap();
    st.confirm_maintenance().unwrap();
    assert_eq!(rec.0.borrow().maint.len(), 1);
    assert_eq!(rec.0.borrow().maint[0].1, job);
    assert!(st.maint.confirm.is_none());
    assert!(st.confirm_maintenance().is_err());
    assert_eq!(ids(&rec), 1);
}

#[test]
fn every_destructive_job_is_confirmed_first() {
    for job in [
        MaintenanceJob::DeleteElevationPaths,
        MaintenanceJob::DeletePropagationPaths,
        MaintenanceJob::DeleteMapTiles,
        MaintenanceJob::DeleteStation {
            call: "TE1ST".into(),
            loc: "JO00AA".into(),
        },
        MaintenanceJob::ClearTable(DbId::Aircraft, "Airports".into()),
        MaintenanceJob::View(DbId::Stations, "Location".into()),
    ] {
        assert!(job.confirmation().is_some(), "{job:?}");
        let (mut st, rec) = state();
        st.request_maintenance(job).unwrap();
        assert_eq!(ids(&rec), 0);
        assert!(st.maint.confirm.is_some());
    }
    // reads need no question
    let (mut st, rec) = state();
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    assert_eq!(ids(&rec), 1);
    assert!(st.maint.confirm.is_none());
}

#[test]
fn maintenance_refused_while_playing() {
    let (mut st, rec) = state();
    st.play().unwrap();
    for job in [
        MaintenanceJob::DeleteElevationPaths,
        MaintenanceJob::DeleteMapTiles,
        MaintenanceJob::ClearTable(DbId::Stations, "QRV".into()),
    ] {
        assert!(st.request_maintenance(job).is_err());
        assert!(st.maint.confirm.is_none());
    }
    assert_eq!(ids(&rec), 0);
    // playing starts after the question: the answer is refused
    st.pause();
    st.request_maintenance(MaintenanceJob::DeletePropagationPaths)
        .unwrap();
    st.play().unwrap();
    assert!(st.confirm_maintenance().is_err());
    assert_eq!(ids(&rec), 0);
    assert!(st.maint.confirm.is_none());
}

#[test]
fn maintenance_refused_while_a_job_runs() {
    let (mut st, rec) = state();
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    assert_eq!(ids(&rec), 1);
    // a second job while the first one runs
    assert!(st
        .request_maintenance(MaintenanceJob::DeleteMapTiles)
        .is_err());
    assert_eq!(ids(&rec), 1);
    // its answer frees the state
    let id = rec.0.borrow().maint[0].0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Sizes(vec![(DbId::Stations, "x".into(), 1.0)]),
    });
    assert_eq!(st.maint.sizes.len(), 1);
    st.request_maintenance(MaintenanceJob::DeleteMapTiles)
        .unwrap();
    st.confirm_maintenance().unwrap();
    assert_eq!(ids(&rec), 2);
}

#[test]
fn maintenance_refused_while_background_workers_run() {
    let (mut st, rec) = state();
    rec.0.borrow_mut().background_jobs = 1;
    // a delete/clear issued before the workers are idle is refused with a status message (#155)
    assert_eq!(
        st.request_maintenance(MaintenanceJob::DeletePropagationPaths)
            .unwrap_err(),
        "wait for the background workers to stop"
    );
    assert_eq!(ids(&rec), 0);
    // idle again: the delete is allowed
    rec.0.borrow_mut().background_jobs = 0;
    st.request_maintenance(MaintenanceJob::DeletePropagationPaths)
        .unwrap();
    st.confirm_maintenance().unwrap();
    assert_eq!(ids(&rec), 1);
}

#[test]
fn sizes_read_runs_while_background_workers_stop() {
    let (mut st, rec) = state();
    rec.0.borrow_mut().background_jobs = 1;
    // reading the database sizes is read-only, so it is not refused while workers stop (#155)
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    assert_eq!(ids(&rec), 1);
}

#[test]
fn maintenance_refused_while_history_loads() {
    let (mut st, rec) = state();
    st.enter_history().unwrap();
    assert_eq!(st.life_mode, LifeMode::History);
    assert!(st
        .request_maintenance(MaintenanceJob::DeleteElevationPaths)
        .is_err());
    assert_eq!(ids(&rec), 0);
    // the history load is still the current analysis job
    assert_eq!(rec.0.borrow().analysis.len(), 1);
}

#[test]
fn stale_and_foreign_events_are_dropped() {
    let (mut st, rec) = state();
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    let id = rec.0.borrow().maint[0].0;
    st.on_event(ServiceEvent::Job {
        id: id + 1,
        event: JobEvent::Sizes(vec![(DbId::Stations, "x".into(), 1.0)]),
    });
    assert!(st.maint.sizes.is_empty());
    assert!(st.maint.running.is_some());
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Failed("boom".into()),
    });
    assert!(st.maint.running.is_none());
    assert!(st.status.contains("boom"));
}

// ---- copyright ----

#[test]
fn copyright_texts_are_the_settings_defaults() {
    assert!(ElevationModel::Globe
        .copyright()
        .contains("GLOBE Task Team and others"));
    assert!(ElevationModel::Srtm3
        .copyright()
        .contains("OTSRTM.042013.4326.1"));
    assert!(ElevationModel::Srtm1
        .copyright()
        .contains("OTSRTM.082015.4326.1"));
    assert!(ElevationModel::Srtm3
        .copyright()
        .contains("NSF Award Numbers 1226353 & 1225810"));
    for m in [ElevationModel::Aster3, ElevationModel::Aster1] {
        assert!(
            m.copyright()
                .contains("ASTER Global Digital Elevation Model"),
            "{m:?}"
        );
    }
    assert!(ElevationModel::None.copyright().is_empty());
}

#[test]
fn db_names_and_descriptions_verbatim() {
    // StationDatabase.cs:48-51, AircraftDatabase.cs:51-54, AircraftPositionDatabase.cs:43-44,
    // SignalDatabase.cs:40-41, ElevationDatabase.cs:126-131, PropagationDatabase.cs:89-93
    assert_eq!(DbId::Stations.title(), "ScoutBase Station Database");
    assert_eq!(
        DbId::Stations.description(),
        "The Scoutbase Station Database is containing location and QRV infos. Each information has a unique key: \ncallsign and 6-digit Maidenhead locator square. \nThe database is periodically updated from a global web resource. The user can modify / add entries via user interface."
    );
    assert_eq!(DbId::Aircraft.title(), "AirScout Aircraft Database");
    assert_eq!(
        DbId::Aircraft.description(),
        "Holds all aircraft, airline and airport information.\nThe information is updated periodically from a global web resource.\nThe user cannot change any item."
    );
    assert_eq!(
        DbId::Positions.title(),
        "AirScout Aircraft Position Database"
    );
    assert_eq!(
        DbId::Positions.description(),
        "Holds aircraft position history."
    );
    assert_eq!(DbId::Signals.title(), "Scoutbase Signal Database");
    assert_eq!(
        DbId::Signals.description(),
        "Holds recorded signal strength information"
    );
    for m in ElevationModel::ALL {
        assert_eq!(DbId::Elevation(m).title(), "ScoutBase Elevation Database");
        assert_eq!(
            DbId::Elevation(m).description(),
            "The Scoutbase Elevation Database is containing various elevation information.\nThe basic elevation information is kept unique per  6digit-Maidenhead Locator and is updated periodically from a global web resource.\nThe path and horizon information are unique per one oder between two geographical locations.\nThese values are (pre-)calculated and stored at runtime.\nAll values are based on a distinct elevation model either GLOBE, SRTM3, SRTM1 or ASTER."
        );
        assert_eq!(
            DbId::Propagation(m).title(),
            "ScoutBase Propagation Database"
        );
        assert_eq!(
            DbId::Propagation(m).description(),
            "The Scoutbase Propagation Database is containing propagation path and horizon information.\nThe info is unique for one single or between two geographical locations, heights, frequency, F1-Clearance and calculation stepwidth.\nAll calculations are based on a distinct elevation model GLOBE, SRTM3, SRTM1 or ASTER.\nAll values are (pre-)calculated and stored at runtime."
        );
    }
    // the model tells the databases of one kind apart in the lists
    assert_eq!(
        DbId::Elevation(ElevationModel::Srtm1).label(),
        "ScoutBase Elevation Database SRTM1"
    );
}

#[test]
fn play_refused_while_a_maintenance_job_runs() {
    let (mut st, rec) = state();
    st.request_maintenance(MaintenanceJob::Sizes).unwrap();
    assert!(st.play().is_err());
    assert!(!st.is_playing());
    assert!(st.status.contains("maintenance"), "{}", st.status);
    // the answer frees Play again
    let id = rec.0.borrow().maint[0].0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Sizes(Vec::new()),
    });
    st.play().unwrap();
}
