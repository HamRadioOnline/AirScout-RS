mod common;
use as_elevation::{
    check_area, download_area, prepare_area, ElevationError, ElevationModel, ElevationStore,
};
use as_geo::latlon::GRect;

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().build().unwrap()
}

// JO50 (lon 10..12) and JO60 (lon 12..14)
fn rect() -> GRect {
    GRect::new(50.2, 10.2, 50.8, 13.0)
}

#[test]
fn downloads_square_and_records_sea() {
    let mut server = mockito::Server::new();
    let zip = std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap();
    let ok = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let sea = server
        .mock("GET", "/GLOBE/JO/JO60.zip")
        .with_status(404)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let sea_file = dir.path().join("sea").join("globe-sea-squares.txt");
    let mut msgs = Vec::new();
    let st = prepare_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        rect(),
        &sea_file,
        true,
        &mut |m| msgs.push(m),
    )
    .unwrap();
    ok.assert();
    sea.assert();
    assert!(st.complete);
    assert!(st.missing.is_empty());
    assert_eq!(st.sea_added, 1);
    assert_eq!(std::fs::read_to_string(&sea_file).unwrap(), "JO60\n");
    assert!(store.is_complete(ElevationModel::Globe));
    assert!(msgs.iter().any(|m| m.contains("JO60") && m.contains("sea")));
    // second run: nothing to download, sea square remembered
    let st2 = prepare_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        rect(),
        &sea_file,
        true,
        &mut |_| {},
    )
    .unwrap();
    assert!(st2.complete);
    assert_eq!(st2.sea_added, 0);
}

#[test]
fn without_download_missing_squares_are_listed() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let st = prepare_area(
        &store,
        &client(),
        "http://127.0.0.1:1",
        ElevationModel::Globe,
        GRect::new(50.2, 10.2, 50.8, 11.0),
        &dir.path().join("sea.txt"),
        false,
        &mut |_| {},
    )
    .unwrap();
    assert!(!st.complete);
    assert_eq!(st.missing, vec!["JO50".to_string()]);
    assert!(!store.is_complete(ElevationModel::Globe));
    assert!(!dir.path().join("sea.txt").exists());
}

#[test]
fn download_error_makes_area_incomplete_and_resets_complete_bit() {
    let mut server = mockito::Server::new();
    let zip = std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let broken = server
        .mock("GET", "/GLOBE/JO/JO60.zip")
        .with_status(500)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let sea = dir.path().join("sea.txt");
    // a complete area first
    let first = prepare_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.2, 10.2, 50.8, 11.0),
        &sea,
        true,
        &mut |_| {},
    )
    .unwrap();
    assert!(first.complete);
    assert!(store.is_complete(ElevationModel::Globe));
    // then an area whose square cannot be downloaded: incomplete, not an early error
    let mut msgs = Vec::new();
    let st = prepare_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.2, 12.2, 50.8, 13.0),
        &sea,
        true,
        &mut |m| msgs.push(m),
    )
    .unwrap();
    broken.assert();
    assert!(!st.complete);
    assert_eq!(st.missing, vec!["JO60".to_string()]);
    assert_eq!(st.errors.len(), 1);
    assert!(st.errors[0].starts_with("JO60: "));
    assert!(!store.is_complete(ElevationModel::Globe));
    assert!(msgs
        .iter()
        .any(|m| m.contains("JO60") && m.contains("failed")));
    assert!(!sea.exists() || !std::fs::read_to_string(&sea).unwrap().contains("JO60"));
}

#[test]
fn download_area_leaves_bounds_and_status_alone() {
    let mut server = mockito::Server::new();
    let zip = std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let before = store.bounds();
    let st = download_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.2, 10.2, 50.8, 11.0),
        &dir.path().join("sea.txt"),
        None,
        &mut |_| {},
    )
    .unwrap();
    assert!(st.complete);
    assert_eq!(store.bounds(), before);
    assert!(!store.is_complete(ElevationModel::Globe));
}

#[test]
fn download_area_stops_after_cancel() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut server = mockito::Server::new();
    let zip = std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap();
    let first = server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .expect(1)
        .create();
    let second = server
        .mock("GET", "/GLOBE/JO/JO60.zip")
        .with_status(404)
        .expect(0)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let cancel = AtomicBool::new(false);
    let r = download_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        rect(),
        &dir.path().join("sea.txt"),
        Some(&cancel),
        // cancel while the first square (JO50) is being fetched
        &mut |m| {
            if m.contains("JO50") {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(r, Err(ElevationError::Cancelled)));
    first.assert();
    second.assert();
}

#[test]
fn check_area_downloads_nothing_and_counts_sea() {
    let mut server = mockito::Server::new();
    let zip = std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(zip)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let sea_file = dir.path().join("sea.txt");
    // nothing stored yet: both squares missing, no request
    let st = check_area(
        &store,
        ElevationModel::Globe,
        rect(),
        &sea_file,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(st.missing, vec!["JO50".to_string(), "JO60".to_string()]);
    download_area(
        &store,
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.2, 10.2, 50.8, 11.0),
        &sea_file,
        None,
        &mut |_| {},
    )
    .unwrap();
    std::fs::write(&sea_file, "JO60\n").unwrap();
    let before = store.bounds();
    let st = check_area(
        &store,
        ElevationModel::Globe,
        rect(),
        &sea_file,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert!(st.complete, "{st:?}");
    assert_eq!(store.bounds(), before);
    assert!(!store.is_complete(ElevationModel::Globe));
}
