//! Elevation coverage maps (Options Elevation tab) and the elevation grid of the station map.
//!
//! - Coverage (`OptionsDlg.cs:1356-1461`): which tiles of the update server's catalogue the
//!   local database has. The C# starts on tab Enter; the port only after "Show coverage".
//! - Elevation grid (`MapStationDlg.cs:269-424`): the elevation of 1" x 2" cells over the view
//!   at zoom 17 and above.
//!
//! Both run as maintenance jobs; this module holds the jobs and the pure logic of the drawing.

use crate::jobs::{JobCtx, JobEvent};
use crate::overlays::MAX_GRID_CELLS;
use crate::services::{client, Paths};
use crate::ui::map_geom::{contains, pad};
use as_elevation::{elevation_color, Catalogue, ElevationModel};
use as_geo::latlon::GRect;
use as_geo::maidenhead::{self, PositionInRectangle};
use std::collections::BTreeMap;

/// The grid shows from this map zoom on (`gm_Callsign.Zoom >= 17`, `MapStationDlg.cs:285`).
pub const GRID_MIN_ZOOM: f64 = 17.0;
/// Cell height [deg] of the grid: 1" (`MapStationDlg.cs:293`).
const STEP_LAT: f64 = 5.555_555_555_555_556e-4 / 2.0;
/// Cell width [deg] of the grid: 2".
const STEP_LON: f64 = 5.555_555_555_555_556e-4;

// ---- coverage ----

/// Bottom-left of the 6-character locator, 1/24 deg high and 2/24 deg wide (`:1428-1432`).
/// `None` if `index6` is not a 6-character locator.
pub fn tile_cell(index6: &str) -> Option<GRect> {
    if index6.len() != 6 || !maidenhead::check(index6) {
        return None;
    }
    let (lat, lon) = maidenhead::lat_lon_from_loc(index6, PositionInRectangle::BottomLeft).ok()?;
    Some(GRect::new(lat, lon, lat + 1.0 / 24.0, lon + 2.0 / 24.0))
}

/// What a drawn coverage cell stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverState {
    /// In the database (green).
    Found,
    /// On the server, not in the database (red).
    Missing,
    /// A coarse cell that holds both (zoomed far out).
    Mixed,
}

struct Agg {
    rect: GRect,
    found: u32,
    missing: u32,
}

impl Agg {
    fn state(&self) -> CoverState {
        match (self.found, self.missing) {
            (_, 0) => CoverState::Found,
            (0, _) => CoverState::Missing,
            _ => CoverState::Mixed,
        }
    }
}

/// The tiles of one coverage run, kept at three levels (tile, 4-character square, 2-character
/// field) so a view of the whole area stays within the cell cap. A catalogue can hold some
/// 300 000 tiles; drawing them all every frame would stall the UI.
pub struct CoverageData {
    levels: [Vec<Agg>; 3],
    found: usize,
    missing: usize,
}

impl CoverageData {
    pub fn new(found: &[String], missing: &[String]) -> Self {
        let mut maps: [BTreeMap<&str, (u32, u32)>; 3] = Default::default();
        let tiles = found
            .iter()
            .map(|t| (t, true))
            .chain(missing.iter().map(|t| (t, false)));
        for (t, is_found) in tiles {
            for (level, len) in [(0, 6), (1, 4), (2, 2)] {
                if let Some(key) = t.get(..len) {
                    let e = maps[level].entry(key).or_default();
                    if is_found {
                        e.0 += 1;
                    } else {
                        e.1 += 1;
                    }
                }
            }
        }
        let levels = maps.map(|m| {
            m.into_iter()
                .filter_map(|(key, (found, missing))| {
                    let rect = if key.len() == 6 {
                        tile_cell(key)?
                    } else {
                        maidenhead::bounds_from_loc(key).ok()?
                    };
                    Some(Agg {
                        rect,
                        found,
                        missing,
                    })
                })
                .collect()
        });
        Self {
            levels,
            found: found.len(),
            missing: missing.len(),
        }
    }

