//! Elevation coverage maps and the station map elevation grid. Synthetic data only.
mod maint_common;
use airscout::coverage::{
    grid_cells, grid_colours, grid_wanted, tile_cell, CoverState, CoverageData, GridRequest,
    GridState, GRID_MIN_ZOOM,
};
use airscout::jobs::JobEvent;
use airscout::maintenance::MaintenanceJob;
use airscout::overlays::MAX_GRID_CELLS;
use airscout::services::{open_data, ServiceEvent};
use as_elevation::{ElevationModel, ElevationTile};
use as_geo::latlon::GRect;
use chrono::Utc;
use maint_common::*;
use std::io::Write;

const STEP_LAT: f64 = 5.5556e-4 / 2.0;
const STEP_LON: f64 = 5.5556e-4;

fn close(a: f64, b: f64) -> bool {
    // the constants of the C# are rounded to 5 digits here
    (a - b).abs() < 1e-7
}

#[test]
fn tile_cell_geometry() {
    // JO50AA: bottom-left 50.0 N, 10.0 E; 1/24 deg high, 2/24 deg wide
    let r = tile_cell("JO50AA").unwrap();
    assert!(close(r.min_lat, 50.0) && close(r.min_lon, 10.0), "{r:?}");
    assert!(close(r.max_lat, 50.0 + 1.0 / 24.0), "{r:?}");
    assert!(close(r.max_lon, 10.0 + 2.0 / 24.0), "{r:?}");
    // lower case is fine, anything but a locator is not
    assert_eq!(tile_cell("jo50aa"), Some(r));
    assert_eq!(tile_cell("JO50"), None);
    assert_eq!(tile_cell("??50AA"), None);
}

#[test]
fn grid_cells_step_and_cap() {
    // a small view inside JO50AA: every cell has the C# step widths and meets the view
    let view = GRect::new(50.0010, 10.0010, 50.0016, 10.0021);
    let cells = grid_cells(view);
    assert!(cells.len() > 2, "{}", cells.len());
    for c in &cells {
        assert!(close(c.max_lat - c.min_lat, STEP_LAT), "{c:?}");
        assert!(close(c.max_lon - c.min_lon, STEP_LON), "{c:?}");
        assert!(
            c.max_lat >= view.min_lat && c.min_lat <= view.max_lat,
            "{c:?}"
        );
        assert!(
            c.max_lon >= view.min_lon && c.min_lon <= view.max_lon,
            "{c:?}"
        );
    }
    // the cells cover the view
    let lo = cells.iter().map(|c| c.min_lat).fold(f64::MAX, f64::min);
    let hi = cells.iter().map(|c| c.max_lat).fold(f64::MIN, f64::max);
    assert!(lo <= view.min_lat && hi >= view.max_lat);
    // a view far too big for 2000 cells still gives a grid, never more than the cap
    let big = GRect::new(50.0, 10.0, 50.1, 10.1);
    let coarse = grid_cells(big);
    assert!(
        !coarse.is_empty() && coarse.len() <= MAX_GRID_CELLS,
        "{}",
        coarse.len()
    );
    assert!(coarse[0].max_lat - coarse[0].min_lat > STEP_LAT);
    // out of range: nothing
    assert!(grid_cells(GRect::new(f64::NAN, 10.0, 50.0, 10.1)).is_empty());
}

#[test]
fn grid_colour_flat_view_no_nan() {
    let r = GRect::new(50.0, 10.0, 50.0003, 10.0006);
    // flat: (elv - min) / (max - min) is 0 / 0 in the C#; the port takes 0, the palette start
    let flat = grid_colours(&[(r, 120), (r, 120)]);
    assert_eq!(flat, vec![[0, 97, 71]; 2]);
    // normalised over the view: the lowest cell gets 0, the highest 10000
    let c = grid_colours(&[(r, 100), (r, 500), (r, 300)]);
    assert_eq!(c[0], [0, 97, 71]);
    assert_eq!(c[1], [255, 255, 255]);
    assert_eq!(c[2], as_elevation::elevation_color(5000.0));
    assert!(grid_colours(&[]).is_empty());
}

#[test]
fn grid_hidden_below_zoom_17() {
    let m = ElevationModel::Srtm1;
    assert_eq!(GRID_MIN_ZOOM, 17.0);
    assert!(!grid_wanted(true, 16.9, m));
    assert!(grid_wanted(true, 17.0, m));
    assert!(!grid_wanted(false, 18.0, m));
    assert!(!grid_wanted(true, 18.0, ElevationModel::None));
    let view = GRect::new(50.0010, 10.0010, 50.0016, 10.0021);
    let mut g = GridState::new(true, m);
    assert_eq!(g.update(view, 16.0), None, "nothing below zoom 17");
    let Some(GridRequest::Run { rect, model }) = g.update(view, 17.0) else {
        panic!("zoom 17 starts the job");
    };
    assert_eq!(model, m);
    assert!(
        rect.min_lat < view.min_lat && rect.max_lon > view.max_lon,
        "padded"
    );
    // still inside the covered area: no new job
    assert_eq!(g.update(view, 17.5), None);
    // panned out of it: a newer job
    let moved = GRect::new(
        view.min_lat + 1.0,
        view.min_lon,
        view.max_lat + 1.0,
        view.max_lon,
    );
    assert!(matches!(
        g.update(moved, 17.0),
        Some(GridRequest::Run { .. })
    ));
    // zooming out hides the grid and cancels the job
    g.set_cells(vec![(view, 5)]);
    assert_eq!(g.cells().len(), 1);
    assert_eq!(g.update(view, 16.0), Some(GridRequest::Cancel));
    assert!(g.cells().is_empty());
    assert_eq!(g.update(view, 16.0), None);
}

