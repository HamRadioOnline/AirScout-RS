#![allow(clippy::field_reassign_with_default)]
use as_elevation::ElevationModel;
use as_geo::band::Band;
use as_settings::{BandSettings, Settings};

/// Values from `AirScout.Core/BandSettings.cs` `GenerateDefault` and the `Path_Default_*`
/// settings of `AirScout.Core/Properties/Settings.settings`.
#[test]
fn generated_defaults_use_approved_k_and_legacy_band_fields() {
    let s = Settings::default();
    let k = |b: Band| s.band_settings[&b].k_factor;
    let f1 = |b: Band| s.band_settings[&b].f1_clearance;
    assert_eq!((k(Band::B50M), f1(Band::B50M)), (1.33, 0.1));
    assert_eq!((k(Band::B70M), f1(Band::B70M)), (1.33, 0.1));
    assert_eq!((k(Band::B144M), f1(Band::B144M)), (1.33, 0.2));
    assert_eq!((k(Band::B432M), f1(Band::B432M)), (1.33, 0.4));
    for b in [
        Band::B1_2G,
        Band::B2_3G,
        Band::B3_4G,
        Band::B5_7G,
        Band::B10G,
        Band::B24G,
        Band::B47G,
        Band::B76G,
    ] {
        assert_eq!((k(b), f1(b)), (1.33, 0.6), "{b:?}");
    }
    assert_eq!(s.band_settings.len(), 12);
    let b = &s.band_settings[&Band::B10G];
    assert_eq!(
        (
            b.ground_clearance,
            b.max_distance,
            b.max_squint,
            b.max_elevation
        ),
        (0.0, 10.0, 0.0, 0.0)
    );
    // AirScout/Properties/Settings.settings
    assert_eq!(s.band, Band::B1_2G);
    assert_eq!(s.elevation_model, ElevationModel::None);
    assert_eq!(s.path_near_field_suppression_m, 100.0);
    assert_eq!((s.planes_min_alt_m, s.planes_max_alt_m), (5000, 12200));
    assert_eq!(s.path_max_length_km, 1000.0);
    assert_eq!((s.my_height_m, s.dx_height_m), (14.0, 30.0));
}

#[test]
fn band_setting_fallback_like_csharp_class_default() {
    // `BandSetting()` (used when a band row is missing) differs from the generated table rows
    let d = BandSettings::default();
    assert_eq!(
        (d.k_factor, d.f1_clearance, d.max_distance),
        (1.33, 0.6, 10.0)
    );
    assert_eq!((d.max_squint, d.max_elevation), (f64::MAX, f64::MAX));
    let mut s = Settings::default();
    s.band_settings.remove(&Band::B24G);
    s.band = Band::B24G;
    assert_eq!(s.current_band_settings(), d);
}

#[test]
fn radius_uses_k_factor_of_current_band() {
    let mut s = Settings::default();
    s.band = Band::B144M;
    s.band_settings.get_mut(&Band::B144M).unwrap().k_factor = 1.5;
    assert_eq!(s.radius_km(), 6371.0 * 1.5);
    s.band = Band::B10G;
    assert_eq!(s.radius_km(), 6371.0 * 1.33);
}

#[test]
fn roundtrip_toml() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("sub/settings.toml");
    let mut s = Settings::default();
    s.my_call = "DA0TST".into();
    s.elevation_model = ElevationModel::Srtm3;
    s.band_settings.get_mut(&Band::B432M).unwrap().k_factor = 1.25;
    s.save(&p).unwrap();
    assert!(!dir.path().join("sub/settings.toml.tmp").exists());
    let l = Settings::load_or_default(&p).unwrap();
    assert_eq!(l, s);
}

#[test]
fn unknown_fields_ignored_and_missing_fields_default() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(
        &p,
        "my_call = \"DA0TST\"\nfuture_option = 3\n[some_new_table]\nx = 1\n",
    )
    .unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.my_call, "DA0TST");
    assert_eq!(s.band, Band::B1_2G);
    assert_eq!(s.band_settings.len(), 12);
}

#[test]
fn missing_file_gives_default() {
    let dir = tempfile::tempdir().unwrap();
    let s = Settings::load_or_default(&dir.path().join("nope.toml")).unwrap();
    assert_eq!(s, Settings::default());
}

#[test]
fn corrupt_file_is_err_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, "my_call = [unclosed").unwrap();
    assert!(Settings::load_or_default(&p).is_err());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "my_call = [unclosed");
}

#[test]
fn default_path_ends_in_settings_toml() {
    let p = Settings::default_path();
    assert_eq!(p.file_name().unwrap(), "settings.toml");
}

#[test]
fn settings_old_file_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    // a stage-1 file: no feed keys
    std::fs::write(
        &p,
        "my_call = \"DL0XX\"\nmy_lat = 50.0\nplanes_min_alt_m = 4000\n",
    )
    .unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.my_call, "DL0XX");
    assert_eq!(s.feed, "OpenSky");
    assert_eq!(s.planes_interval_s, 60);
    assert_eq!(s.planes_position_ttl_min, 5);
    assert!(!s.planes_keep_history);
    assert!(s.planes_extended_plausibility);
    assert_eq!(s.planes_max_error_dist, 30.0);
    assert_eq!(s.planes_max_delta_m, 1000.0);
    assert_eq!(
        s.feed_settings["AirScoutServer"]["url"].as_str(),
        Some("http://localhost:9880/planes.json")
    );
}

