use as_elevation::ElevationModel;
use as_settings::{ElevationEnabled, Secret, Settings, Watchlist, WatchlistItem};
use chrono::{TimeZone, Utc};

fn item(call: &str, loc: &str) -> WatchlistItem {
    WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        ..Default::default()
    }
}

#[test]
fn defaults_match_csharp() {
    let s = Settings::default();
    let e = s.elevation_enabled;
    assert!(e.globe && !e.srtm3 && !e.srtm1 && !e.aster3 && !e.aster1);
    assert!(s.watchlist.0.is_empty());
    assert_eq!(s.watchlist_max_count, 1000);
    assert!(s.location_restrict_to_area);
    assert!(s.feeds_disclaimer_accepted.is_empty());
    let b = &s.background;
    assert!(b.update_on_startup && !b.update_periodically && b.calculations_enable);
    assert_eq!(b.update_period_min, 60);
    assert_eq!(b.stations_url, as_stations::DEFAULT_UPDATE_URL);
    assert_eq!(b.aircraft_url, as_aircraft::updater::DEFAULT_UPDATE_URL);
    let l = &s.speclab;
    assert!(!l.enabled);
    assert_eq!(l.url, "http://localhost:10080/");
    assert_eq!(l.file_name, "_fft_expt.json");
    assert_eq!((l.f1, l.f2, l.update_s), (400, 1600, 1));
    assert!(s.time.online);
    assert_eq!(
        s.time.offline,
        Utc.with_ymd_and_hms(2015, 2, 25, 0, 0, 0).unwrap()
    );
    assert!(s.gui.thunderforest_api_key.is_empty());
    assert!(s.gui.horizon_plot_polar);
}

#[test]
fn effective_model_order() {
    let mut e = ElevationEnabled {
        globe: false,
        ..Default::default()
    };
    assert_eq!(e.effective_model(), ElevationModel::None);
    e.globe = true;
    e.srtm3 = true;
    assert_eq!(e.effective_model(), ElevationModel::Srtm3);
    assert!(e.is_enabled(ElevationModel::Globe));
    assert!(!e.is_enabled(ElevationModel::Aster1));
    e.srtm1 = true;
    assert_eq!(e.effective_model(), ElevationModel::Srtm1);
    e.aster3 = true;
    assert_eq!(e.effective_model(), ElevationModel::Aster3);
    e.aster1 = true;
    assert_eq!(e.effective_model(), ElevationModel::Aster1);
}

#[test]
fn stage3a_file_does_not_trigger_wizard() {
    let mut v = toml::Value::try_from(Settings::default()).unwrap();
    let t = v.as_table_mut().unwrap();
    for k in [
        "elevation_enabled",
        "watchlist",
        "watchlist_max_count",
        "location_restrict_to_area",
        "background",
        "speclab",
        "time",
        "feeds_disclaimer_accepted",
        "first_run",
    ] {
        t.remove(k);
    }
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, toml::to_string(&v).unwrap()).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert!(!s.first_run);
    assert!(s.watchlist.0.is_empty());
    assert_eq!(s.watchlist_max_count, 1000);
}

#[test]
fn fresh_default_is_first_run() {
    assert!(Settings::default().first_run);
}

#[test]
fn secret_debug_is_redacted() {
    let mut s = Settings::default();
    s.gui.thunderforest_api_key = Secret::from("abc123");
    let d = format!("{:?}", s);
    assert!(!d.contains("abc123"));
    assert!(d.contains("Secret(***)"));
    assert_eq!(format!("{:?}", Secret::default()), "Secret(\"\")");
}

#[test]
fn secret_roundtrips_as_plain_string() {
    let mut s = Settings::default();
    s.gui.thunderforest_api_key = Secret::from("abc123");
    let text = toml::to_string(&s).unwrap();
    assert!(text.contains("thunderforest_api_key = \"abc123\""));
    let back: Settings = toml::from_str(&text).unwrap();
    assert_eq!(back.gui.thunderforest_api_key.expose(), "abc123");
}

