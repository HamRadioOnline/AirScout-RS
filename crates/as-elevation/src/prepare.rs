//! Make the elevation squares of an area available (moved from `airscout-cli`).

use crate::{
    get_locs_from_rect, update_square, DbStatus, ElevationError, ElevationModel, ElevationStore,
};
use as_geo::latlon::GRect;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Result of [`prepare_area`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaStatus {
    /// Every square has tiles or is known to have no file on the server (sea).
    pub complete: bool,
    /// 4-character squares that have no tiles (and are not sea).
    pub missing: Vec<String>,
    /// Download failures other than 404 (`"JO60: <error>"`); their squares are in `missing`.
    pub errors: Vec<String>,
    /// Squares newly recorded as sea in `sea_file`.
    pub sea_added: usize,
}

/// Where [`scan_area`] downloads from; `None`: check only.
type Source<'a> = Option<(&'a reqwest::blocking::Client, &'a str)>;

/// Shared per-square walk of [`prepare_area`], [`download_area`] and [`check_area`]: download
/// (if a source is given) the squares without tiles, record 404 squares as sea, collect
/// `missing` and `errors`.
fn scan_area(
    store: &ElevationStore,
    source: Source<'_>,
    model: ElevationModel,
    rect: GRect,
    sea_file: &Path,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(String),
) -> Result<AreaStatus, ElevationError> {
    let squares = get_locs_from_rect(rect, 2)?;
    let has_tiles = |sq: &str| -> Result<bool, ElevationError> {
        let pattern = format!("{sq}%");
        store.with_conn(model, |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM Elevation WHERE TileIndex LIKE ?1)",
                [&pattern],
                |r| r.get::<_, bool>(0),
            )?)
        })
    };
    let mut sea: BTreeSet<String> = std::fs::read_to_string(sea_file)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let sea_before = sea.len();
    let mut missing = Vec::new();
    let mut errors = Vec::new();
    for sq in &squares {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(ElevationError::Cancelled);
        }
        if sea.contains(sq) || has_tiles(sq)? {
            continue;
        }
        if let Some((client, base_url)) = source {
            progress(format!("downloading {} {sq} ...", model.dir_name()));
            match update_square(client, base_url, model, sq, store, cancel, &mut |_| {}) {
                Ok(_) => {}
                Err(ElevationError::NotFound(_)) => {
                    progress(format!("no file on the server for {sq} (sea)"));
                    sea.insert(sq.clone());
                    continue;
                }
                // A failed download (network, HTTP 5xx, bad zip) does not abort the run: the
                // square stays without tiles, lands in `missing` below and so the area is
                // incomplete, and the COMPLETE bit of an earlier area is reset.
                Err(e) => {
                    progress(format!("download of {sq} failed: {e}"));
                    errors.push(format!("{sq}: {e}"));
                }
            }
        }
        if !has_tiles(sq)? {
            progress(format!("no {} tiles for square {sq}", model.dir_name()));
            missing.push(sq.clone());
        }
    }
    let sea_added = sea.len() - sea_before;
    if sea_added > 0 {
        if let Some(dir) = sea_file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text: String = sea.iter().map(|s| format!("{s}\n")).collect();
        std::fs::write(sea_file, text)?;
    }
    Ok(AreaStatus {
        complete: missing.is_empty(),
        missing,
        errors,
        sea_added,
    })
}

/// Make sure the 4-character squares covering `rect` are in the store (downloading them if
/// `download`) and mark the model complete for `rect` only if every square has tiles or is
/// known to have no file on the server (sea, HTTP 404; kept in `sea_file`). The store keeps its
/// status in memory, so this decides path validity for this run. A square whose download fails
/// (other than 404) is recorded in `missing` and the run goes on with the next square; the error
/// text goes to `progress`.
#[allow(clippy::too_many_arguments)]
pub fn prepare_area(
    store: &ElevationStore,
    client: &reqwest::blocking::Client,
    base_url: &str,
    model: ElevationModel,
    rect: GRect,
    sea_file: &Path,
    download: bool,
    progress: &mut dyn FnMut(String),
) -> Result<AreaStatus, ElevationError> {
    let source = download.then_some((client, base_url));
    let st = scan_area(store, source, model, rect, sea_file, None, progress)?;
    store.set_bounds(rect);
    if st.complete {
        store.set_status_bit(model, DbStatus::COMPLETE);
    } else {
        // a bit left over from an earlier, complete area must not make this one valid
        store.reset_status_bit(model, DbStatus::COMPLETE);
    }
    Ok(st)
}

/// Download the squares of `rect` that are missing, like [`prepare_area`] with `download`, but
/// leave the store's bounds and status untouched: for background updaters, which must not change
/// what a path calculation running at the same time considers valid. `cancel` is checked per
/// square and passed on to the download; a cancelled run gives `Err(Cancelled)`.
#[allow(clippy::too_many_arguments)]
pub fn download_area(
    store: &ElevationStore,
    client: &reqwest::blocking::Client,
    base_url: &str,
    model: ElevationModel,
    rect: GRect,
    sea_file: &Path,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(String),
) -> Result<AreaStatus, ElevationError> {
    scan_area(
        store,
        Some((client, base_url)),
        model,
        rect,
        sea_file,
        cancel,
        progress,
    )
}

/// Check, without any download and without touching the store's bounds and status, whether
/// every square of `rect` has tiles or is recorded as sea in `sea_file` (the original's
/// `IsDatabaseComplete`, ElevationDatabaseUpdater.cs:242). `cancel` is checked per square.
pub fn check_area(
    store: &ElevationStore,
    model: ElevationModel,
    rect: GRect,
    sea_file: &Path,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(String),
) -> Result<AreaStatus, ElevationError> {
    scan_area(store, None, model, rect, sea_file, cancel, progress)
}
