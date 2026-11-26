use as_settings::{Settings, Track, TrackDde, TrackSerial};

#[test]
fn track_serial_dde_defaults_match_csharp() {
    let t = Settings::default().track;
    assert_eq!(t, Track::default());
    assert_eq!(t.serial, TrackSerial::None);
    assert_eq!(t.serial_port, "COM13");
    assert_eq!(t.serial_baudrate, 4800);
    assert_eq!(t.dde, TrackDde::None);
}

#[test]
fn track_4b_file_loads_unchanged() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let tr = v.get_mut("track").unwrap().as_table_mut().unwrap();
    for k in ["serial", "serial_port", "serial_baudrate", "dde"] {
        assert!(tr.remove(k).is_some(), "{k}");
    }
    tr.insert("offset_s".into(), toml::Value::Integer(5));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string_pretty(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.track.serial, TrackSerial::None);
    assert_eq!(s.track.serial_port, "COM13");
    assert_eq!(s.track.serial_baudrate, 4800);
    assert_eq!(s.track.dde, TrackDde::None);
    assert_eq!(s.track.offset_s, 5);
}

fn track_value(key: &str, val: &str) -> Track {
    let mut tbl = toml::Table::new();
    tbl.insert(key.into(), toml::Value::String(val.into()));
    toml::Value::Table(tbl).try_into().unwrap()
}

fn track_key(t: &Track, key: &str) -> String {
    let v = toml::Value::try_from(t).unwrap();
    v.get(key).unwrap().as_str().unwrap().to_owned()
}

#[test]
fn serial_enums_roundtrip() {
    for (e, name) in [
        (TrackSerial::None, "none"),
        (TrackSerial::Gs232Az, "gs232az"),
        (TrackSerial::Gs232AzEl, "gs232azel"),
    ] {
        let t = track_value("serial", name);
        assert_eq!(t.serial, e);
        assert_eq!(track_key(&t, "serial"), name);
    }
    for (e, name) in [(TrackDde::None, "none"), (TrackDde::Hrd, "hrd")] {
        let t = track_value("dde", name);
        assert_eq!(t.dde, e);
        assert_eq!(track_key(&t, "dde"), name);
    }
}

#[test]
fn engine_lenient_case_and_type() {
    use as_settings::CatEngine;
    let load = |engine: &str| -> Settings {
        toml::from_str(&format!("[cat]\nengine = {engine}\nrigctld_port = 4533\n")).unwrap()
    };
    for (raw, e) in [
        ("\"OmniRig\"", CatEngine::OmniRig),
        ("\"omnirig\"", CatEngine::OmniRig),
        ("\"RIGCTLD\"", CatEngine::Rigctld),
        ("\"Hamlib\"", CatEngine::Hamlib),
        ("\"flrig\"", CatEngine::None),
        ("42", CatEngine::None),
        ("[\"x\"]", CatEngine::None),
        ("{ a = 1 }", CatEngine::None),
    ] {
        let s = load(raw);
        assert_eq!(s.cat.engine, e, "{raw}");
        assert_eq!(s.cat.rigctld_port, 4533, "{raw}");
    }
}

#[test]
fn omnirig_engine_serializes_lowercase() {
    use as_settings::{Cat, CatEngine};
    let c = Cat {
        engine: CatEngine::OmniRig,
        omnirig_rig: 3,
        ..Cat::default()
    };
    let text = toml::to_string(&c).unwrap();
    assert!(text.contains("engine = \"omnirig\""), "{text}");
    assert!(text.contains("omnirig_rig = 3"), "{text}");
    let back: Cat = toml::from_str(&text).unwrap();
    assert_eq!(back, c);
}

#[test]
fn omnirig_rig_default_is_1() {
    assert_eq!(Settings::default().cat.omnirig_rig, 1);
    let s: Settings = toml::from_str("[cat]\nengine = \"omnirig\"\n").unwrap();
    assert_eq!(s.cat.omnirig_rig, 1);
}
