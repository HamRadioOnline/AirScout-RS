//! `--clean` (Task 14): targets, deletion safety, dialog state. Temp dirs only.
use airscout::cleanup::{
    clean_flag, refusal, run, targets, CleanupChoice, CleanupDlg, CleanupOutcome,
};
use airscout::maintenance::DbId;
use as_elevation::ElevationModel;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn touch(p: &Path) {
    fs::write(p, b"x").unwrap();
}

/// Poll the dialog until its worker is done.
fn finish(dlg: &mut CleanupDlg) {
    let t = Instant::now();
    while dlg.busy() {
        assert!(t.elapsed() < Duration::from_secs(10), "worker hangs");
        std::thread::sleep(Duration::from_millis(2));
        dlg.poll();
    }
}

/// `HOME`, or `USERPROFILE` where there is no `HOME` (hosted Windows).
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn names(v: &[PathBuf]) -> Vec<String> {
    v.iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// A data dir with every target, plus foreign files that must survive.
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    fs::create_dir_all(data.join("Tmp").join("History")).unwrap();
    fs::create_dir_all(data.join("tmp")).unwrap();
    fs::create_dir_all(data.join("tiles").join("1")).unwrap();
    touch(&data.join("Tmp").join("History").join("a.json"));
    touch(&data.join("tiles").join("1").join("t.png"));
    for db in DbId::all() {
        let n = db.file_name();
        touch(&data.join(&n));
        touch(&data.join(format!("{n}-wal")));
        touch(&data.join(format!("{n}-shm")));
    }
    for m in ElevationModel::ALL {
        let n = m.dir_name().to_lowercase();
        touch(&data.join(format!("{n}-sea-squares.txt")));
        touch(&data.join(format!("calculator-{n}.stamp")));
    }
    touch(&data.join("notes.txt"));
    fs::create_dir_all(data.join("mine")).unwrap();
    touch(&data.join("mine").join("keep.db3"));
    let settings = root.path().join("settings.toml");
    touch(&settings);
    (root, data, settings)
}

#[test]
fn clean_flag_parsed() {
    let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert!(clean_flag(&a(&["airscout", "/CLEAN"])));
    assert!(clean_flag(&a(&["airscout", "-clean"])));
    assert!(clean_flag(&a(&["airscout", "--clean"])));
    assert!(!clean_flag(&a(&["airscout"])));
    assert!(!clean_flag(&a(&["airscout", "--cleanx"])));
    assert!(!clean_flag(&a(&["airscout", "x", "--clean"])));
    assert!(!clean_flag(&a(&["--clean"])));
}

#[test]
fn defaults_all_true() {
    let c = CleanupChoice::default();
    assert!(c.settings && c.aircraft_db && c.scoutbase_db && c.tmp && c.tiles);
}

#[test]
fn targets_cover_all_db_files() {
    let data = Path::new("/d");
    let t = targets(
        &CleanupChoice::default(),
        Path::new("/c/settings.toml"),
        data,
    );
    assert_eq!(t[0], Path::new("/c/settings.toml"));
    for db in DbId::all() {
        for suffix in ["", "-wal", "-shm"] {
            let p = data.join(format!("{}{suffix}", db.file_name()));
            assert!(t.contains(&p), "missing {}", p.display());
        }
    }
    for n in ["Tmp", "tmp", "tiles"] {
        assert!(t.contains(&data.join(n)));
    }
    assert!(!t.contains(&data.to_path_buf()));
    assert!(names(&t).contains(&"propagation_globe.db3".to_string()));
    for m in ElevationModel::ALL {
        let n = m.dir_name().to_lowercase();
        assert!(t.contains(&data.join(format!("{n}-sea-squares.txt"))));
        assert!(t.contains(&data.join(format!("calculator-{n}.stamp"))));
    }
    // choices narrow the list
    let only_tmp = CleanupChoice {
        settings: false,
        aircraft_db: false,
        scoutbase_db: false,
        tmp: true,
        tiles: false,
    };
    assert_eq!(
        targets(&only_tmp, Path::new("/c/s.toml"), data),
        vec![data.join("Tmp"), data.join("tmp")]
    );
}