#[test]
fn feed_settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    let mut s = Settings::default();
    s.feed = "ADSBOne".into();
    let mut t = toml::Table::new();
    t.insert(
        "url".into(),
        toml::Value::String("https://example.invalid/%MYLAT%".into()),
    );
    t.insert("timeout_s".into(), toml::Value::Integer(30));
    s.feed_settings
        .insert("ADSBOne".into(), toml::Value::Table(t));
    s.save(&p).unwrap();
    let back = Settings::load_or_default(&p).unwrap();
    assert_eq!(back, s);
    assert_eq!(
        back.feed_settings["ADSBOne"]["url"].as_str(),
        Some("https://example.invalid/%MYLAT%")
    );
}

#[test]
fn finding_371_fresh_profiles_use_standard_gradient_on_every_band() {
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings::load_or_default(&dir.path().join("settings.toml")).unwrap();
    assert!(settings
        .band_settings
        .values()
        .all(|band| band.k_factor == 1.33));
    let radius_m = settings.band_settings[&Band::B144M].k_factor * 6_371_000.0;
    let rise_m = 100_000_f64.powi(2) / (8.0 * radius_m);
    assert!((rise_m - 147.52).abs() < 0.01);
}

#[test]
fn finding_373_legacy_band_omissions_and_explicit_values_survive_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "[band_settings.B144M]\nground_clearance = 12.0\n").unwrap();
    let partial = Settings::load_or_default(&path).unwrap();
    let row = partial.band_settings[&Band::B144M];
    assert_eq!(
        (row.ground_clearance, row.k_factor, row.f1_clearance),
        (12.0, 1.33, 0.6)
    );
    assert!(!partial.band_settings.contains_key(&Band::B432M));
    partial.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text
        .parse::<toml::Table>()
        .unwrap()
        .contains_key("settings_version"));
    assert_eq!(Settings::load_or_default(&path).unwrap(), partial);

    std::fs::write(&path, "").unwrap();
    let implicit = Settings::load_or_default(&path).unwrap();
    assert_eq!(implicit.band_settings[&Band::B144M].k_factor, 1.5);
    for _ in 0..2 {
        implicit.save(&path).unwrap();
        assert_eq!(Settings::load_or_default(&path).unwrap(), implicit);
    }
}

#[test]
fn finding_329_desktop_fresh_and_legacy_urls_are_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let url = |s: &Settings| {
        s.feed_settings
            .get("AirScoutServer")
            .and_then(|t| t.get("url"))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };
    assert_eq!(
        url(&Settings::load_or_default(&path).unwrap()).as_deref(),
        Some("http://localhost:9872/planes.json")
    );
    for (text, endpoint) in [
        (
            "feed = \"AirScoutServer\"",
            "http://localhost:9880/planes.json",
        ),
        (
            "[feed_settings.AirScoutServer]\ntimeout_s = 10",
            "http://localhost:9880/planes.json",
        ),
        ("[feed_settings.AirScoutServer]\nurl = \"\"", ""),
        (
            "[feed_settings.AirScoutServer]\nurl = \"http://localhost:1234/planes.json\"",
            "http://localhost:1234/planes.json",
        ),
    ] {
        std::fs::write(&path, text).unwrap();
        let loaded = Settings::load_or_default(&path).unwrap();
        assert_eq!(url(&loaded).as_deref(), Some(endpoint));
        loaded.save(&path).unwrap();
        assert_eq!(Settings::load_or_default(&path).unwrap(), loaded);
    }
}

#[test]
fn finding_340_desktop_ordinary_save_canonicalizes_only_the_legacy_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    for (entered, expected) in [
        ("https://API.OPENSKY-NETWORK.ORG:443/api/%7e/states?x=%2f&lat=%MINLAT%",
         "https://opensky-network.org:443/api/%7e/states?x=%2f&lat=%MINLAT%"),
        ("https://api.opensky-network.org@localhost/api/?next=https://api.opensky-network.org/api/#https://api.opensky-network.org/api/",
         "https://api.opensky-network.org@localhost/api/?next=https://api.opensky-network.org/api/#https://api.opensky-network.org/api/"),
    ] {
        let mut settings = Settings::default();
        settings.feed_settings.insert("OpenSky".into(), toml::Value::Table(toml::Table::from_iter([("url".into(), entered.into())])));
        settings.save(&path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap().parse::<toml::Table>().unwrap();
        assert_eq!(saved["feed_settings"]["OpenSky"]["url"].as_str(), Some(expected));
        let loaded = Settings::load_or_default(&path).unwrap();
        assert_eq!(loaded.feed_settings["OpenSky"]["url"].as_str(), Some(expected));
    }
}
