use as_net::download_if_newer;

#[test]
fn downloads_then_honours_304() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.zip");
    let client = reqwest::blocking::Client::new();
    let url = format!("{}/f.zip", server.url());

    let m1 = server
        .mock("GET", "/f.zip")
        .match_header(
            "user-agent",
            mockito::Matcher::Regex("^AirScout-RS/".into()),
        )
        .with_body("abc")
        .create();
    assert!(download_if_newer(&client, &url, &path).unwrap());
    m1.assert();
    m1.remove();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");

    let m2 = server
        .mock("GET", "/f.zip")
        .match_header("if-modified-since", mockito::Matcher::Regex(r"GMT$".into()))
        .with_status(304)
        .create();
    assert!(!download_if_newer(&client, &url, &path).unwrap());
    m2.assert();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");
}

#[test]
fn http_error_keeps_old_file() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.zip");
    std::fs::write(&path, b"old").unwrap();
    let client = reqwest::blocking::Client::new();
    let _m = server.mock("GET", "/f.zip").with_status(500).create();
    let r = download_if_newer(&client, &format!("{}/f.zip", server.url()), &path);
    assert!(r.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
}

#[test]
fn http_404_is_not_found() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let client = reqwest::blocking::Client::new();
    let _m = server.mock("GET", "/f.zip").with_status(404).create();
    let r = download_if_newer(
        &client,
        &format!("{}/f.zip", server.url()),
        &dir.path().join("f.zip"),
    );
    assert!(matches!(r, Err(as_net::NetError::NotFound(_))), "{r:?}");
}
