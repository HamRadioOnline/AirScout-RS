//! Maintenance dialogs through egui_kittest: Options Database tab, table list, station delete.
mod maint_common;
use airscout::jobs::JobEvent;
use airscout::maintenance::{DbId, MaintenanceJob};
use airscout::services::ServiceEvent;
use airscout::ui::dialogs::db_maintenance::find_next;
use airscout::ui::options::OptionsTab;
use egui_kittest::kittest::Queryable;
use maint_common::*;
#[test]
fn options_database_tab_buttons_ask_first() {
    let (mut h, rec) = harness();
    h.get_by_label("Options").click();
    h.run();
    // opening the dialog reads the sizes
    assert!(matches!(rec.0.borrow().maint[0].1, MaintenanceJob::Sizes));
    let id = rec.0.borrow().maint[0].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Sizes(
            DbId::all()
                .into_iter()
                .map(|d| (d, format!("/data/{}", d.file_name()), 2.0))
                .collect(),
        ),
    });
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Database;
    h.run();
    h.get_by_label_contains("Total");
    h.get_by_label("Delete All Elevation Paths").click();
    h.run();
    assert_eq!(rec.0.borrow().maint.len(), 1, "asked, not run");
    h.get_by_label_contains("delete all precalculated elevation paths");
    h.get_by_label("No").click();
    h.run();
    assert_eq!(rec.0.borrow().maint.len(), 1);
    h.get_by_label("Delete All Elevation Paths").click();
    h.run();
    h.get_by_label("Yes").click();
    h.run();
    let log = rec.0.borrow();
    assert_eq!(log.maint.len(), 2);
    assert_eq!(log.maint[1].1, MaintenanceJob::DeleteElevationPaths);
}

#[test]
fn maintenance_dialog_delete_all_and_view_ask_first() {
    let (mut h, rec) = harness();
    h.state_mut().open_db_maintenance(DbId::Stations);
    h.run();
    assert_eq!(
        rec.0.borrow().maint[0].1,
        MaintenanceJob::Tables(DbId::Stations)
    );
    let id = rec.0.borrow().maint[0].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Tables(
            DbId::Stations,
            vec![("Location".into(), "Holds location information.".into(), 3)],
        ),
    });
    h.run();
    h.get_by_label("Holds location information.");
    h.get_by_label("Delete All").click();
    h.run();
    h.get_by_label_contains("Do you really want to delete all entries of table Location?");
    assert_eq!(rec.0.borrow().maint.len(), 1);
    h.get_by_label("No").click();
    h.run();
    assert_eq!(rec.0.borrow().maint.len(), 1, "No deletes nothing");
    h.get_by_label("Delete All").click();
    h.run();
    h.get_by_label("Yes").click();
    h.run();
    assert_eq!(
        rec.0.borrow().maint[1].1,
        MaintenanceJob::ClearTable(DbId::Stations, "Location".into())
    );
    // the answer refreshes the counts
    let id = rec.0.borrow().maint[1].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Tables(
            DbId::Stations,
            vec![("Location".into(), "Holds location information.".into(), 0)],
        ),
    });
    h.run();
    // View: CAUTION first, then a read-only grid
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("CAUTION! Pressing \"View\"");
    h.get_by_label("Yes").click();
    h.run();
    assert_eq!(
        rec.0.borrow().maint[2].1,
        MaintenanceJob::View(DbId::Stations, "Location".into())
    );
    let id = rec.0.borrow().maint[2].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::View {
            columns: vec!["Call".into(), "Loc".into()],
            rows: vec![vec!["TE1ST".into(), "JO00AA".into()]],
        },
    });
    h.run();
    h.get_by_label("Location (read only)");
    h.get_by_label("TE1ST");
    h.get_by_label("Find Next");
}