#[test]
fn watchlist_insert_capped_and_sort() {
    let mut w = Watchlist::default();
    assert!(w.insert_capped(item("DL2ALF", "JO50"), 3));
    assert!(w.insert_capped(item("DL2ALF", "JN58"), 3));
    assert!(!w.insert_capped(item("DL2ALF", "JN58"), 3), "duplicate");
    assert!(w.insert_capped(item("AA1A", "FN42"), 3));
    assert!(!w.insert_capped(item("ZZ9Z", "AA00"), 3), "full");
    assert_eq!(w.0.len(), 3);
    w.sort();
    let order: Vec<_> = w.0.iter().map(|i| (&*i.call, &*i.loc)).collect();
    assert_eq!(
        order,
        [("AA1A", "FN42"), ("DL2ALF", "JN58"), ("DL2ALF", "JO50")]
    );
    assert_eq!(w.index_of("DL2ALF"), Some(1));
    assert_eq!(w.index_of_loc("DL2ALF", "JO50"), Some(2));
    assert_eq!(w.index_of_loc("DL2ALF", "XX00"), None);
}

#[test]
fn watchlist_serializes_as_array_of_tables() {
    let mut s = Settings::default();
    s.watchlist.0.push(item("AA1A", "FN42"));
    let text = toml::to_string(&s).unwrap();
    assert!(text.contains("[[watchlist]]"));
    let back: Settings = toml::from_str(&text).unwrap();
    assert_eq!(back.watchlist, s.watchlist);
}

#[test]
fn my_calls_mru_max_10() {
    let mut s = Settings::default();
    for i in 0..12 {
        s.my_calls_push(&format!("C{i}"));
    }
    assert_eq!(s.gui.my_calls.len(), 10);
    assert_eq!(s.gui.my_calls[0], "C11");
    s.my_calls_push("C5");
    assert_eq!(s.gui.my_calls[0], "C5");
    assert_eq!(s.gui.my_calls.iter().filter(|c| *c == "C5").count(), 1);
    assert_eq!(s.gui.my_calls.len(), 10);
}

#[test]
fn debug_does_not_leak_feed_credentials() {
    let mut s = Settings::default();
    let mut t = toml::Table::new();
    t.insert("oauth_secret".into(), "s3cr3t".into());
    s.feed_settings.insert("OpenSky".into(), t.into());
    let dbg = format!("{s:?}");
    assert!(!dbg.contains("s3cr3t"), "{dbg}");
    // the file format is unchanged
    let toml = toml::to_string(&s).unwrap();
    assert!(toml.contains("oauth_secret = \"s3cr3t\""), "{toml}");
    let back: Settings = toml::from_str(&toml).unwrap();
    assert_eq!(
        back.feed_settings["OpenSky"]["oauth_secret"].as_str(),
        Some("s3cr3t")
    );
}

/// A stage-3a file (no stage-3b tables) with the given `elevation_model`.
fn stage3a_file(dir: &std::path::Path, model: ElevationModel) -> std::path::PathBuf {
    let s = Settings {
        elevation_model: model,
        ..Settings::default()
    };
    let mut v = toml::Value::try_from(s).unwrap();
    let t = v.as_table_mut().unwrap();
    for k in ["elevation_enabled", "background", "first_run"] {
        t.remove(k);
    }
    let p = dir.join("settings.toml");
    std::fs::write(&p, toml::to_string(&v).unwrap()).unwrap();
    p
}

#[test]
fn stage3a_elevation_model_is_migrated() {
    let dir = tempfile::tempdir().unwrap();
    // stage 3a computed with exactly this model: 3b enables it alone
    let s = Settings::load_or_default(&stage3a_file(dir.path(), ElevationModel::Srtm3)).unwrap();
    let only_srtm3 = ElevationEnabled {
        globe: false,
        srtm3: true,
        srtm1: false,
        aster3: false,
        aster1: false,
    };
    assert_eq!(s.elevation_enabled, only_srtm3);
    assert_eq!(s.elevation_enabled.effective_model(), ElevationModel::Srtm3);
    // stage 3a turned `None` into GLOBE: the default
    let s = Settings::load_or_default(&stage3a_file(dir.path(), ElevationModel::None)).unwrap();
    assert_eq!(s.elevation_enabled, ElevationEnabled::default());
    // a stage-3b file keeps its own choice
    let b = Settings {
        elevation_model: ElevationModel::Srtm3,
        ..Settings::default()
    };
    let p = dir.path().join("3b.toml");
    b.save(&p).unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.elevation_enabled, ElevationEnabled::default());
}
