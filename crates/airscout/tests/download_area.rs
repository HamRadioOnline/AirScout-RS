//! "Download data for the area": the pure plan (tile/square counts and size estimates) and the
//! job that preloads map tiles from the provider's mirror and elevation squares for every enabled
//! model. Localhost mock servers and synthetic data only.
mod maint_common;
use airscout::download_area::{plan, ElevPlan, ELEV_SQUARE_BYTES, MAP_TILE_BYTES};
use airscout::jobs::JobEvent;
use airscout::maintenance::MaintenanceJob;
use airscout::map_preloader::{map_mirror_ok, PRELOAD_MIN_ZOOM};
use airscout::services::open_data;
use airscout::tiles::store_path;
use airscout::ui::dialogs::download_area::DownloadAreaDlg;
use as_elevation::ElevationModel;
use as_geo::latlon::GRect;
use as_maps::{preload_tile_count, TileStore};
use as_settings::{ElevationEnabled, Settings};
use maint_common::*;

/// A PNG signature and a few bytes, as the map preloader stores without decoding.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nTEST-TILE";

fn area() -> GRect {
    GRect::new(50.0, 10.0, 50.1, 10.1)
}

fn globe() -> ElevationEnabled {
    ElevationEnabled::only(ElevationModel::Globe)
}

// ---- plan ----

#[test]
fn plan_counts_map_tiles_and_elevation_squares() {
    let p = plan(&area(), Some(5), true, &globe());
    let maps = p.maps.unwrap();
    assert_eq!(maps.zoom, 5);
    assert_eq!(maps.tiles, preload_tile_count(&area(), PRELOAD_MIN_ZOOM, 5));
    assert_eq!(maps.bytes, maps.tiles * MAP_TILE_BYTES);
    assert_eq!(
        p.elevation,
        vec![ElevPlan {
            model: ElevationModel::Globe,
            squares: 1, // JO50
            bytes: ELEV_SQUARE_BYTES,
        }]
    );
    assert_eq!(p.total_bytes(), maps.bytes + ELEV_SQUARE_BYTES);
    assert!(p.enough(p.total_bytes()));
    assert!(!p.enough(p.total_bytes() - 1));
}

#[test]
fn plan_without_parts_is_empty() {
    let p = plan(&area(), None, false, &globe());
    assert_eq!(p.maps, None);
    assert!(p.elevation.is_empty());
    assert_eq!(p.total_bytes(), 0);
    assert!(p.enough(0));
}

#[test]
fn plan_only_enabled_elevation_models() {
    let e = ElevationEnabled {
        srtm3: true,
        ..ElevationEnabled::default()
    };
    let p = plan(&area(), None, true, &e);
    let models: Vec<ElevationModel> = p.elevation.iter().map(|m| m.model).collect();
    assert_eq!(models, vec![ElevationModel::Globe, ElevationModel::Srtm3]);
    assert_eq!(p.total_bytes(), 2 * ELEV_SQUARE_BYTES);
}

#[test]
fn plan_maps_none_when_no_mirror() {
    let p = plan(&area(), None, true, &globe());
    assert_eq!(p.maps, None);
}

/// The dialog shows maps only when the provider has a usable mirror.
#[test]
fn dialog_plan_skips_maps_without_mirror() {
    let s = Settings::default();
    let dlg = DownloadAreaDlg::new(&s);
    let (p, notice) = dlg.plan_for(&s);
    assert!(p.maps.is_some());
    assert!(notice.is_none());

    let mut other = Settings::default();
    other.gui.map_provider = "OpenTopoMap".into();
    let (p, notice) = dlg.plan_for(&other);
    assert!(p.maps.is_none());
    assert!(
        notice.as_deref().unwrap().contains("no mirror"),
        "{notice:?}"
    );
}

// ---- map_mirror_ok ----

#[test]
fn map_mirror_ok_accepts_the_mirror() {
    assert!(map_mirror_ok("OpenStreetMap", as_maps::MIRROR_URL).is_ok());
    assert!(map_mirror_ok("OpenStreetMap", "http://127.0.0.1:1234/osm/").is_ok());
}

#[test]
fn map_mirror_ok_refuses_provider_without_mirror() {
    let e = map_mirror_ok("OpenTopoMap", as_maps::MIRROR_URL).unwrap_err();
    assert!(e.contains("no mirror"), "{e}");
}