#[test]
fn find_next_wraps_and_ignores_case() {
    let rows: Vec<Vec<String>> = [["te1st", "JO00AA"], ["XX9", "te1xx"], ["AB", "CD"]]
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    assert_eq!(find_next(&rows, "TE1", None), Some(((0, 0), false)));
    assert_eq!(find_next(&rows, "TE1", Some((0, 0))), Some(((1, 1), false)));
    // after the last hit it wraps to the first
    assert_eq!(find_next(&rows, "TE1", Some((1, 1))), Some(((0, 0), true)));
    assert_eq!(find_next(&rows, "ab", Some((1, 0))), Some(((2, 0), false)));
    assert_eq!(find_next(&rows, "zzz", None), None);
    assert_eq!(find_next(&rows, "", None), None);
    assert_eq!(find_next(&[], "a", None), None);
}

#[test]
fn delete_station_dialog_looks_up_and_asks_first() {
    let (mut h, rec) = harness();
    h.state_mut().open_delete_station();
    h.run();
    // one character: no lookup
    h.state_mut().maint.station_dlg.as_mut().unwrap().call = "T".into();
    h.run();
    assert!(rec.0.borrow().maint.is_empty());
    h.state_mut().maint.station_dlg.as_mut().unwrap().call = "TE1".into();
    h.run();
    assert_eq!(
        rec.0.borrow().maint[0].1,
        MaintenanceJob::Locations("%TE1%".into())
    );
    let id = rec.0.borrow().maint[0].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Locations(vec![
            ("TE1ST".into(), "JO00AA".into()),
            ("TE1ST".into(), "JO00AB".into()),
            ("TE1XY".into(), "JO00CC".into()),
        ]),
    });
    // typing on asks again; the answer lists the locators of the full call
    h.state_mut().maint.station_dlg.as_mut().unwrap().call = "TE1ST".into();
    h.run();
    assert_eq!(
        rec.0.borrow().maint[1].1,
        MaintenanceJob::Locations("%TE1ST%".into())
    );
    let id = rec.0.borrow().maint[1].0;
    h.state_mut().on_event(ServiceEvent::Job {
        id,
        event: JobEvent::Locations(vec![
            ("TE1ST".into(), "JO00AA".into()),
            ("TE1ST".into(), "JO00AB".into()),
        ]),
    });
    h.run();
    {
        let st = h.state().maint.station_dlg.as_ref().unwrap();
        // the locators of the chosen call only
        assert_eq!(st.locs().cloned().collect::<Vec<_>>(), ["JO00AA", "JO00AB"]);
        assert_eq!(st.loc, "JO00AA");
    }
    h.get_by_label_contains("CAUTION: You can only remove own entries permanently.");
    h.get_by_label("Delete").click();
    h.run();
    assert!(
        rec.0
            .borrow()
            .maint
            .iter()
            .all(|(_, j)| !matches!(j, MaintenanceJob::DeleteStation { .. })),
        "asked, not run"
    );
    h.get_by_label("Yes").click();
    h.run();
    let log = rec.0.borrow();
    assert_eq!(
        log.maint.last().unwrap().1,
        MaintenanceJob::DeleteStation {
            call: "TE1ST".into(),
            loc: "JO00AA".into()
        }
    );
    drop(log);
    // its answer clears the list and asks again
    answer(&mut h, &rec, &MaintenanceJob::Sizes);
    h.run();
    assert!(matches!(
        rec.0.borrow().maint.last().unwrap().1,
        MaintenanceJob::Locations(_)
    ));
}

#[test]
fn copyright_and_license_windows() {
    let (mut h, _rec) = harness();
    h.get_by_label("Options").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Elevation;
    h.run();
    assert_eq!(h.get_all_by_label("Copyright").count(), 5);
    h.get_all_by_label("Copyright").next().unwrap().click();
    h.run();
    h.get_by_label("GLOBE Copyright");
    h.get_by_label_contains("GLOBE Task Team and others");
    h.get_by_label("Close").click();
    h.run();
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Info;
    h.run();
    h.get_by_label("Show license").click();
    h.run();
    h.get_by_label_contains("GNU GENERAL PUBLIC LICENSE");
}