#[test]
fn coverage_data_levels_and_cap() {
    let found: Vec<String> = vec!["JO50AA".into(), "JO50AB".into()];
    let missing: Vec<String> = vec!["JO50AC".into(), "JO51AA".into()];
    let d = CoverageData::new(&found, &missing);
    // zoomed in: single tiles
    let v = d.visible(GRect::new(50.0, 10.0, 50.2, 10.4));
    assert_eq!(v.iter().filter(|c| c.1 == CoverState::Found).count(), 2);
    assert_eq!(v.iter().filter(|c| c.1 == CoverState::Missing).count(), 1);
    // far out: 4-char squares, a square with both is mixed
    let v = d.visible(GRect::new(40.0, 0.0, 60.0, 30.0));
    assert!(v.iter().any(|c| c.1 == CoverState::Mixed), "{v:?}");
    assert!(v.len() <= MAX_GRID_CELLS);
    // a world-sized view never exceeds the cap either
    assert!(d.visible(GRect::new(-90.0, -180.0, 90.0, 180.0)).len() <= MAX_GRID_CELLS);
    assert_eq!(
        d.status(),
        "2 tile(s) found, 2 more tile(s) available and missing."
    );
}

// ---- jobs ----

fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn catalogue_zip() -> Vec<u8> {
    let cat = "// header\nJO50AA.loc;2018-02-27 08:26:32Z\nJO50AB.loc;2018-02-27 08:26:33Z\nJN58TD.loc;2018-02-27 08:26:35Z\n";
    zip_bytes(&[("files.cat", cat.as_bytes())])
}

fn tile(index: &str, elv: i16) -> ElevationTile {
    ElevationTile::new(index, 0, 0.0, 0.0, 0, 0.0, 0.0, 1, 1, vec![elv], Utc::now()).unwrap()
}

fn area() -> GRect {
    // the 4-character square JO50 only
    GRect::new(50.0, 10.0, 50.5, 11.5)
}

#[test]
fn coverage_job_splits_found_missing() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .store
        .insert_or_update_if_newer(ElevationModel::Globe, &tile("JO50AA", 5))
        .unwrap();
    let mut server = mockito::Server::new();
    let m = server
        .mock("GET", "/GLOBE/files.zip")
        .match_header("user-agent", as_net::user_agent().as_str())
        .with_body(catalogue_zip())
        .expect(1)
        .create();
    let ev = run_job(
        &paths,
        MaintenanceJob::Coverage {
            model: ElevationModel::Globe,
            base_url: server.url(),
            area: area(),
        },
    );
    m.assert();
    let found_missing = ev.iter().find_map(|e| match e {
        JobEvent::Coverage { found, missing } => Some((found.clone(), missing.clone())),
        _ => None,
    });
    // JN58TD lies outside the area and is not in the answer
    assert_eq!(
        found_missing,
        Some((vec!["JO50AA".to_string()], vec!["JO50AB".to_string()])),
        "{ev:?}"
    );
    // the small catalogue went to a directory of the job in <data>/tmp and was removed again
    assert!(!dir.path().join("tmp").join("coverage-5").exists());
}

#[test]
fn coverage_job_cancelled_sends_nothing_and_error_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/GLOBE/files.zip")
        .with_body(catalogue_zip())
        .create();
    let job = MaintenanceJob::Coverage {
        model: ElevationModel::Globe,
        base_url: server.url(),
        area: area(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let ctx = airscout::jobs::JobCtx::new(5, cancel, tx);
    airscout::maintenance::run(&ctx, &paths, job);
    assert!(
        !rx.try_iter().any(|e| matches!(
            e,
            ServiceEvent::Job {
                event: JobEvent::Coverage { .. },
                ..
            }
        )),
        "a cancelled job reports no result"
    );
    // an unreachable server is a Failed event, not a panic
    let ev = run_job(
        &paths,
        MaintenanceJob::Coverage {
            model: ElevationModel::Globe,
            base_url: "http://127.0.0.1:1".into(),
            area: area(),
        },
    );
    assert!(failed(&ev).is_some(), "{ev:?}");
}

#[test]
fn grid_job_uses_the_selected_model_only() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    // only SRTM3 knows the tile
    paths
        .store
        .insert_or_update_if_newer(ElevationModel::Srtm3, &tile("JO50AA", 321))
        .unwrap();
    let rect = GRect::new(50.0010, 10.0010, 50.0016, 10.0021);
    let grid = |model| {
        let ev = run_job(&paths, MaintenanceJob::ElevationGrid { rect, model });
        ev.into_iter()
            .find_map(|e| match e {
                JobEvent::ElevationGrid(v) => Some(v),
                _ => None,
            })
            .expect("grid event")
    };
    let v = grid(ElevationModel::Srtm3);
    let cells = grid_cells(rect);
    assert_eq!(v.iter().map(|c| c.0).collect::<Vec<_>>(), cells);
    assert!(v.iter().all(|c| c.1 == 321), "{v:?}");
    // GLOBE has no such tile: 0 (the C# indexer sets invalid to zero), no fall-through to SRTM3
    let v = grid(ElevationModel::Globe);
    assert!(v.iter().all(|c| c.1 == 0), "{v:?}");
}

