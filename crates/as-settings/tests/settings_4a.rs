use as_settings::{Server, Settings, TrackFile, TrackUdp};

#[test]
fn defaults_match_csharp() {
    let s = Settings::default();
    assert!(!s.server.activate);
    assert_eq!(s.server.port, 9872);
    assert_eq!(s.server.name, "AS");
    assert_eq!(Server::default().name, "AS");
    assert!(!s.watchlist_sync_with_kst);
    let t = &s.track;
    assert_eq!(t.udp, TrackUdp::None);
    assert_eq!(t.udp_wintest_port, 9871);
    assert_eq!(t.udp_airscout_port, 9872);
    assert_eq!(t.file, TrackFile::None);
}

#[test]
fn stage3c_file_loads_unchanged() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let t = v.as_table_mut().unwrap();
    for k in ["server", "watchlist_sync_with_kst"] {
        assert!(t.remove(k).is_some(), "{k}");
    }
    t.remove("first_run");
    let tr = t.get_mut("track").unwrap().as_table_mut().unwrap();
    for k in ["udp", "udp_wintest_port", "udp_airscout_port", "file"] {
        assert!(tr.remove(k).is_some(), "{k}");
    }
    tr.insert("offset_s".into(), toml::Value::Integer(5));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string_pretty(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.server, Server::default());
    assert!(!s.watchlist_sync_with_kst);
    assert_eq!(s.track.udp, TrackUdp::None);
    assert_eq!(s.track.udp_wintest_port, 9871);
    assert_eq!(s.track.udp_airscout_port, 9872);
    assert_eq!(s.track.file, TrackFile::None);
    assert_eq!(s.track.offset_s, 5);
    assert!(!s.first_run);
}

fn track_value(key: &str, val: &str) -> as_settings::Track {
    let mut tbl = toml::Table::new();
    tbl.insert(key.into(), toml::Value::String(val.into()));
    toml::Value::Table(tbl).try_into().unwrap()
}

fn track_key(t: &as_settings::Track, key: &str) -> String {
    let v = toml::Value::try_from(t).unwrap();
    v.get(key).unwrap().as_str().unwrap().to_owned()
}

#[test]
fn track_udp_roundtrip() {
    let t = track_value("udp", "wintest");
    assert_eq!(t.udp, TrackUdp::WinTest);
    assert_eq!(track_key(&t, "udp"), "wintest");
    let t = track_value("udp", "airscout");
    assert_eq!(t.udp, TrackUdp::AirScout);
    assert_eq!(track_key(&t, "udp"), "airscout");
}

#[test]
fn track_file_roundtrip() {
    let t = track_value("file", "native");
    assert_eq!(t.file, TrackFile::Native);
    assert_eq!(track_key(&t, "file"), "native");
    let t = track_value("file", "wsjt");
    assert_eq!(t.file, TrackFile::Wsjt);
    assert_eq!(track_key(&t, "file"), "wsjt");
}

#[test]
fn server_name_kept_verbatim() {
    let mut s = Settings::default();
    s.server.name = " as ".into();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    s.save(&p).unwrap();
    let l = Settings::load_or_default(&p).unwrap();
    assert_eq!(l.server.name, " as ");
}
