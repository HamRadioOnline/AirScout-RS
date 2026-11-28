//! "Download data for the area" (`MapDlg.cs` has no equivalent; owner request): preload
//! everything AirScout needs for the configured area so it runs offline in a contest.
//!
//! - Map tiles for the area up to a chosen zoom, from the provider's preload mirror only (see
//!   [`crate::map_preloader`]). The mirror is the airscout.eu mirror the preloader already uses;
//!   a provider without a mirror makes the map part unavailable, never a bulk fetch of
//!   `tile.openstreetmap.org`.
//! - Elevation squares for every enabled model, through [`download_area`].
//!
//! This module holds the pure plan the dialog shows before a run and the job that runs in a
//! maintenance worker thread.

use crate::jobs::{JobCtx, JobEvent};
use crate::map_preloader::{
    finished_text, preload_run, preloader_client, PreloadOutcome, PRELOAD_DELAY, PRELOAD_MIN_ZOOM,
};
use crate::services::{client, sea_file, Paths};
use crate::tiles::store_path;
use as_elevation::{download_area, get_locs_from_rect, ElevationError, ElevationModel};
use as_geo::latlon::GRect;
use as_maps::{preload_tile_count, preload_tiles, TileStore};
use as_settings::ElevationEnabled;

/// Conservative estimate of one stored map tile (an average OpenStreetMap PNG).
pub const MAP_TILE_BYTES: u64 = 30 * 1024;
/// Conservative estimate of one 4-character elevation square (up to 100 `.loc` tiles).
pub const ELEV_SQUARE_BYTES: u64 = 1024 * 1024;

/// The map part of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPlan {
    pub zoom: u8,
    pub tiles: u64,
    pub bytes: u64,
}

/// The elevation part of a plan, per enabled model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElevPlan {
    pub model: ElevationModel,
    pub squares: u64,
    pub bytes: u64,
}

/// What a download will fetch and about how much space it needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AreaPlan {
    pub maps: Option<MapPlan>,
    pub elevation: Vec<ElevPlan>,
}

impl AreaPlan {
    /// Estimated bytes of everything the plan will fetch.
    pub fn total_bytes(&self) -> u64 {
        self.maps.map_or(0, |m| m.bytes) + self.elevation.iter().map(|e| e.bytes).sum::<u64>()
    }

    /// Whether `free` bytes of disk space are enough for this plan.
    pub fn enough(&self, free: u64) -> bool {
        free >= self.total_bytes()
    }
}

/// The plan of a download. `maps` is `Some(max_zoom)` when the Maps box is checked and the
/// provider has a mirror; `elevation` enables every model of `enabled`. Pure: no I/O.
pub fn plan(
    area: &GRect,
    maps: Option<u8>,
    elevation: bool,
    enabled: &ElevationEnabled,
) -> AreaPlan {
    let mut p = AreaPlan::default();
    if let Some(zoom) = maps {
        let tiles = preload_tile_count(area, PRELOAD_MIN_ZOOM, zoom);
        p.maps = Some(MapPlan {
            zoom,
            tiles,
            bytes: tiles.saturating_mul(MAP_TILE_BYTES),
        });
    }
    if elevation {
        let squares = get_locs_from_rect(*area, 2).map_or(0, |v| v.len() as u64);
        for m in ElevationModel::ALL {
            if enabled.is_enabled(m) {
                p.elevation.push(ElevPlan {
                    model: m,
                    squares,
                    bytes: squares.saturating_mul(ELEV_SQUARE_BYTES),
                });
            }
        }
    }
    p
}

/// Run the download job in the calling thread. `maps` is `Some((mirror_url, max_zoom))` and
/// `elevation` is `Some(base_url)`. A hard error aborts (`JobEvent::Failed`); a cancel ends the
/// job with a `Done` and downloads nothing more. Tile and square failures are reported through
/// progress and do not abort the rest.
pub(crate) fn run(
    ctx: &JobCtx,
    paths: &Paths,
    maps: Option<(String, u8)>,
    elevation: Option<String>,
    models: ElevationEnabled,
    area: GRect,
) -> Result<(), String> {
    if let Some((mirror, zoom)) = maps {
        let Some(o) = run_maps(ctx, paths, &mirror, zoom, area)? else {
            ctx.send(JobEvent::Done("download cancelled".into()));
            return Ok(());
        };
        ctx.progress(finished_text(&o));
    }
    if let Some(base) = elevation {
        let Some(()) = run_elevation(ctx, paths, &base, models, area)? else {
            ctx.send(JobEvent::Done("download cancelled".into()));
            return Ok(());
        };
    }
    ctx.send(JobEvent::Done("download finished".into()));
    Ok(())
}

/// The map part: preload the area's tiles from `mirror` into the tile store. `None` if cancelled.
fn run_maps(
    ctx: &JobCtx,
    paths: &Paths,
    mirror: &str,
    zoom: u8,
    area: GRect,
) -> Result<Option<PreloadOutcome>, String> {
    let store = TileStore::open(&store_path(&paths.data_dir)).map_err(|e| e.to_string())?;
    let client = preloader_client()?;
    let list = preload_tiles(&area, PRELOAD_MIN_ZOOM, zoom);
    Ok(preload_run(
        &store,
        &client,
        mirror,
        list,
        PRELOAD_DELAY,
        ctx.cancel.as_ref(),
        &mut |s| ctx.progress(s),
    ))
}

/// The elevation part: download the missing squares of every enabled model. `None` if cancelled.
fn run_elevation(
    ctx: &JobCtx,
    paths: &Paths,
    base: &str,
    models: ElevationEnabled,
    area: GRect,
) -> Result<Option<()>, String> {
    let client = client()?;
    let squares = get_locs_from_rect(area, 2).map_or(0, |v| v.len());
    for m in ElevationModel::ALL {
        if !models.is_enabled(m) {
            continue;
        }
        if ctx.cancelled() {
            return Ok(None);
        }
        let sea = sea_file(&paths.data_dir, m);
        let st = download_area(
            &paths.store,
            &client,
            base,
            m,
            area,
            &sea,
            Some(ctx.cancel.as_ref()),
            &mut |s| ctx.progress(s),
        );
        match st {
            Ok(st) if st.complete => {
                ctx.progress(format!("{}: {} squares complete", m.dir_name(), squares));
            }
            Ok(st) => {
                ctx.progress(format!(
                    "{}: {} squares, {} missing, {} failed",
                    m.dir_name(),
                    squares,
                    st.missing.len(),
                    st.errors.len()
                ));
            }
            Err(ElevationError::Cancelled) => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", m.dir_name())),
        }
    }
    Ok(Some(()))
}
