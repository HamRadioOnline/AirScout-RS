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
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let _q = server
            .mock("GET", "/qrv.zip")
            .with_body(zip_of("qrv.json", QRVS))
            .create();
        update_stations(&client, &base, dir.path(), &db, None, None, &mut |_| {}).unwrap();
    }
    let _l = server
        .mock("GET", "/locations.zip")
        .with_status(304)
        .create();
    let _q = server.mock("GET", "/qrv.zip").with_status(304).create();
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
            .with_body(zip_of("locations.json", LOCATIONS))
            .create();
        let cancel = std::sync::atomic::AtomicBool::new(true);
        assert!(update_stations(
            &client,
            &base,
            dir.path(),
            &db,
            None,
            Some(&cancel),
            &mut |_| {}
        )
        .is_err());
    }
    assert_eq!(db.location_count().unwrap(), 0);
    let _l = server
        .mock("GET", "/locations.zip")
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
