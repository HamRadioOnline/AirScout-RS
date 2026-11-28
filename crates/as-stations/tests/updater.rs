use as_geo::band::Band;
use as_geo::latlon::GRect;
use as_stations::{update_stations, StationDb};
use std::io::Write;

fn zip_of(name: &str, body: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(body.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

const LOCATIONS: &str = r#"[
 {"Elevation":0,"BestCaseElevation":false,"Call":"DA0AAA","Loc":"JO50WC","Lat":50.1042,"Lon":11.9583,"Source":1,"Hits":0,"LastUpdated":"2013-05-12T19:18:12Z"},
 {"Elevation":0,"BestCaseElevation":false,"Call":"DA0BBB","Loc":"IO83PO","Lat":53.6042,"Lon":-2.7083,"Source":2,"Hits":3,"LastUpdated":"2014-01-01T00:00:00Z"}
]"#;
const QRVS: &str = r#"[
 {"Call":"DA0AAA","Loc":"JO50WC","Band":144,"AntennaHeight":10.0,"AntennaGain":12.0,"Power":100.0,"LastUpdated":"2010-09-04T00:00:00Z"},
 {"Call":"DA0AAA","Loc":"JO50WC","Band":10368,"AntennaHeight":0.0,"AntennaGain":0.0,"Power":0.0,"LastUpdated":"2010-09-04T00:00:00Z"},
 {"Call":"DA0BBB","Loc":"IO83PO","Band":432,"AntennaHeight":0.0,"AntennaGain":0.0,"Power":0.0,"LastUpdated":"2011-09-04T00:00:00Z"}
]"#;

#[test]
fn updater_parses_fixture() {
    let mut server = mockito::Server::new();
    let _l = server
        .mock("GET", "/locations.zip")
        .with_body(zip_of("locations.json", LOCATIONS))
        .create();
    let _q = server
        .mock("GET", "/qrv.zip")
        .with_body(zip_of("qrv.json", QRVS))
        .create();
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let client = reqwest::blocking::Client::new();
    let mut msgs = Vec::new();
    let st = update_stations(
        &client,
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        None,
        None,
        &mut |m| msgs.push(m.to_string()),
    )
    .unwrap();
    assert_eq!((st.locations, st.qrvs), (2, 3));
    assert_eq!(db.location_count().unwrap(), 2);
    assert!(db
        .qrv_find("DA0AAA", "JO50WC", Band::B10G)
        .unwrap()
        .is_some());
    assert!(db.table_timestamp("Location").unwrap().is_some());
    assert!(!msgs.is_empty());
}

#[test]
fn updater_restricts_to_area_of_interest() {
    let mut server = mockito::Server::new();
    let _l = server
        .mock("GET", "/locations.zip")
        .with_body(zip_of("locations.json", LOCATIONS))
        .create();
    let _q = server
        .mock("GET", "/qrv.zip")
        .with_body(zip_of("qrv.json", QRVS))
        .create();
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let client = reqwest::blocking::Client::new();
    let area = GRect::new(45.0, 5.0, 55.0, 15.0);
    let st = update_stations(
        &client,
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        Some(area),
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!((st.locations, st.qrvs), (1, 2));
    assert!(db.location_find("DA0BBB", None).unwrap().is_none());
}

#[test]
fn updater_skips_unchanged_files() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let client = reqwest::blocking::Client::new();
    let base = format!("{}/", server.url());
    {
        let _l = server
            .mock("GET", "/locations.zip")
            .with_header("ETag", "\"locations-v1\"")
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let _q = server
            .mock("GET", "/qrv.zip")
            .with_header("ETag", "\"qrv-v1\"")
            .with_body(zip_of("qrv.json", QRVS))
            .create();
        update_stations(&client, &base, dir.path(), &db, None, None, &mut |_| {}).unwrap();
    }
    let _l = server
        .mock("GET", "/locations.zip")
        .match_header("if-none-match", "\"locations-v1\"")
        .with_status(304)
        .create();
    let _q = server
        .mock("GET", "/qrv.zip")
        .match_header("if-none-match", "\"qrv-v1\"")
        .with_status(304)
        .create();
    let st = update_stations(&client, &base, dir.path(), &db, None, None, &mut |_| {}).unwrap();
    assert_eq!((st.locations, st.qrvs), (0, 0));
    assert_eq!(db.location_count().unwrap(), 2);
}

#[test]
fn updater_http_error_is_err() {
    let mut server = mockito::Server::new();
    let _l = server
        .mock("GET", "/locations.zip")
        .with_status(404)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let client = reqwest::blocking::Client::new();
    assert!(update_stations(
        &client,
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        None,
        None,
        &mut |_| {}
    )
    .is_err());
}

