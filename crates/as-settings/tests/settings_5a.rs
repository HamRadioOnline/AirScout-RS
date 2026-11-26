//! Stage 5a: a settings parse error names the path, the line and the column, never the text.

use as_settings::Settings;

fn load_err(text: &str) -> (std::path::PathBuf, as_settings::SettingsError) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(&p, text).unwrap();
    let e = Settings::load_or_default(&p).unwrap_err();
    // the file is left untouched
    assert_eq!(std::fs::read_to_string(&p).unwrap(), text);
    (p, e)
}

#[test]
fn parse_error_has_no_source_text() {
    let (p, e) = load_err("my_call = \"TE1ST\"\nsecret = \"SECRET-TEST-2");
    let t = e.to_string();
    assert!(t.contains(&p.display().to_string()), "{t}");
    assert!(t.contains("line 2"), "{t}");
    assert!(!t.contains("SECRET-TEST-2"), "{t}");
}

/// The broken line holds a credential; neither `Display` nor `Debug` shows it (C9: the app
/// status bar showed this text).
#[test]
fn parse_error_never_shows_a_secret_in_display_or_debug() {
    let (_, e) = load_err(
        "[cat]\nengine = \"rigctld\"\n[feed_settings.OpenSky]\nclient_secret = \"SECRET-TEST-3",
    );
    let (d, g) = (e.to_string(), format!("{e:?}"));
    assert!(d.contains("line 4, column 31"), "{d}");
    for t in [&d, &g] {
        assert!(!t.contains("SECRET-TEST-3"), "{t}");
        assert!(!t.contains("client_secret"), "{t}");
    }
}

/// A type error quotes the value in toml's message; that message is dropped too.
#[test]
fn type_error_does_not_quote_the_value() {
    let (_, e) = load_err("[cat]\nrigctld_port = \"SECRET-TEST-4\"\n");
    let (d, g) = (e.to_string(), format!("{e:?}"));
    assert!(d.contains("line 2, column 16"), "{d}");
    for t in [&d, &g] {
        assert!(!t.contains("SECRET-TEST-4"), "{t}");
    }
}

#[test]
fn finding_374_settings_debug_omits_url_paths_and_arbitrary_feed_tables() {
    let mut settings = Settings::default();
    let sentinel = "SyntheticCredential";
    let url = format!("http://user:{sentinel}@localhost/{sentinel}?key={sentinel}#{sentinel}");
    settings.background.stations_url = url.clone();
    settings.background.aircraft_url = url.clone();
    settings.background.map_mirror_url = url.clone();
    settings.analysis.history_url = url.clone();
    settings.gui.elevation_base_url = url.clone();
    settings.web.kst_websocket = url.clone();
    settings.speclab.url = url;
    settings
        .feed_settings
        .insert(sentinel.into(), "opaque".into());
    assert!(!format!("{settings:?}").contains(sentinel));
}
