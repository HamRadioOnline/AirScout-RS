//! `airscout --clean`: port of `CleanupDlg.cs:42-104` and `MapDlg.cs:632-646`.
//!
//! The dialog runs before any database is opened. Nothing is deleted without the explicit
//! "Continue" action. Only the exact computed targets are removed: never `data_dir` itself, never
//! a glob. Foreign files in the data directory survive.
//!
//! Deviations (findings): the targets are computed before anything is deleted (the C# resets the
//! settings first and then reads the DB directories from the reset values, `:49-63`); the C# Log,
//! legacy Elevation and Plugin directories do not exist in the port; the settings are reset by
//! deleting the settings file; a symbolic link is refused, never followed.

use crate::calculator::stamp_file_in;
use crate::maintenance::{data_dir_unsafe, DbId};
use crate::services::sea_file;
use crate::tiles;
use as_elevation::ElevationModel;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// `--clean`, `/CLEAN` or `-CLEAN` (case-insensitive) as the first argument after the program
/// name (`MapDlg.cs:632-640` checks `args[1]` only). `args` includes the program name.
pub fn clean_flag(args: &[String]) -> bool {
    args.get(1)
        .is_some_and(|a| matches!(a.to_uppercase().as_str(), "/CLEAN" | "-CLEAN" | "--CLEAN"))
}

/// What to erase (`CleanupDlg.Designer.cs:59-160`), all on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupChoice {
    pub settings: bool,
    pub aircraft_db: bool,
    pub scoutbase_db: bool,
    pub tmp: bool,
    pub tiles: bool,
}

impl Default for CleanupChoice {
    fn default() -> Self {
        Self {
            settings: true,
            aircraft_db: true,
            scoutbase_db: true,
            tmp: true,
            tiles: true,
        }
    }
}

/// A database file with its SQLite `-wal` and `-shm` companions.
fn db_files(out: &mut Vec<PathBuf>, data_dir: &Path, db: DbId) {
    let name = db.file_name();
    out.push(data_dir.join(&name));
    out.push(data_dir.join(format!("{name}-wal")));
    out.push(data_dir.join(format!("{name}-shm")));
}

/// The exact paths to delete for `c`, computed without touching the file system.
pub fn targets(c: &CleanupChoice, settings_path: &Path, data_dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if c.settings {
        v.push(settings_path.to_path_buf());
    }
    if c.aircraft_db {
        for db in [DbId::Aircraft, DbId::Positions, DbId::Signals] {
            db_files(&mut v, data_dir, db);
        }
    }
    if c.scoutbase_db {
        for db in DbId::all() {
            if !matches!(db, DbId::Aircraft | DbId::Positions | DbId::Signals) {
                db_files(&mut v, data_dir, db);
            }
        }
        // per-model caches the port writes next to the databases (the C# deletes whole dirs)
        for m in ElevationModel::ALL {
            v.push(sea_file(data_dir, m));
            v.push(stamp_file_in(data_dir, m));
        }
    }
    if c.tmp {
        v.push(data_dir.join("Tmp"));
        v.push(data_dir.join("tmp"));
    }
    if c.tiles {
        v.push(tiles::tiles_cache_dir(data_dir));
    }
    v
}

/// Why `t` may not be deleted, or `None`. `t` must be the settings path or a direct child of a
/// safe `data_dir` (empty, relative, root and home are unsafe); `data_dir` itself never.
pub fn refusal(t: &Path, data_dir: &Path, settings_path: &Path) -> Option<String> {
    let is_settings = t == settings_path && t != data_dir && t.file_name().is_some();
    let in_data =
        !data_dir_unsafe(data_dir) && t.parent() == Some(data_dir) && t.file_name().is_some();
    (!(is_settings || in_data))
        .then(|| format!("{}: refused, outside the data directory", t.display()))
}