#[test]
#[ignore = "network: downloads the live station database from airscout.eu"]
fn live_update() {
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open(&dir.path().join("stations.db3")).unwrap();
    let client = reqwest::blocking::Client::new();
    let area = GRect::new(47.0, 5.0, 55.0, 15.0);
    let st = update_stations(
        &client,
        as_stations::DEFAULT_UPDATE_URL,
        dir.path(),
        &db,
        Some(area),
        None,
        &mut |m| eprintln!("{m}"),
    )
    .unwrap();
    assert!(st.locations > 1000, "{st:?}");
    assert!(st.qrvs > 1000, "{st:?}");
}

/// Review I3: a run that downloads a new file but is cancelled before the import must not make
/// the next run (304) skip that file.
#[test]
fn cancelled_import_is_retried_on_304() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let client = reqwest::blocking::Client::new();
    let base = format!("{}/", server.url());
    // earlier successful import sets the table timestamps
    db.set_table_timestamp("Location", chrono::Utc::now() - chrono::Duration::days(1))
        .unwrap();
    db.set_table_timestamp("QRV", chrono::Utc::now() - chrono::Duration::days(1))
        .unwrap();
    {
        let _l = server
            .mock("GET", "/locations.zip")
            .with_header("ETag", "\"locations-v1\"")
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        assert!(update_stations(
            &client,
            &base,
            dir.path(),
            &db,
            None,
            Some(&cancel),
            &mut |_| cancel.store(true, std::sync::atomic::Ordering::Relaxed)
        )
        .is_err());
    }
    assert_eq!(db.location_count().unwrap(), 0);
    let _l = server
        .mock("GET", "/locations.zip")
        .match_header("if-none-match", "\"locations-v1\"")
        .with_status(304)
        .create();
    let _q = server
        .mock("GET", "/qrv.zip")
        .with_body(zip_of("qrv.json", QRVS))
        .create();
    let st = update_stations(&client, &base, dir.path(), &db, None, None, &mut |_| {}).unwrap();
    assert_eq!(st.locations, 2);
    assert_eq!(db.location_count().unwrap(), 2);
}

