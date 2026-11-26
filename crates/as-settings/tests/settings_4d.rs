use as_settings::{Settings, Web};

#[test]
fn web_defaults() {
    let w = Settings::default().web;
    assert_eq!(w, Web::default());
    // `Webserver_Port` (`Settings.settings:260-261`)
    assert_eq!(w.port, 9880);
    // port and path of `KST_Websocket` (`Settings.settings:772-773`), host `localhost`
    assert_eq!(w.kst_websocket, "ws://localhost:9881/AirScout");
    // `KST_WebSocket_Timeout` (`Settings.settings:775-776`)
    assert_eq!(w.kst_websocket_timeout_s, 150.0);
    // `Map_SmallMarkers` (`Settings.settings:493-494`)
    assert!(w.map_small_markers);
}

#[test]
fn stage4c_file_loads_unchanged() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let t = v.as_table_mut().unwrap();
    assert!(t.remove("web").is_some());
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string_pretty(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s, Settings::default());
}

#[test]
fn partial_web_table_keeps_other_defaults() {
    let s: Settings = toml::from_str("[web]\nport = 9999\n").unwrap();
    assert_eq!(s.web.port, 9999);
    assert_eq!(
        Web {
            port: 9880,
            ..s.web.clone()
        },
        Web::default()
    );
}

#[test]
fn web_roundtrips() {
    let s = Settings {
        web: Web {
            port: 1234,
            kst_websocket: "ws://localhost:1/x".into(),
            kst_websocket_timeout_s: 12.5,
            map_small_markers: false,
        },
        ..Settings::default()
    };
    let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
    assert_eq!(back.web, s.web);
}