#[test]
fn targets_computed_before_delete() {
    let (_r, data, settings) = setup();
    let c = CleanupChoice::default();
    let before = targets(&c, &settings, &data);
    run(&before, &data, &settings).unwrap();
    // same list afterwards: it depends on the paths only, not on what exists
    assert_eq!(targets(&c, &settings, &data), before);
    assert!(!settings.exists());
}

#[test]
fn run_deletes_only_targets_and_keeps_foreign_files() {
    let (_r, data, settings) = setup();
    let t = targets(&CleanupChoice::default(), &settings, &data);
    run(&t, &data, &settings).unwrap();
    for p in &t {
        assert!(!p.exists(), "{} still exists", p.display());
    }
    assert!(data.is_dir());
    assert!(data.join("notes.txt").exists());
    assert!(data.join("mine").join("keep.db3").exists());
    // idempotent: missing paths are no error
    run(&t, &data, &settings).unwrap();
}

#[test]
fn run_reports_errors_and_deletes_the_rest() {
    let (root, data, settings) = setup();
    let outside = root.path().join("outside.txt");
    touch(&outside);
    let t = vec![data.join("stations.db3"), data.clone(), outside.clone()];
    let errs = run(&t, &data, &settings).unwrap_err();
    assert_eq!(errs.len(), 2);
    assert!(!data.join("stations.db3").exists());
    assert!(data.is_dir() && data.join("notes.txt").exists());
    assert!(outside.exists());
}

/// Pure path checks: the guard is tested without any delete.
#[test]
fn guard_refuses_unsafe_data_dirs() {
    let s = Path::new("/c/settings.toml");
    for bad in ["", "rel", "/"] {
        let d = Path::new(bad);
        assert!(refusal(&d.join("Tmp"), d, s).is_some(), "{bad:?}");
        assert!(refusal(&d.join("stations.db3"), d, s).is_some(), "{bad:?}");
    }
    if let Some(h) = home() {
        assert!(refusal(&h.join("Tmp"), &h, s).is_some());
    }
    // an absolute path on every OS: "/d" has no drive letter, so Windows reads it as relative
    let d = if cfg!(windows) {
        Path::new(r"C:\d")
    } else {
        Path::new("/d")
    };
    assert!(refusal(&d.join("Tmp"), d, s).is_none());
    assert!(refusal(s, d, s).is_none());
    assert!(refusal(d, d, s).is_some(), "data dir itself");
    assert!(refusal(Path::new("/other/x.db3"), d, s).is_some());
    assert!(
        refusal(&d.join("a").join("b"), d, s).is_some(),
        "not a direct child"
    );
}

#[test]
fn relative_data_dir_refuses_all_targets_but_settings() {
    let settings = Path::new("/c/settings.toml");
    let bad = PathBuf::from("rel-data-dir");
    let t = targets(&CleanupChoice::default(), settings, &bad);
    assert!(t
        .iter()
        .skip(1)
        .all(|p| refusal(p, &bad, settings).is_some()));
    assert!(refusal(settings, &bad, settings).is_none());
}