#[test]
fn finding_374_malformed_archive_does_not_echo_station_fields() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/locations.zip")
        .with_body(zip_of("locations.json", r#"["SyntheticCredential"]"#))
        .create();
    let cache = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let error = update_stations(
        &reqwest::blocking::Client::new(),
        &format!("{}/", server.url()),
        cache.path(),
        &db,
        None,
        None,
        &mut |_| {},
    )
    .unwrap_err();
    mock.assert();
    assert!(matches!(
        error,
        as_stations::StationError::ArchiveData { .. }
    ));
    assert!(!format!("{error} {error:?}").contains("SyntheticCredential"));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn finding_377_scope_expansion_reuses_the_matching_cached_archives() {
    let mut server = mockito::Server::new();
    let modified = "Tue, 01 Sep 2020 00:00:00 GMT";
    let mut first = Vec::new();
    for (archive, member, data) in [
        ("locations.zip", "locations.json", LOCATIONS),
        ("qrv.zip", "qrv.json", QRVS),
    ] {
        first.push(
            server
                .mock("GET", format!("/{archive}").as_str())
                .with_header("Last-Modified", modified)
                .with_body(zip_of(member, data))
                .create(),
        );
    }
    let client = reqwest::blocking::Client::new();
    let cache = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let url = format!("{}/", server.url());
    let first_area = Some(GRect::new(49.0, 11.0, 52.0, 13.0));
    let first_pass = update_stations(
        &client,
        &url,
        cache.path(),
        &db,
        first_area,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!((first_pass.locations, first_pass.qrvs), (1, 2));
    for req in first {
        req.assert();
        req.remove();
    }
    for path in ["/locations.zip", "/qrv.zip"] {
        server.mock("GET", path).with_status(304).create();
    }
    let wide = Some(GRect::new(49.0, -5.0, 55.0, 13.0));
    let second =
        update_stations(&client, &url, cache.path(), &db, wide, None, &mut |_| {}).unwrap();
    assert_eq!((second.locations, second.qrvs), (1, 1));
    assert_eq!(db.location_count().unwrap(), 2);
}

#[test]
fn finding_383_rejects_invalid_coordinates_before_filtering_or_writing() {
    for bad_latitude in [r#""NaN""#, r#""Infinity""#, "91"] {
        let bad = LOCATIONS.replace("50.1042", bad_latitude);
        assert!(as_stations::locations_from_json(&bad).is_err());
    }
    let db = StationDb::open_in_memory().unwrap();
    let mut location = as_stations::locations_from_json(LOCATIONS)
        .unwrap()
        .remove(0);
    location.lat = f64::NAN;
    assert!(db.location_insert_or_update_if_newer(&location).is_err());
    assert!(db
        .location_bulk_insert_or_update_if_newer(&[location])
        .is_err());
    assert_eq!(db.location_count().unwrap(), 0);
}

#[test]
fn finding_377_switching_source_requires_full_body_even_for_identical_archives() {
    let mut server = mockito::Server::new();
    let client = reqwest::blocking::Client::new();
    let cache = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    for source in ["a", "b", "a"] {
        let loc = server
            .mock("GET", format!("/{source}/locations.zip").as_str())
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_header("ETag", "\"fixed\"")
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let qrv = server
            .mock("GET", format!("/{source}/qrv.zip").as_str())
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_header("ETag", "\"fixed\"")
            .with_body(zip_of("qrv.json", QRVS))
            .create();
        let base = format!("{}/{source}/", server.url());
        update_stations(&client, &base, cache.path(), &db, None, None, &mut |_| {}).unwrap();
        loc.assert();
        qrv.assert();
    }
}

#[test]
fn finding_377_location_change_and_table_clear_refresh_scoped_qrv_import() {
    let mut server = mockito::Server::new();
    let client = reqwest::blocking::Client::new();
    let cache = tempfile::tempdir().unwrap();
    let db_path = cache.path().join("stations.db3");
    let db = StationDb::open(&db_path).unwrap();
    let base = format!("{}/", server.url());
    let area = Some(GRect::new(49.0, 11.0, 52.0, 13.0));
    {
        let loc = server
            .mock("GET", "/locations.zip")
            .with_header("ETag", "\"loc\"")
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let qrv = server
            .mock("GET", "/qrv.zip")
            .with_header("ETag", "\"qrv\"")
            .with_body(zip_of("qrv.json", QRVS))
            .create();
        let first =
            update_stations(&client, &base, cache.path(), &db, area, None, &mut |_| {}).unwrap();
        assert_eq!(first.qrvs, 2);
        loc.assert();
        qrv.assert();
    }
    drop(db);
    let db = StationDb::open(&db_path).unwrap();
    let mut added = as_stations::locations_from_json(LOCATIONS)
        .unwrap()
        .pop()
        .unwrap();
    added.lat = 50.25;
    added.lon = 11.25;
    db.location_insert_or_update_if_newer(&added).unwrap();
    let loc = server
        .mock("GET", "/locations.zip")
        .match_header("if-none-match", "\"loc\"")
        .with_status(304)
        .expect(3)
        .create();
    let qrv = server
        .mock("GET", "/qrv.zip")
        .match_header("if-none-match", "\"qrv\"")
        .with_status(304)
        .expect(3)
        .create();
    let refresh =
        || update_stations(&client, &base, cache.path(), &db, area, None, &mut |_| {}).unwrap();
    assert_eq!(refresh().qrvs, 1);
    let stamp = db.table_timestamp("QRV").unwrap();
    assert_eq!(refresh().qrvs, 0);
    assert_eq!(db.table_timestamp("QRV").unwrap(), stamp);
    assert_eq!(db.clear_table("QRV").unwrap(), 3);
    assert_eq!(refresh().qrvs, 3);
    loc.assert();
    qrv.assert();
}

#[test]
fn finding_382_incomplete_qrv_archive_preserves_its_prior_committed_timestamp() {
    let mut server = mockito::Server::new();
    let client = reqwest::blocking::Client::new();
    let cache = tempfile::tempdir().unwrap();
    let db = StationDb::open_in_memory().unwrap();
    let old = chrono::DateTime::<chrono::Utc>::from_timestamp(100, 0).unwrap();
    db.set_table_timestamp("Location", old).unwrap();
    db.set_table_timestamp("QRV", old).unwrap();
    let loc = server
        .mock("GET", "/locations.zip")
        .with_body(zip_of("locations.json", LOCATIONS))
        .create();
    let body = zip_of("qrv.json", QRVS);
    let qrv = server
        .mock("GET", "/qrv.zip")
        .with_body(&body[..body.len() - 10])
        .create();
    let base = format!("{}/", server.url());
    assert!(update_stations(&client, &base, cache.path(), &db, None, None, &mut |_| {}).is_err());
    assert_eq!(db.location_count().unwrap(), 2);
    assert!(db
        .table_timestamp("Location")
        .unwrap()
        .is_some_and(|time| time > old));
    assert_eq!(db.table_timestamp("QRV").unwrap(), Some(old));
    assert_eq!(db.qrv_count().unwrap(), 0);
    loc.assert();
    qrv.assert();
}
