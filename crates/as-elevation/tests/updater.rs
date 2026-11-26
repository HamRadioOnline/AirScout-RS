mod common;
use as_elevation::{
    get_locs_from_rect, update_square, Catalogue, ElevationError, ElevationModel, ElevationStore,
    UpdateProgress,
};
use as_geo::latlon::GRect;
use std::io::Write;

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().build().unwrap()
}

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
    let cat = "// header line\n\nJO50AA.loc;2018-02-27 08:26:32Z\nJO50AB.loc;2018-02-27 08:26:33Z\nJO61UF.loc;2018-02-27 08:26:34Z\nJN58TD.loc;2018-02-27 08:26:35Z\nbroken line\n";
    zip_bytes(&[("files.cat", cat.as_bytes())])
}

fn fixture_zip() -> Vec<u8> {
    std::fs::read(common::fixture_dir().join("GLOBE/JO/JO50.zip")).unwrap()
}

#[test]
fn locs_from_rect_like_original() {
    let sq = get_locs_from_rect(GRect::new(50.0, 10.0, 51.0, 12.0), 2).unwrap();
    assert_eq!(sq, vec!["JO50", "JO60", "JO51", "JO61"]);
    assert!(get_locs_from_rect(GRect::new(51.0, 10.0, 50.0, 12.0), 2)
        .unwrap()
        .is_empty());
    assert!(get_locs_from_rect(GRect::new(50.0, 10.0, 51.0, 12.0), 4).is_err());
}

#[test]
fn catalogue_filters_by_rect() {
    let mut server = mockito::Server::new();
    let m = server
        .mock("GET", "/GLOBE/files.zip")
        .with_body(catalogue_zip())
        .create();
    let dir = tempfile::tempdir().unwrap();
    let cat = Catalogue::fetch(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.0, 10.0, 51.0, 12.0),
        dir.path(),
    )
    .unwrap();
    m.assert();
    let keys: Vec<_> = cat.files.keys().cloned().collect();
    assert_eq!(keys, vec!["JO50AA", "JO50AB", "JO61UF"]);
    assert_eq!(
        cat.files["JO50AB"].to_rfc3339(),
        "2018-02-27T08:26:33+00:00"
    );
}

#[test]
fn catalogue_uses_cached_file_on_304() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let rect = GRect::new(50.0, 10.0, 51.0, 12.0);
    let m1 = server
        .mock("GET", "/GLOBE/files.zip")
        .with_body(catalogue_zip())
        .expect(1)
        .create();
    Catalogue::fetch(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        rect,
        dir.path(),
    )
    .unwrap();
    m1.assert();
    m1.remove();
    let m2 = server
        .mock("GET", "/GLOBE/files.zip")
        .match_header(
            "if-modified-since",
            mockito::Matcher::Regex(r"^\w{3}, \d{2} \w{3} \d{4} ".into()),
        )
        .with_status(304)
        .create();
    let cat = Catalogue::fetch(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        rect,
        dir.path(),
    )
    .unwrap();
    m2.assert();
    assert_eq!(cat.files.len(), 3);
}

#[test]
fn update_square_writes_all_tiles() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(fixture_zip())
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let mut events = Vec::new();
    let n = update_square(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        "JO50",
        &store,
        None,
        &mut |p| events.push(p),
    )
    .unwrap();
    assert_eq!(n, 576);
    assert_eq!(store.count(ElevationModel::Globe).unwrap(), 576);
    assert!(matches!(
        events.last(),
        Some(UpdateProgress::Square { tiles: 576, .. })
    ));
}

#[test]
fn http_404_is_err_and_store_unchanged() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_status(404)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let r = update_square(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        "JO50",
        &store,
        None,
        &mut |_| {},
    );
    // 404 = the server has no file for this square (open sea), distinct from other HTTP errors
    assert!(matches!(r, Err(ElevationError::NotFound(_))), "{r:?}");
    assert_eq!(store.count(ElevationModel::Globe).unwrap(), 0);
}

#[test]
fn truncated_zip_is_err_and_store_unchanged() {
    let mut server = mockito::Server::new();
    let z = fixture_zip();
    server
        .mock("GET", "/GLOBE/JO/JO50.zip")
        .with_body(&z[..z.len() / 2])
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let r = update_square(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        "JO50",
        &store,
        None,
        &mut |_| {},
    );
    assert!(matches!(r, Err(ElevationError::Corrupt(_))), "{r:?}");
    assert_eq!(store.count(ElevationModel::Globe).unwrap(), 0);
}

#[test]
fn malformed_loc_rolls_back_whole_square() {
    let good = std::fs::read(common::fixture_dir().join("SRTM3/JO/JO50AA.loc")).unwrap();
    let body = zip_bytes(&[("JO50AA.loc", &good), ("JO50AB.loc", b"{ not json")]);
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/SRTM3/JO/JO50.zip")
        .with_body(body)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let r = update_square(
        &client(),
        &server.url(),
        ElevationModel::Srtm3,
        "JO50",
        &store,
        None,
        &mut |_| {},
    );
    assert!(matches!(r, Err(ElevationError::Parse(_))), "{r:?}");
    assert_eq!(store.count(ElevationModel::Srtm3).unwrap(), 0);
}

#[test]
fn invalid_square_is_err() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let r = update_square(
        &client(),
        "http://127.0.0.1:9",
        ElevationModel::Globe,
        "J/50",
        &store,
        None,
        &mut |_| {},
    );
    assert!(
        matches!(r, Err(ElevationError::InvalidArgument(_))),
        "{r:?}"
    );
}

#[test]
#[ignore = "hits the real airscout.eu server"]
fn live_download_globe_jo50() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let n = update_square(
        &client(),
        as_elevation::DEFAULT_BASE_URL,
        ElevationModel::Globe,
        "JO50",
        &store,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(n, 576);
}

/// Review M7: server data with non-ASCII names must not panic (byte slicing inside a char).
#[test]
fn catalogue_with_non_ascii_name_does_not_panic() {
    let mut server = mockito::Server::new();
    let cat = "JO5ÖAA.loc;2018-02-27 08:26:32Z\nJO50AA.loc;2018-02-27 08:26:32Z\n";
    let _m = server
        .mock("GET", "/GLOBE/files.zip")
        .with_body(zip_bytes(&[("files.cat", cat.as_bytes())]))
        .create();
    let dir = tempfile::tempdir().unwrap();
    let cat = Catalogue::fetch(
        &client(),
        &server.url(),
        ElevationModel::Globe,
        GRect::new(50.0, 10.0, 51.0, 12.0),
        dir.path(),
    )
    .unwrap();
    assert_eq!(cat.files.keys().collect::<Vec<_>>(), ["JO50AA"]);
}
