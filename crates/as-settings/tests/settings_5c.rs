//! Stage 5c: the map preloader settings (off by default, the airscout.eu mirror).

use as_settings::Settings;

fn load(text: &str) -> Settings {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, text).unwrap();
    Settings::load_or_default(&p).unwrap()
}

#[test]
fn map_preloader_defaults() {
    let s = Settings::default();
    assert!(!s.gui.map_preloader_enabled);
    assert_eq!(s.gui.map_preloader_max_zoom, 11);
    assert_eq!(s.background.map_mirror_url, as_maps::MIRROR_URL);
    assert_eq!(
        s.background.map_mirror_url,
        "http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/"
    );
}

/// A settings file written before stage 5c (no preloader fields) loads with the defaults.
#[test]
fn old_settings_file_loads_without_preloader_fields() {
    let s = load(
        "my_call = \"TE1ST\"\n\
         [gui]\nmap_provider = \"OpenTopoMap\"\n\
         [background]\nupdate_on_startup = false\n",
    );
    assert_eq!(s.gui.map_provider, "OpenTopoMap");
    assert!(!s.background.update_on_startup);
    assert!(!s.gui.map_preloader_enabled);
    assert_eq!(s.gui.map_preloader_max_zoom, 11);
    assert_eq!(s.background.map_mirror_url, as_maps::MIRROR_URL);
}

/// `map_preloader_max_zoom` is clamped to `0..=11` on load (C# `Map_Preloader_MaxZoom`, the
/// spinner goes to 11); a value out of the `u8` range does not make the whole file unreadable.
#[test]
fn map_preloader_max_zoom_clamped_on_load() {
    for (raw, want) in [
        ("0", 0),
        ("7", 7),
        ("11", 11),
        ("12", 11),
        ("300", 11),
        ("-4", 0),
    ] {
        let s = load(&format!(
            "[gui]\nmap_preloader_enabled = true\nmap_preloader_max_zoom = {raw}\n"
        ));
        assert!(s.gui.map_preloader_enabled, "{raw}");
        assert_eq!(s.gui.map_preloader_max_zoom, want, "{raw}");
    }
}

#[test]
fn map_preloader_settings_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    let mut s = Settings::default();
    s.gui.map_preloader_enabled = true;
    s.gui.map_preloader_max_zoom = 8;
    s.background.map_mirror_url = "http://mirror.test/osm/".into();
    s.save(&p).unwrap();
    assert_eq!(Settings::load_or_default(&p).unwrap(), s);
}
