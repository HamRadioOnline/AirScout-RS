//! Port of the download part of `ElevationDatabaseUpdater.cs`: fetch a 4-char square zip of
//! `.loc` tiles and store it. Works in memory and writes all tiles in one transaction.

use crate::{ElevationError, ElevationModel, ElevationStore, ElevationTile};
use as_net::user_agent;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

/// Progress events of the updater.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateProgress {
    Catalogue { done: usize, total: usize },
    Square { square: String, tiles: usize },
}

/// Download `{base}/{MODEL}/{AB}/{ABCD}.zip` and insert every tile (all or nothing).
/// Returns the number of tiles in the square; `Ok(0)` if cancelled before writing.
pub fn update_square(
    client: &reqwest::blocking::Client,
    base_url: &str,
    model: ElevationModel,
    square4: &str,
    store: &ElevationStore,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<usize, ElevationError> {
    let sq = square4.to_uppercase();
    if sq.len() != 4 || !as_geo::maidenhead::check(&sq) {
        return Err(ElevationError::InvalidArgument(format!(
            "not a 4-character square: {square4:?}"
        )));
    }
    let url = format!(
        "{}/{}/{}/{}.zip",
        base_url.trim_end_matches('/'),
        model.dir_name(),
        &sq[..2],
        sq
    );
    let resp = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, user_agent())
        .send()
        .map_err(|_| ElevationError::Http("request or response body failed".into()))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ElevationError::NotFound("HTTP 404".into()));
    }
    if !resp.status().is_success() {
        return Err(ElevationError::Http(format!(
            "HTTP {}",
            resp.status().as_u16()
        )));
    }
    let bytes = resp
        .bytes()
        .map_err(|_| ElevationError::Http("request or response body failed".into()))?;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| ElevationError::Corrupt("invalid square archive".into()))?;
    let mut tiles = Vec::with_capacity(z.len());
    for i in 0..z.len() {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Ok(0);
        }
        let mut f = z
            .by_index(i)
            .map_err(|_| ElevationError::Corrupt("invalid square archive".into()))?;
        if !f.name().to_ascii_lowercase().ends_with(".loc") {
            continue;
        }
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|_| {
            ElevationError::Corrupt("could not decode square archive member".into())
        })?;
        tiles.push(
            ElevationTile::from_loc_json(&s).map_err(|error| match error {
                ElevationError::Parse(error) => ElevationError::ArchiveTile {
                    line: error.line(),
                    column: error.column(),
                },
                _ => ElevationError::Corrupt("invalid square archive tile".into()),
            })?,
        );
    }
    store.insert_tiles_atomic(model, &tiles)?;
    progress(UpdateProgress::Square {
        square: sq,
        tiles: tiles.len(),
    });
    Ok(tiles.len())
}