#[test]
fn partly_covered_view_is_not_washed_out() {
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    paths
        .store
        .insert_or_update_if_newer(ElevationModel::Srtm3, &tile("JO50AA", 321))
        .unwrap();
    // JO50AA ends at lon 10 + 2/24; the view reaches into the missing JO50AB
    let edge = 10.0 + 2.0 / 24.0;
    let rect = GRect::new(50.0010, edge - 0.0010, 50.0016, edge + 0.0010);
    let ev = run_job(
        &paths,
        MaintenanceJob::ElevationGrid {
            rect,
            model: ElevationModel::Srtm3,
        },
    );
    let Some(JobEvent::ElevationGrid(v)) = ev
        .into_iter()
        .find(|e| matches!(e, JobEvent::ElevationGrid(_)))
    else {
        panic!("grid event");
    };
    assert!(v.iter().any(|c| c.1 == 321) && v.iter().any(|c| c.1 == 0));
    assert!(v.iter().all(|c| c.1 == 0 || c.1 == 321), "{v:?}");
    let colours = grid_colours(&v);
    let real = v.iter().position(|c| c.1 == 321).unwrap();
    assert_eq!(
        colours[real],
        [255, 255, 255],
        "321 is the top of the range, not washed out"
    );
}

#[test]
fn grid_colours_are_normalised_over_the_visible_cells() {
    let view = GRect::new(50.0010, 10.0010, 50.0016, 10.0021);
    let inside = |lat: f64| GRect::new(lat, 10.0012, lat + 0.0001, 10.0014);
    let mut g = GridState::new(true, ElevationModel::Srtm1);
    g.set_cells(vec![
        (inside(50.0011), 100),
        (inside(50.0013), 500),
        (GRect::new(51.0, 11.0, 51.0001, 11.0001), 5000), // outside the view
    ]);
    let v = g.visible(view);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].colour, [0, 97, 71]);
    assert_eq!(v[1].colour, [255, 255, 255]);
}

/// The real services keep a cancel flag per maintenance job: the job of the id stops, and the
/// download of the catalogue is not followed by a result.
#[test]
fn real_services_cancel_maintenance_by_job_id() {
    use airscout::app::{RealServices, Services};
    use std::time::Duration;
    let run = |cancel: bool| {
        let dir = tempfile::tempdir().unwrap();
        let paths = open_data(dir.path()).unwrap();
        let mut server = mockito::Server::new();
        let body = catalogue_zip();
        let mocks: Vec<_> = ["/GLOBE/files.zip", "/SRTM3/files.zip"]
            .into_iter()
            .map(|p| {
                let body = body.clone();
                server
                    .mock("GET", p)
                    .with_chunked_body(move |w| {
                        // the answer takes a moment, so the cancel arrives while the job waits
                        std::thread::sleep(Duration::from_millis(400));
                        w.write_all(&body)
                    })
                    .create()
            })
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut svc = RealServices::new(paths, tx);
        let job = MaintenanceJob::Coverage {
            model: ElevationModel::Globe,
            base_url: server.url(),
            area: area(),
        };
        let other = MaintenanceJob::Coverage {
            model: ElevationModel::Srtm3,
            base_url: server.url(),
            area: area(),
        };
        svc.run_maintenance(7, job);
        svc.run_maintenance(8, other);
        if cancel {
            svc.cancel_maintenance(7);
            svc.cancel_maintenance(99); // unknown: ignored
        }
        let mut got = Vec::new();
        while let Ok(ServiceEvent::Job { id, event }) = rx.recv_timeout(Duration::from_secs(5)) {
            if matches!(event, JobEvent::Coverage { .. }) {
                got.push(id);
            }
            if got.len() == 2 || (cancel && got == [8]) {
                break;
            }
        }
        drop(mocks);
        got.sort_unstable();
        got
    };
    assert_eq!(run(false), vec![7, 8]);
    assert_eq!(
        run(true),
        vec![8],
        "only the job that was not cancelled answers"
    );
}