    /// `"{n} tile(s) found, {m} more tile(s) available and missing."` (`:1415`).
    pub fn status(&self) -> String {
        format!(
            "{} tile(s) found, {} more tile(s) available and missing.",
            self.found, self.missing
        )
    }

    /// The cells that meet `view`: single tiles if at most [`MAX_GRID_CELLS`] fit, otherwise
    /// the 4-character squares, otherwise the 2-character fields.
    pub fn visible(&self, view: GRect) -> Vec<(GRect, CoverState)> {
        let dlat = (view.max_lat - view.min_lat).abs();
        let dlon = (view.max_lon - view.min_lon).abs();
        // cells per axis (+2: the cells at the edges)
        let count = |lat: f64, lon: f64| ((dlat / lat) + 2.0) * ((dlon / lon) + 2.0);
        let sizes = [(1.0 / 24.0, 2.0 / 24.0), (1.0, 2.0), (10.0, 20.0)];
        let level = sizes
            .iter()
            .position(|&(la, lo)| count(la, lo) <= MAX_GRID_CELLS as f64)
            .unwrap_or(2);
        self.levels[level]
            .iter()
            .filter(|c| {
                c.rect.min_lat <= view.max_lat
                    && c.rect.max_lat >= view.min_lat
                    && c.rect.min_lon <= view.max_lon
                    && c.rect.max_lon >= view.min_lon
            })
            .take(MAX_GRID_CELLS)
            .map(|c| (c.rect, c.state()))
            .collect()
    }
}

/// Download the catalogue and sort its tiles by what the database has (`bw_*_MapUpdater_DoWork`).
/// The small `files.zip` goes to a directory of its own in `<data>/tmp`, removed afterwards. A cancelled job sends nothing.
pub(crate) fn run_coverage(
    ctx: &JobCtx,
    paths: &Paths,
    model: ElevationModel,
    base_url: &str,
    area: GRect,
) -> Result<(), String> {
    ctx.progress(format!(
        "{}: Creating elevation tile catalogue...",
        model.dir_name()
    ));
    // a directory per job: a cancelled job may still be downloading when the next one starts,
    // and the background updater uses `<data>/tmp` too
    let cache = paths
        .data_dir
        .join("tmp")
        .join(format!("coverage-{}", ctx.id));
    let cat = Catalogue::fetch(&client()?, base_url, model, area, &cache);
    let _ = std::fs::remove_dir_all(&cache);
    let cat = cat.map_err(|e| e.to_string())?;
    ctx.progress(format!("{}: Processing tiles...", model.dir_name()));
    let (mut found, mut missing) = (Vec::new(), Vec::new());
    for key in cat.files.keys() {
        if ctx.cancelled() {
            return Ok(());
        }
        let Some(index) = key.get(..6) else { continue };
        if paths
            .store
            .tile_exists(model, index)
            .map_err(|e| e.to_string())?
        {
            found.push(index.to_string());
        } else {
            missing.push(index.to_string());
        }
    }
    if !ctx.cancelled() {
        ctx.send(JobEvent::Coverage { found, missing });
    }
    Ok(())
}

// ---- elevation grid ----

/// True if the grid is drawn: the overlay is on, the zoom is at least [`GRID_MIN_ZOOM`] and a
/// model is enabled.
pub fn grid_wanted(overlay: bool, zoom: f64, model: ElevationModel) -> bool {
    overlay && zoom >= GRID_MIN_ZOOM && model != ElevationModel::None
}

