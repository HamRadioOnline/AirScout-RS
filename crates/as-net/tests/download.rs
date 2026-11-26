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
    assert!(download_if_newer(&client, &url, &path, 3).unwrap());
    m1.assert();
    m1.remove();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");

    let m2 = server
        .mock("GET", "/f.zip")
        .match_header("if-modified-since", mockito::Matcher::Regex(r"GMT$".into()))
        .with_status(304)
        .create();
    assert!(!download_if_newer(&client, &url, &path, 3).unwrap());
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
    let r = download_if_newer(&client, &format!("{}/f.zip", server.url()), &path, 3);
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
        3,
    );
    assert!(matches!(r, Err(as_net::NetError::NotFound(_))), "{r:?}");
}

#[test]
fn finding_305_rejects_oversize_download_and_preserves_existing_bytes() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("archive.zip");
    std::fs::write(&path, b"archive").unwrap();
    let client = reqwest::blocking::Client::new();
    let request = server.mock("GET", "/archive").with_body("1234").create();
    let url = format!("{}/archive", server.url());

    assert!(download_if_newer(&client, &url, &path, 3).is_err());
    request.assert();
    assert_eq!(std::fs::read(&path).unwrap(), b"archive");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn finding_305_checks_streamed_bytes_without_content_length() {
    let mut server = mockito::Server::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("archive.zip");
    let client = reqwest::blocking::Client::new();
    let exact = server
        .mock("GET", "/exact")
        .with_chunked_body(|body| body.write_all(b"abc"))
        .create();
    let excess = server
        .mock("GET", "/excess")
        .with_chunked_body(|body| body.write_all(b"abcd"))
        .create();

    assert!(download_if_newer(&client, &format!("{}/exact", server.url()), &path, 3).unwrap());
    assert!(download_if_newer(&client, &format!("{}/excess", server.url()), &path, 3).is_err());
    exact.assert();
    excess.assert();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
