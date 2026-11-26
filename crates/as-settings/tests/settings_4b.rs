use as_settings::{Cat, CatEngine, FlowControl, Parity, Settings, StopBits};

#[test]
fn cat_defaults_match_csharp() {
    let c = Settings::default().cat;
    assert_eq!(c, Cat::default());
    assert!(!c.activate);
    assert_eq!(c.update_ms, 500);
    assert_eq!(c.engine, CatEngine::None);
    assert_eq!(c.rigctld_host, "localhost");
    assert_eq!(c.rigctld_port, 4532);
    assert_eq!(c.hamlib_model, 1);
    assert_eq!(c.port_name, "");
    assert_eq!(c.baudrate, 9600);
    assert_eq!(c.data_bits, 8);
    assert_eq!(c.parity, Parity::None);
    assert_eq!(c.stop_bits, StopBits::One);
    assert_eq!(c.rts, FlowControl::Low);
    assert_eq!(c.dtr, FlowControl::Low);
    assert_eq!(c.timeout_ms, 500);
}

#[test]
fn stage4a_file_loads_unchanged() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let t = v.as_table_mut().unwrap();
    assert!(t.remove("cat").is_some());
    let tr = t.get_mut("track").unwrap().as_table_mut().unwrap();
    tr.insert("offset_s".into(), toml::Value::Integer(5));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string_pretty(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.cat, Cat::default());
    assert_eq!(s.track.offset_s, 5);
}

#[test]
fn cat_engine_roundtrip() {
    for (e, name) in [
        (CatEngine::None, "none"),
        (CatEngine::Rigctld, "rigctld"),
        (CatEngine::Hamlib, "hamlib"),
    ] {
        let c = Cat {
            engine: e,
            ..Cat::default()
        };
        let text = toml::to_string(&c).unwrap();
        assert!(text.contains(&format!("engine = \"{name}\"")), "{text}");
        let back: Cat = toml::from_str(&text).unwrap();
        assert_eq!(back.engine, e);
    }
}

#[test]
fn unknown_engine_loads_as_none() {
    let c: Cat = toml::from_str("engine = \"flrig\"\nrigctld_port = 4533\n").unwrap();
    assert_eq!(c.engine, CatEngine::None);
    assert_eq!(c.rigctld_port, 4533);
    let s: Settings = toml::from_str("[cat]\nengine = \"flrig\"\nrigctld_port = 4533\n").unwrap();
    assert_eq!(s.cat.engine, CatEngine::None);
    assert_eq!(s.cat.rigctld_port, 4533);
}

#[test]
fn serial_enums_lowercase() {
    let c = Cat {
        parity: Parity::Space,
        stop_bits: StopBits::One5,
        rts: FlowControl::Handshake,
        dtr: FlowControl::High,
        ..Cat::default()
    };
    let text = toml::to_string(&c).unwrap();
    for l in [
        "parity = \"space\"",
        "stop_bits = \"one5\"",
        "rts = \"handshake\"",
        "dtr = \"high\"",
    ] {
        assert!(text.contains(l), "{l} in {text}");
    }
    assert_eq!(toml::from_str::<Cat>(&text).unwrap(), c);
}
