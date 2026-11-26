use airscout_planeserver::config::*;
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn defaults_match_csharp() {
    let c = PlaneServerConfig::default();
    assert!(c.server_active);
    assert_eq!(c.bind, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(c.port, 9872);
    assert_eq!(DEFAULT_PORT, 9872);
    assert_eq!(MAX_FEEDS, 3);
    assert_eq!(
        WELCOME,
        "<HTML><BODY> Welcome to AirScout PlaneServer</BODY></HTML>"
    );
    assert_eq!((c.min_lat, c.max_lat), (35.0, 60.0));
    assert_eq!((c.min_lon, c.max_lon), (-15.0, 30.0));
    assert_eq!((c.min_alt_m, c.max_alt_m), (5000, 20000));
    assert_eq!(c.lifetime_min, 5);
    assert_eq!(c.interval_s, 60);
    assert!(c.feeds.is_empty());
    assert!(c.feed_settings.is_empty());
    assert!(c.feeds_disclaimer_accepted.is_empty());
    assert!(c.extended_plausibility);
    assert_eq!(c.max_error_dist, 30.0);
    assert_eq!(c.data_dir, None);
    assert!(!c.start_in_tray);
}

#[test]
fn missing_file_gives_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let c = PlaneServerConfig::load_or_default(&dir.path().join("none.toml")).unwrap();
    assert_eq!(c, PlaneServerConfig::default());
}

#[test]
fn broken_file_names_path_not_content() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("planeserver.toml");
    std::fs::write(&p, "port = \"x\"\nsecret = \"hunter2\"\n").unwrap();
    let e = PlaneServerConfig::load_or_default(&p)
        .unwrap_err()
        .to_string();
    assert!(e.contains(p.to_str().unwrap()), "{e}");
    assert!(!e.contains("hunter2"), "{e}");
}

#[test]
fn config_validation() {
    let mut c = PlaneServerConfig {
        port: 0,
        min_lat: 95.0,
        feeds: ["A", "B", "C", "D"].map(String::from).to_vec(),
        interval_s: 0,
        ..Default::default()
    };
    let w = c.validate();
    // port, min_lat clamp, lat swap, feeds, interval
    assert_eq!(w.len(), 5, "{w:?}");
    assert_eq!(c.port, DEFAULT_PORT);
    assert_eq!((c.min_lat, c.max_lat), (60.0, 90.0));
    assert_eq!(c.feeds, ["A", "B", "C"]);
    assert_eq!(c.interval_s, 1);
    assert!(c.validate().is_empty());
}

#[test]
fn validation_edge_cases() {
    let mut c = PlaneServerConfig {
        min_lon: f64::NAN,
        max_lon: 500.0,
        max_error_dist: f64::INFINITY,
        min_alt_m: 9000,
        max_alt_m: 1000,
        lifetime_min: 5000,
        ..Default::default()
    };
    let w = c.validate();
    // two non-finite resets, lon clamp, alt swap, lifetime clamp
    assert_eq!(w.len(), 5, "{w:?}");
    assert_eq!(c.min_lon, -15.0);
    assert_eq!(c.max_lon, 180.0);
    assert_eq!(c.max_error_dist, 30.0);
    assert_eq!((c.min_alt_m, c.max_alt_m), (1000, 9000));
    assert_eq!(c.lifetime_min, 1440);
}

#[test]
fn save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("sub").join("planeserver.toml");
    let mut c = PlaneServerConfig {
        port: 1234,
        feeds: vec!["OpenSky".into()],
        data_dir: Some(dir.path().join("d")),
        ..Default::default()
    };
    c.feed_settings
        .insert("OpenSky".into(), toml::Value::Table(Default::default()));
    c.save(&p).unwrap();
    assert!(!dir.path().join("sub").join("planeserver.toml.tmp").exists());
    assert_eq!(PlaneServerConfig::load_or_default(&p).unwrap(), c);
    assert_eq!(c.data_dir(), dir.path().join("d"));
}

#[test]
fn save_error_names_path_only() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "x").unwrap();
    let p = blocker.join("planeserver.toml");
    let e = PlaneServerConfig::default().save(&p).unwrap_err();
    assert!(e.to_string().contains(blocker.to_str().unwrap()), "{e}");
}

#[test]
fn debug_hides_secrets() {
    let mut c = PlaneServerConfig::default();
    let mut t = toml::Table::new();
    t.insert("password".into(), "hunter2".into());
    c.feed_settings.insert("ADSBSupport".into(), t.into());
    let s = format!("{c:?}");
    assert!(!s.contains("hunter2"), "{s}");
    assert!(s.contains("ADSBSupport"));
}

#[test]
fn default_path_is_planeserver_toml() {
    assert_eq!(default_path().file_name().unwrap(), "planeserver.toml");
}

#[test]
fn restricted_feeds_need_disclaimer() {
    let needs = |n: &str| feed_needs_disclaimer(&*as_feeds::registry::feed_by_name(n).unwrap());
    for n in ["BinCraft", "PublicBinCraft", "ADSBSupport", "FlexJSON"] {
        assert!(needs(n), "{n}");
    }
    assert!(!needs("OpenSky"));
}

#[cfg(unix)]
mod unix_perms {
    use airscout_planeserver::config::PlaneServerConfig;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn saved_file_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("planeserver.toml");
        PlaneServerConfig::default().save(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn save_over_stale_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("planeserver.toml");
        let tmp = dir.path().join("planeserver.toml.tmp");
        std::fs::write(&tmp, "stale").unwrap();
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        let c = PlaneServerConfig::default();
        c.save(&p).unwrap();
        assert_eq!(PlaneServerConfig::load_or_default(&p).unwrap(), c);
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!tmp.exists());
    }

    #[test]
    fn failed_rename_removes_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("planeserver.toml");
        std::fs::create_dir(&p).unwrap();
        assert!(PlaneServerConfig::default().save(&p).is_err());
        assert!(!dir.path().join("planeserver.toml.tmp").exists());
    }
}