#[test]
fn settings_directory_is_refused() {
    let (root, data, _s) = setup();
    let dir = root.path().join("settings.toml.d");
    fs::create_dir_all(&dir).unwrap();
    touch(&dir.join("inner"));
    let errs = run(std::slice::from_ref(&dir), &data, &dir).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert!(dir.join("inner").exists());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_not_followed() {
    let (root, data, settings) = setup();
    let outside = root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    touch(&outside.join("precious.txt"));
    fs::remove_dir_all(data.join("tiles")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("tiles")).unwrap();
    fs::remove_dir_all(data.join("Tmp")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("Tmp")).unwrap();
    let t = targets(&CleanupChoice::default(), &settings, &data);
    let errs = run(&t, &data, &settings).unwrap_err();
    // 2 links; a case-insensitive file system also reports `tmp` as the `Tmp` link
    assert!(
        errs.len() >= 2 && errs.iter().all(|e| e.contains("symbolic link")),
        "{errs:?}"
    );
    assert!(outside.join("precious.txt").exists());
    assert!(!data.join("stations.db3").exists());
}

fn everything_exists(data: &Path, settings: &Path) -> bool {
    settings.exists()
        && DbId::all()
            .iter()
            .all(|d| data.join(d.file_name()).exists())
        && data.join("tiles").join("1").join("t.png").exists()
        && data.join("Tmp").join("History").join("a.json").exists()
}

#[test]
fn cleanup_close_deletes_nothing() {
    let (_r, data, settings) = setup();
    let mut dlg = CleanupDlg::new(settings.clone(), data.clone());
    dlg.close();
    assert_eq!(dlg.outcome, Some(CleanupOutcome::Abort));
    // a later Continue after close does nothing
    dlg.apply();
    assert!(!dlg.busy());
    assert_eq!(dlg.outcome, Some(CleanupOutcome::Abort));
    assert!(everything_exists(&data, &settings));
}

/// C# `btn_Cancel_Click` (CleanupDlg.cs:126-131): Cancel starts the app, deletes nothing.
#[test]
fn cancel_starts_app_without_deleting() {
    let (_r, data, settings) = setup();
    let mut dlg = CleanupDlg::new(settings.clone(), data.clone());
    dlg.cancel();
    assert_eq!(dlg.outcome, Some(CleanupOutcome::Proceed));
    dlg.apply();
    assert!(!dlg.busy());
    assert!(everything_exists(&data, &settings));
}

#[test]
fn nothing_deleted_before_continue() {
    let (_r, data, settings) = setup();
    let dlg = CleanupDlg::new(settings.clone(), data.clone());
    assert!(dlg.outcome.is_none() && !dlg.busy());
    assert!(everything_exists(&data, &settings));
}

#[test]
fn continue_runs_on_worker_and_proceeds() {
    let (_r, data, settings) = setup();
    let mut dlg = CleanupDlg::new(settings.clone(), data.clone());
    dlg.choice.settings = false;
    dlg.choice.tiles = false;
    dlg.apply();
    // busy until polled: no outcome, buttons are ignored
    assert!(dlg.busy() && dlg.outcome.is_none());
    dlg.apply();
    dlg.cancel();
    dlg.close();
    dlg.proceed_anyway();
    assert!(dlg.outcome.is_none());
    finish(&mut dlg);
    assert_eq!(dlg.outcome, Some(CleanupOutcome::Proceed));
    assert!(settings.exists());
    assert!(data.join("tiles").exists());
    assert!(!data.join("Tmp").exists());
    assert!(!data.join("stations.db3").exists());
    assert!(!data.join("globe-sea-squares.txt").exists());
    assert!(!data.join("calculator-srtm3.stamp").exists());
    assert!(data.join("notes.txt").exists());
}

#[cfg(unix)]
#[test]
fn retry_after_error_then_proceed() {
    let (root, data, settings) = setup();
    let outside = root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::remove_dir_all(data.join("tiles")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("tiles")).unwrap();
    let mut dlg = CleanupDlg::new(settings, data.clone());
    dlg.apply();
    finish(&mut dlg);
    assert_eq!(dlg.errors.len(), 1);
    assert!(dlg.outcome.is_none());
    dlg.apply();
    finish(&mut dlg);
    assert_eq!(dlg.errors.len(), 1);
    dlg.proceed_anyway();
    assert_eq!(dlg.outcome, Some(CleanupOutcome::Proceed));
    assert!(outside.exists());
}

#[test]
fn data_dir_and_settings_path_are_absolute() {
    assert!(airscout::app::data_dir().is_absolute());
    assert!(as_settings::Settings::default_path().is_absolute());
}
