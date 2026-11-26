use as_settings::{DopplerStrategy, Settings};
use chrono::NaiveDate;

#[test]
fn defaults_match_csharp() {
    let s = Settings::default();
    let a = &s.analysis;
    assert_eq!(a.history_url, "");
    assert!(!a.history_disclaimer_accepted);
    assert_eq!(a.history_directory, "");
    assert_eq!(a.history_date, NaiveDate::from_ymd_opt(2017, 7, 1).unwrap());
    assert_eq!(a.history_step_min, 5);
    assert_eq!(a.load_file_name, "");
    assert!(!a.crossing_with_signal_level);
    assert_eq!(a.crossing_ambiguous_gap_s, 10);
    let t = &s.track;
    assert!(!t.activate);
    assert_eq!((t.set_az, t.set_el), (0.0, 0.0));
    assert_eq!((t.offset_s, t.update_ms), (0, 1000));
    assert_eq!(s.doppler.dial_freq_hz, 0);
    assert_eq!(s.doppler.strategy, DopplerStrategy::None);
    assert!(s.gui.tracking_gauges_show);
    assert!(!s.gui.stations_map_overlay_elevation);
}

#[test]
fn stage3b_file_loads_unchanged() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let t = v.as_table_mut().unwrap();
    for k in ["analysis", "track", "doppler"] {
        assert!(t.remove(k).is_some(), "{k}");
    }
    // a stage-3b file written by an existing install carries no wizard flag
    t.remove("first_run");
    let g = t.get_mut("gui").unwrap().as_table_mut().unwrap();
    for k in ["tracking_gauges_show", "stations_map_overlay_elevation"] {
        assert!(g.remove(k).is_some(), "{k}");
    }
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    let d = Settings::default();
    assert!(!s.first_run);
    assert_eq!(s.analysis, d.analysis);
    assert_eq!(s.track, d.track);
    assert_eq!(s.doppler, d.doppler);
    assert_eq!(s.gui, d.gui);
}

#[test]
fn history_url_has_no_default() {
    assert!(Settings::default().analysis.history_url.is_empty());
}

#[test]
fn doppler_strategy_roundtrip() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct W {
        s: DopplerStrategy,
    }
    let text = toml::to_string(&W {
        s: DopplerStrategy::C,
    })
    .unwrap();
    assert_eq!(text.trim(), "s = \"c\"");
    let back: W = toml::from_str("s = \"c\"").unwrap();
    assert_eq!(back.s, DopplerStrategy::C);
}

#[test]
fn debug_hides_history_url_credentials() {
    let mut s = Settings::default();
    s.analysis.history_url = "https://u:secret@host.example/hist/?token=x#frag".into();
    for text in [format!("{s:?}"), format!("{:?}", s.analysis)] {
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("u:"), "{text}");
        assert!(!text.contains("token"), "{text}");
        assert!(!text.contains("host.example"), "{text}");
    }
    // a password with an unescaped `/` or `?` stays hidden too
    s.analysis.history_url = "https://u:pa/ss?x@host.example/".into();
    let text = format!("{:?}", s.analysis);
    assert!(!text.contains("pa/ss") && !text.contains("u:"), "{text}");
    // the value itself is unchanged
    assert_eq!(s.analysis.history_url, "https://u:pa/ss?x@host.example/");
}