/// The cells of the grid over `view` (`bw_Elevationgrid_DoWork`, `:288-336`): the view is
/// snapped outward to 6-character locator bounds, and the cells of 1" x 2" that meet the view
/// are kept. More than [`MAX_GRID_CELLS`] cells (the 3a cap rule) are drawn with a larger
/// stride, so a big view still shows a grid. The C# has no cap.
pub fn grid_cells(view: GRect) -> Vec<GRect> {
    if ![view.min_lat, view.min_lon, view.max_lat, view.max_lon]
        .iter()
        .all(|x| x.is_finite())
    {
        return Vec::new();
    }
    let v = GRect::new(
        view.min_lat.max(-90.0),
        view.min_lon.max(-180.0),
        view.max_lat.min(89.999_999),
        view.max_lon.min(179.999_999),
    );
    if v.min_lat >= v.max_lat || v.min_lon >= v.max_lon {
        return Vec::new();
    }
    let Some((min_lat, min_lon, max_lat, max_lon)) = snapped(v) else {
        return Vec::new();
    };
    let range = |lo: f64, hi: f64, start: f64, end: f64, step: f64| -> (i64, i64) {
        let a = ((lo - step - start) / step).ceil().max(0.0) as i64;
        let b =
            (((hi - start) / step).floor() as i64).min(((end - start) / step).ceil() as i64 - 1);
        (a, b)
    };
    let (la, lb) = range(v.min_lat, v.max_lat, min_lat, max_lat, STEP_LAT);
    let (oa, ob) = range(v.min_lon, v.max_lon, min_lon, max_lon, STEP_LON);
    let (n_lat, n_lon) = ((lb - la + 1).max(0) as u64, (ob - oa + 1).max(0) as u64);
    let mut stride = 1_u64;
    while n_lat.div_ceil(stride) * n_lon.div_ceil(stride) > MAX_GRID_CELLS as u64 {
        stride += 1;
    }
    let mut cells = Vec::new();
    for i in (la..=lb).step_by(stride as usize) {
        let lat = min_lat + i as f64 * STEP_LAT;
        for j in (oa..=ob).step_by(stride as usize) {
            let lon = min_lon + j as f64 * STEP_LON;
            // the C# test, on the first cell of a stride
            if lat + STEP_LAT >= v.min_lat
                && lat <= v.max_lat
                && lon + STEP_LON >= v.min_lon
                && lon <= v.max_lon
            {
                cells.push(GRect::new(
                    lat,
                    lon,
                    lat + STEP_LAT * stride as f64,
                    lon + STEP_LON * stride as f64,
                ));
            }
        }
    }
    cells.truncate(MAX_GRID_CELLS);
    cells
}

/// Bottom-left of the 6-character locator of the view's lower-left corner and top-right of the
/// locator of its upper-right corner (`:293-299`).
fn snapped(v: GRect) -> Option<(f64, f64, f64, f64)> {
    let loc = |lat, lon| maidenhead::loc_from_lat_lon(lat, lon, false, 3, false).ok()?;
    let lo = loc(v.min_lat, v.min_lon)?;
    let hi = loc(v.max_lat, v.max_lon)?;
    let (min_lat, min_lon) =
        maidenhead::lat_lon_from_loc(&lo, PositionInRectangle::BottomLeft).ok()?;
    let (max_lat, max_lon) =
        maidenhead::lat_lon_from_loc(&hi, PositionInRectangle::TopRight).ok()?;
    Some((min_lat, min_lon, max_lat, max_lon))
}

/// Colour of every cell: `(elv - min) / (max - min) * 10000` through the palette, normalised
/// over the cells (`:315-319`). A flat view gives 0 where the C# divides 0 by 0 (NaN).
pub fn grid_colours(cells: &[(GRect, i16)]) -> Vec<[u8; 3]> {
    let min = cells.iter().map(|c| c.1).min().map_or(0.0, f64::from);
    let max = cells.iter().map(|c| c.1).max().map_or(0.0, f64::from);
    cells
        .iter()
        .map(|&(_, elv)| {
            let v = if max == min {
                0.0
            } else {
                (f64::from(elv) - min) / (max - min) * 10_000.0
            };
            elevation_color(v)
        })
        .collect()
}