/// Remove one target. The settings file must be a regular file; named directories (`Tmp`,
/// `tiles`) are removed recursively. A symbolic link is refused, never followed.
fn remove_one(path: &Path, is_settings: bool) -> Result<(), String> {
    let name = path.display();
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{name}: {e}")),
        Ok(m) if m.file_type().is_symlink() => {
            Err(format!("{name}: is a symbolic link, not deleted"))
        }
        Ok(m) if m.is_dir() && is_settings => Err(format!("{name}: is a directory, not deleted")),
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path).map_err(|e| format!("{name}: {e}")),
        Ok(_) => std::fs::remove_file(path).map_err(|e| format!("{name}: {e}")),
    }
}

/// Delete `targets` and collect one error text per failure. A missing path is not an error.
/// Every target must pass `refusal`.
pub fn run(targets: &[PathBuf], data_dir: &Path, settings_path: &Path) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for t in targets {
        if let Some(r) = refusal(t, data_dir, settings_path) {
            errors.push(r);
            continue;
        }
        let is_settings = t == settings_path && t != data_dir;
        if let Err(e) = remove_one(t, is_settings) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// How the dialog ended (`MapDlg.cs:637-646`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// Start the app: after a clean-up, or after Cancel (`DialogResult.Cancel`, nothing deleted).
    Proceed,
    /// The window was closed without a button (`DialogResult.Abort`): exit with -1.
    Abort,
}

/// Dialog state: pure, so it is testable without a display. The deletion runs on a worker
/// thread; `busy` is true from `apply` until `poll` has taken its result.
pub struct CleanupDlg {
    pub choice: CleanupChoice,
    /// Errors of the last run; non-empty means the button reads "Retry".
    pub errors: Vec<String>,
    pub outcome: Option<CleanupOutcome>,
    settings_path: PathBuf,
    data_dir: PathBuf,
    targets: Vec<PathBuf>,
    worker: Option<Receiver<Result<(), Vec<String>>>>,
}

impl CleanupDlg {
    pub fn new(settings_path: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            choice: CleanupChoice::default(),
            errors: Vec::new(),
            outcome: None,
            settings_path,
            data_dir,
            targets: Vec::new(),
            worker: None,
        }
    }

    /// A deletion is running: every button is ignored.
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    /// "Continue" / "Retry" (`btn_OK_Click`): compute the targets once, then delete on a worker
    /// thread. After errors the next call retries the same targets.
    pub fn apply(&mut self) {
        if self.outcome.is_some() || self.busy() {
            return;
        }
        if self.errors.is_empty() {
            self.targets = targets(&self.choice, &self.settings_path, &self.data_dir);
        }
        let (tx, rx) = channel();
        let (t, d, s) = (
            self.targets.clone(),
            self.data_dir.clone(),
            self.settings_path.clone(),
        );
        std::thread::spawn(move || {
            let _ = tx.send(run(&t, &d, &s));
        });
        self.worker = Some(rx);
    }

    /// Take the result of the worker, if it is done. Call every frame.
    pub fn poll(&mut self) {
        let Some(rx) = &self.worker else { return };
        let res = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err(vec!["the cleanup thread ended unexpectedly".to_string()]),
        };
        self.worker = None;
        match res {
            Ok(()) => {
                self.errors.clear();
                self.outcome = Some(CleanupOutcome::Proceed);
            }
            Err(e) => self.errors = e,
        }
    }

    /// Start the app although some deletions failed.
    pub fn proceed_anyway(&mut self) {
        if self.outcome.is_none() && !self.busy() && !self.errors.is_empty() {
            self.outcome = Some(CleanupOutcome::Proceed);
        }
    }

    /// "Cancel" (`btn_Cancel_Click`): start the app, nothing is deleted.
    pub fn cancel(&mut self) {
        if self.outcome.is_none() && !self.busy() {
            self.outcome = Some(CleanupOutcome::Proceed);
        }
    }

    /// The window was closed (`FormClosing` without a button): abort. Ignored while busy.
    pub fn close(&mut self) {
        if self.outcome.is_none() && !self.busy() {
            self.outcome = Some(CleanupOutcome::Abort);
        }
    }
}
