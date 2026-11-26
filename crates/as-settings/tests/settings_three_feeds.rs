//! Three plane feed slots (`Planes_PlaneFeed1/2/3`): `feed` stays slot 1, `feed2` and `feed3`
//! default to "" (= none) and a slot equal to an earlier one is ignored.
#![allow(clippy::field_reassign_with_default)]

use as_settings::Settings;

#[test]
fn fresh_defaults_keep_feed_and_leave_feed2_feed3_empty() {
    let s = Settings::default();
    assert_eq!(s.feed, "OpenSky");
    assert_eq!(s.feed2, "");
    assert_eq!(s.feed3, "");
}

#[test]
fn legacy_file_without_feed2_or_feed3_loads_empty_slots() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    // a file with only the slot-1 key (an older desktop never wrote feed2/feed3)
    std::fs::write(&p, "feed = \"VRSWebServer\"\nmy_call = \"DL0XX\"\n").unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.feed, "VRSWebServer");
    assert_eq!(s.feed2, "");
    assert_eq!(s.feed3, "");
}

#[test]
fn feed_slots_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    let mut s = Settings::default();
    s.feed2 = "VRSWebServer".into();
    s.feed3 = "Readsb".into();
    s.save(&p).unwrap();
    let back = Settings::load_or_default(&p).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.feed2, "VRSWebServer");
    assert_eq!(back.feed3, "Readsb");
    // saving twice does not drift
    back.save(&p).unwrap();
    assert_eq!(Settings::load_or_default(&p).unwrap(), back);
}

#[test]
fn distinct_feeds_skip_empty_and_duplicates() {
    let mut s = Settings::default();
    assert_eq!(s.distinct_feeds(), vec!["OpenSky"]);

    s.feed2 = "OpenSky".into(); // equal to slot 1: one worker per distinct feed
    s.feed3 = "VRSWebServer".into();
    assert_eq!(s.distinct_feeds(), vec!["OpenSky", "VRSWebServer"]);

    s.feed2 = "".into();
    s.feed3 = "[none]".into();
    assert_eq!(s.distinct_feeds(), vec!["OpenSky"]);
}
