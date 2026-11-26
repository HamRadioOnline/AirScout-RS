use as_aircraft::db::AircraftDb;
use as_aircraft::updater::{update_aircraft_db, DEFAULT_UPDATE_URL, FILES};
use std::io::Write;

fn zip_of(name: &str) -> Vec<u8> {
    let body =
        std::fs::read(as_golden::repo_root().join(format!("tests/fixtures/aircraft/{name}.json")))
            .unwrap();
    zip_body(name, &body)
}

fn zip_body(name: &str, body: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file(
            format!("{name}.json"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        z.write_all(body).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::new()
}

#[test]
fn updater_imports_zip_from_mock() {
    let mut server = mockito::Server::new();
    let _m: Vec<_> = FILES
        .iter()
        .map(|n| {
            server
                .mock("GET", format!("/{n}.zip").as_str())
                .with_body(zip_of(n))
                .create()
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let db = AircraftDb::open_in_memory().unwrap();
    let s = update_aircraft_db(
        &client(),
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        (
            s.aircrafts,
            s.aircraft_types,
            s.airlines,
            s.airports,
            s.registrations
        ),
        (16, 26, 10, 8, 12)
    );
    // empty Call/Reg/TypeCode become "[unknown]"
    let a = db.aircraft_find_by_hex("AAAAAA").unwrap().unwrap();
    assert_eq!(
        (a.call.as_str(), a.reg.as_str(), a.type_code.as_str()),
        ("[unknown]", "[unknown]", "[unknown]")
    );
}

#[test]
fn updater_skips_when_not_newer() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let db = AircraftDb::open_in_memory().unwrap();
    {
        let _m: Vec<_> = FILES
            .iter()
            .map(|n| {
                server
                    .mock("GET", format!("/{n}.zip").as_str())
                    .with_body(zip_of(n))
                    .create()
            })
            .collect();
        update_aircraft_db(
            &client(),
            &format!("{}/", server.url()),
            dir.path(),
            &db,
            None,
            &mut |_| {},
        )
        .unwrap();
    }
    server.reset();
    let _m: Vec<_> = FILES
        .iter()
        .map(|n| {
            server
                .mock("GET", format!("/{n}.zip").as_str())
                .with_status(304)
                .create()
        })
        .collect();
    let s = update_aircraft_db(
        &client(),
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(s, Default::default());
}

#[test]
fn updater_http_error_is_err() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(500)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let db = AircraftDb::open_in_memory().unwrap();
    assert!(update_aircraft_db(
        &client(),
        &format!("{}/", server.url()),
        dir.path(),
        &db,
        None,
        &mut |_| {}
    )
    .is_err());
}

#[test]
#[ignore = "hits the real airscout.eu server"]
fn live_update() {
    let dir = tempfile::tempdir().unwrap();
    let db = AircraftDb::open_in_memory().unwrap();
    let s = update_aircraft_db(
        &client(),
        DEFAULT_UPDATE_URL,
        dir.path(),
        &db,
        None,
        &mut |m| eprintln!("{m}"),
    )
    .unwrap();
    assert!(s.aircrafts > 100_000, "{s:?}");
    assert!(s.aircraft_types > 1000, "{s:?}");
}

#[test]
fn finding_374_malformed_archive_does_not_echo_aircraft_fields() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/AircraftTypes.zip")
        .with_body(zip_body("AircraftTypes", br#"["SyntheticCredential"]"#))
        .create();
    let cache = tempfile::tempdir().unwrap();
    let db = AircraftDb::open_in_memory().unwrap();
    let error = update_aircraft_db(
        &client(),
        &format!("{}/", server.url()),
        cache.path(),
        &db,
        None,
        &mut |_| {},
    )
    .unwrap_err();
    mock.assert();
    assert!(matches!(
        error,
        as_aircraft::AircraftError::ArchiveData { .. }
    ));
    assert!(!format!("{error} {error:?}").contains("SyntheticCredential"));
    assert!(std::error::Error::source(&error).is_none());
}
