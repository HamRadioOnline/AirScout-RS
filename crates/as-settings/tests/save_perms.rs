//! The settings file holds feed credentials: on Unix `save` makes it owner-only.
#![cfg(unix)]

use as_settings::Settings;
use std::os::unix::fs::PermissionsExt;

#[test]
fn saved_file_is_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    Settings::default().save(&p).unwrap();
    let mode = std::fs::metadata(&p).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn save_over_stale_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    let tmp = dir.path().join("settings.toml.tmp");
    std::fs::write(&tmp, "stale").unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
    Settings::default().save(&p).unwrap();
    assert_eq!(Settings::load_or_default(&p).unwrap(), Settings::default());
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!tmp.exists());
}

#[test]
fn failed_rename_removes_tmp() {
    let dir = tempfile::tempdir().unwrap();
    // a directory at the target makes the rename fail
    let p = dir.path().join("settings.toml");
    std::fs::create_dir(&p).unwrap();
    assert!(Settings::default().save(&p).is_err());
    assert!(!dir.path().join("settings.toml.tmp").exists());
}