#[test]
fn map_mirror_ok_refuses_provider_host_as_mirror() {
    let e = map_mirror_ok("OpenStreetMap", "http://tile.openstreetmap.org/").unwrap_err();
    assert!(e.contains("map provider's server"), "{e}");
}

// ---- the job ----

/// A minimal `.loc` tile JSON (as the square zips carry them).
fn loc_json() -> String {
    r#"{"TileIndex":"JO50AA","MinElv":0,"MinLat":50.0,"MinLon":10.0,"MaxElv":100,"MaxLat":50.1,"MaxLon":10.1,"Rows":2,"Columns":2,"Elv":[[1,2],[3,4]],"LastUpdated":"2018-02-27T08:26:32Z"}"#
        .to_string()
}

fn square_zip(name: &str, loc: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file(name, opts).unwrap();
        std::io::Write::write_all(&mut z, loc.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn done_text(events: &[JobEvent]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        JobEvent::Done(s) => Some(s.as_str()),
        _ => None,
    })
}

#[test]
fn run_stores_maps_and_elevation() {
    let mut maps = mockito::Server::new();
    let map_mock = maps
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/\d+/\d+/\d+\.png$".into()),
        )
        .with_body(PNG)
        .expect(1)
        .create();
    let mut elev = mockito::Server::new();
    let elev_mock = elev
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(square_zip("JO50AA.loc", &loc_json()))
        .create();

    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let job = MaintenanceJob::DownloadArea {
        maps: Some((maps.url(), 5)),
        elevation: Some(elev.url()),
        models: globe(),
        area: area(),
    };
    let events = run_job(&paths, job);

    map_mock.assert();
    elev_mock.assert();
    assert!(failed(&events).is_none(), "{events:?}");
    assert!(done_text(&events).is_some(), "{events:?}");

    // map tiles landed in the tile store
    let store = TileStore::open(&store_path(dir.path())).unwrap();
    assert_eq!(store.count().unwrap(), 1);
    // the elevation square landed in the elevation store
    assert!(paths
        .store
        .tile_exists(ElevationModel::Globe, "JO50AA")
        .unwrap());
}

#[test]
fn run_partial_failure_reports_and_keeps_the_rest() {
    // every map tile 404s: the map part fails, the elevation part still runs
    let mut maps = mockito::Server::new();
    let map_mock = maps
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/\d+/\d+/\d+\.png$".into()),
        )
        .with_status(404)
        .expect(1)
        .create();
    let mut elev = mockito::Server::new();
    let elev_mock = elev
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(square_zip("JO50AA.loc", &loc_json()))
        .create();

    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let events = run_job(
        &paths,
        MaintenanceJob::DownloadArea {
            maps: Some((maps.url(), 5)),
            elevation: Some(elev.url()),
            models: globe(),
            area: area(),
        },
    );

    map_mock.assert();
    elev_mock.assert();
    // the failed map tiles do not abort the job: no Failed, elevation is stored
    assert!(failed(&events).is_none(), "{events:?}");
    assert!(paths
        .store
        .tile_exists(ElevationModel::Globe, "JO50AA")
        .unwrap());
    let store = TileStore::open(&store_path(dir.path())).unwrap();
    assert_eq!(store.count().unwrap(), 0);
    // the failure is reported through the progress text
    assert!(events
        .iter()
        .any(|e| matches!(e, JobEvent::Progress(s) if s.contains("failed"))));
}

#[test]
fn run_elevation_only_skips_maps() {
    let mut elev = mockito::Server::new();
    let elev_mock = elev
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(square_zip("JO50AA.loc", &loc_json()))
        .create();
    let dir = tempfile::tempdir().unwrap();
    let paths = open_data(dir.path()).unwrap();
    let events = run_job(
        &paths,
        MaintenanceJob::DownloadArea {
            maps: None,
            elevation: Some(elev.url()),
            models: globe(),
            area: area(),
        },
    );
    elev_mock.assert();
    assert!(failed(&events).is_none(), "{events:?}");
    assert!(paths
        .store
        .tile_exists(ElevationModel::Globe, "JO50AA")
        .unwrap());
    // no map tile store written
    assert_eq!(
        TileStore::open(&store_path(dir.path()))
            .unwrap()
            .count()
            .unwrap(),
        0
    );
}