/// The elevation of the centre of every cell of `rect` for the selected model only (`:320`
/// reads `Elevation.Database[lat, lon, ElevationModel]`, whose `setinvalidtozero` defaults to
/// true: a missing tile gives 0; no fall-through to other models).
pub(crate) fn run_grid(
    ctx: &JobCtx,
    paths: &Paths,
    rect: GRect,
    model: ElevationModel,
) -> Result<(), String> {
    ctx.progress("Calculating elevation grid...");
    let mut out = Vec::new();
    for c in grid_cells(rect) {
        if ctx.cancelled() {
            return Ok(());
        }
        let (lat, lon) = ((c.min_lat + c.max_lat) / 2.0, (c.min_lon + c.max_lon) / 2.0);
        let elv = paths
            .store
            .elevation(model, lat, lon, true)
            .map_err(|e| e.to_string())?;
        out.push((c, elv));
    }
    ctx.send(JobEvent::ElevationGrid(out));
    Ok(())
}

/// What the grid controller asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridRequest {
    /// Calculate the grid of `rect`; it supersedes an older job.
    Run { rect: GRect, model: ElevationModel },
    /// Stop the job and drop the grid.
    Cancel,
}

/// A cell with its colour, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct GridCell {
    pub rect: GRect,
    pub elv: i16,
    pub colour: [u8; 3],
}

/// The grid of the station map: when to ask for a job and what it delivered.
pub struct GridState {
    overlay: bool,
    model: ElevationModel,
    /// The area the running or finished job covers.
    covers: Option<GRect>,
    cells: Vec<(GRect, i16)>,
}

impl GridState {
    pub fn new(overlay: bool, model: ElevationModel) -> Self {
        Self {
            overlay,
            model,
            covers: None,
            cells: Vec::new(),
        }
    }

    pub fn overlay(&self) -> bool {
        self.overlay
    }

    pub fn model(&self) -> ElevationModel {
        self.model
    }

    pub fn set_overlay(&mut self, on: bool) {
        self.overlay = on;
    }

    /// The cells the job delivered (raw, not normalised).
    pub fn cells(&self) -> &[(GRect, i16)] {
        &self.cells
    }

    /// The cells that meet `view`, coloured normalised over those cells only (`:315-319` takes
    /// min and max over the view, not over a margin around it).
    pub fn visible(&self, view: GRect) -> Vec<GridCell> {
        let seen: Vec<(GRect, i16)> = self
            .cells
            .iter()
            .filter(|(r, _)| {
                r.min_lat <= view.max_lat
                    && r.max_lat >= view.min_lat
                    && r.min_lon <= view.max_lon
                    && r.max_lon >= view.min_lon
            })
            .copied()
            .collect();
        let colours = grid_colours(&seen);
        seen.into_iter()
            .zip(colours)
            .map(|((rect, elv), colour)| GridCell { rect, elv, colour })
            .collect()
    }

    /// Called every frame with the visible `view` and the zoom. Below the minimum zoom, with the
    /// overlay off or without a model, a running job is cancelled and the grid dropped. A view
    /// that left the covered area starts a newer job for the view plus a margin, so panning
    /// does not start one per frame.
    pub fn update(&mut self, view: GRect, zoom: f64) -> Option<GridRequest> {
        if !grid_wanted(self.overlay, zoom, self.model) {
            self.cells.clear();
            return self.covers.take().map(|_| GridRequest::Cancel);
        }
        if self.covers.as_ref().is_some_and(|c| contains(c, &view)) {
            return None;
        }
        let rect = pad(&view);
        self.covers = Some(rect);
        Some(GridRequest::Run {
            rect,
            model: self.model,
        })
    }

    /// The job delivered its cells.
    pub fn set_cells(&mut self, cells: Vec<(GRect, i16)>) {
        self.cells = cells;
    }
}
