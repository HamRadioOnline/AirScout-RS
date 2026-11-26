//! Tile building: the two branches of `ProcessTile` (`ElevationTileGenerator/MainDlg.cs:199-522`)
//! 1:1, and the `.loc` writer.
//!
//! Ported 1:1 (the tiles are what the app reads): sample positions, void rules, the first
//! extreme for min and max, tile names, and which tiles are written. In the SRTM branch
//! `all_void` is set once per file (`:228`) and never reset, so only the all-void tiles before
//! the first tile with data are skipped (finding 3).

use crate::source::{Source, SourceKind};
use as_elevation::{ElevationError, ElevationTile, ELV_MISSING};
use as_geo::maidenhead::{self, PositionInRectangle};
use chrono::{DateTime, Utc};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// What happened to one tile position.
#[derive(Debug)]
pub enum Outcome {
    /// Built; to be written.
    Tile(ElevationTile),
    /// Not written: every sample void.
    Void(String),
    /// Not read: the `.loc` file exists (`File.Exists`, `:239`, `:448`).
    Exists(String),
}

impl Outcome {
    /// The tile index.
    pub fn index(&self) -> &str {
        match self {
            Outcome::Tile(t) => &t.index,
            Outcome::Void(i) | Outcome::Exists(i) => i,
        }
    }
}

fn check_len(src: &Source, data: &[u8]) -> Result<(), ElevationError> {
    if data.len() != src.rows * src.cols * 2 {
        return Err(ElevationError::Corrupt(format!(
            "{}: {} bytes, expected {} for {} x {} samples",
            src.path.display(),
            data.len(),
            src.rows * src.cols * 2,
            src.rows,
            src.cols
        )));
    }
    Ok(())
}

fn corrupt(src: &Source, what: &str) -> ElevationError {
    ElevationError::Corrupt(format!("{}: {what}", src.path.display()))
}

/// Min and max as the C# loop keeps them (`:256-267`, `:465-476`): void samples are skipped,
/// the first extreme wins, the position is `base + k * step`. `any_data` is set by the first
/// sample that is not void (the C# `all_void = false`, `:268-269`, `:477-478`).
struct Extremes {
    min: (i32, f64, f64),
    max: (i32, f64, f64),
    any_data: bool,
}

impl Extremes {
    fn new() -> Self {
        // `ElevationTileDesignator()`: MinElv = short.MaxValue, MaxElv = short.MinValue,
        // positions 0 (`ScoutBase.Elevation/ElevationTile.cs:116-121`)
        Self {
            min: (i32::from(i16::MAX), 0.0, 0.0),
            max: (i32::from(i16::MIN), 0.0, 0.0),
            any_data: false,
        }
    }

    fn add(&mut self, e: i16, lat: f64, lon: f64) {
        if e == ELV_MISSING {
            return;
        }
        let e32 = i32::from(e);
        if e32 < self.min.0 {
            self.min = (e32, lat, lon);
        }
        if e32 > self.max.0 {
            self.max = (e32, lat, lon);
        }
        self.any_data = true;
    }
}

/// Build one tile from a sample function `sample(k, l)` (column `k`, row `l`). Also returns
/// whether any sample is not void.
fn build(
    index: &str,
    rows: usize,
    columns: usize,
    now: DateTime<Utc>,
    mut sample: impl FnMut(usize, usize) -> Option<i16>,
) -> Result<(ElevationTile, bool), ElevationError> {
    let mut ext = Extremes::new();
    let base_lat = maidenhead::lat_from_loc(index, PositionInRectangle::BottomLeft)?;
    let base_lon = maidenhead::lon_from_loc(index, PositionInRectangle::BottomLeft)?;
    // the C# `Rows`/`Columns` setters (`ElevationTile.cs:89`, `:103`)
    let step_lat = 1.0 / 24.0 / rows as f64;
    let step_lon = 2.0 / 24.0 / columns as f64;
    let mut elv = Vec::with_capacity(rows * columns);
    for k in 0..columns {
        for l in 0..rows {
            let e = sample(k, l)
                .ok_or_else(|| ElevationError::Corrupt(format!("tile {index}: sample outside")))?;
            elv.push(e);
            ext.add(
                e,
                base_lat + l as f64 * step_lat,
                base_lon + k as f64 * step_lon,
            );
        }
    }
    let (min, max) = (ext.min, ext.max);
    let tile = ElevationTile::new(
        index, min.0, min.1, min.2, max.0, max.1, max.2, rows, columns, elv, now,
    )?;
    Ok((tile, ext.any_data))
}

fn read_i16(data: &[u8], sample: i64, big_endian: bool) -> Option<i16> {
    let at = usize::try_from(sample).ok()?.checked_mul(2)?;
    let b: [u8; 2] = data.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if big_endian {
        i16::from_be_bytes(b)
    } else {
        i16::from_le_bytes(b)
    })
}

/// The SRTM branch (`:201-302`): 288 outcomes in C# order (`i` 0..12 outer, `j` 0..24 inner).
/// `exists(index)` tells whether the `.loc` file is already there.
pub fn srtm_tiles(
    src: &Source,
    data: &[u8],
    now: DateTime<Utc>,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Vec<Outcome>, ElevationError> {
    if src.kind == SourceKind::Globe {
        return Err(corrupt(src, "not an SRTM source"));
    }
    check_len(src, data)?;
    let loc = maidenhead::loc_from_lat_lon(
        f64::from(src.lat_base),
        f64::from(src.lon_base),
        false,
        3,
        false,
    )?
    .ok_or_else(|| corrupt(src, "base point outside the globe"))?;
    let tilebase = loc.get(..4).ok_or_else(|| corrupt(src, "short locator"))?;
    let rows = src.rows / 24;
    let columns = src.cols / 12;
    let (frows, fcols) = (src.rows as i64, src.cols as i64);
    // `-1 % 2 != 0` in C# too: an odd negative base starts at 'M' (`:230`)
    let x0 = if src.lon_base % 2 == 0 { b'A' } else { b'M' };
    // set once per file and never reset (`:228`, finding 3)
    let mut all_void = true;
    let mut out = Vec::with_capacity(288);
    for i in 0..12u8 {
        for j in 0..24u8 {
            let index = format!("{tilebase}{}{}", (x0 + i) as char, (b'A' + j) as char);
            if exists(&index) {
                out.push(Outcome::Exists(index));
                continue;
            }
            let (i, j) = (i64::from(i), i64::from(j));
            let (tile, any_data) = build(&index, rows, columns, now, |k, l| {
                let (k, l) = (k as i64, l as i64);
                let r = frows - 2 - j * rows as i64 - l;
                let c = i * columns as i64 + k;
                read_i16(data, r * fcols + c, true)
            })?;
            if any_data {
                all_void = false;
            }
            out.push(if all_void {
                Outcome::Void(index)
            } else {
                Outcome::Tile(tile)
            });
        }
    }
    Ok(out)
}

/// The loop values of one GLOBE tile (`:424-437`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobeIndex {
    pub i0: usize,
    pub j0: usize,
    pub i1: usize,
    pub j1: usize,
    pub i2: usize,
    pub j2: usize,
}

/// The GLOBE loops (`:424-437`) in C# order: `j0max` is 4 for 6000 rows, else 3; `i1max` is
/// 9 for `i0 < 4`, else 4.
pub fn globe_indices(src: &Source) -> impl Iterator<Item = GlobeIndex> {
    let j0max = if src.rows == 6000 { 4 } else { 3 };
    (0..=4usize).flat_map(move |i0| {
        let i1max = if i0 < 4 { 9 } else { 4 };
        (0..=j0max).flat_map(move |j0| {
            (0..=i1max).flat_map(move |i1| {
                (0..=9usize).flat_map(move |j1| {
                    (0..24usize).flat_map(move |i2| {
                        (0..24usize).map(move |j2| GlobeIndex {
                            i0,
                            j0,
                            i1,
                            j1,
                            i2,
                            j2,
                        })
                    })
                })
            })
        })
    })
}

/// The tile index of a GLOBE position: the locator of the tile midpoint (`:441-444`).
/// Empty for a `Source` whose base point is not on the globe (not one from `identify`).
pub fn globe_tile_index(src: &Source, g: GlobeIndex) -> String {
    let lat = f64::from(src.lat_base)
        + g.j0 as f64 * 10.0
        + g.j1 as f64 * 1.0
        + g.j2 as f64 * 1.0 / 24.0
        + 0.5 / 24.0;
    let lon = f64::from(src.lon_base)
        + g.i0 as f64 * 20.0
        + g.i1 as f64 * 2.0
        + g.i2 as f64 * 2.0 / 24.0
        + 1.0 / 24.0;
    maidenhead::loc_from_lat_lon(lat, lon, false, 3, false)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// The square (4 characters) of the midpoint of one `(i0, j0, i1, j1)` block. A block is one
/// square (2 deg x 1 deg).
pub fn globe_block_square(src: &Source, g: GlobeIndex) -> String {
    let lat = f64::from(src.lat_base) + g.j0 as f64 * 10.0 + g.j1 as f64 + 0.5;
    let lon = f64::from(src.lon_base) + g.i0 as f64 * 20.0 + g.i1 as f64 * 2.0 + 1.0;
    maidenhead::loc_from_lat_lon(lat, lon, false, 2, false)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// One GLOBE tile (`:450-482`): 5 rows x 10 columns, little-endian, `<= -500` is void.
/// `None` when every sample is void (per tile).
pub fn globe_tile(
    src: &Source,
    data: &[u8],
    g: GlobeIndex,
    now: DateTime<Utc>,
) -> Result<Option<ElevationTile>, ElevationError> {
    if src.kind != SourceKind::Globe {
        return Err(corrupt(src, "not a GLOBE source"));
    }
    check_len(src, data)?;
    let index = globe_tile_index(src, g);
    // `:453-454`: 6000 / 50 / 24 = 4800 / 40 / 24 = 5; 10800 / 90 / 24 * 2 = 10
    let rows = if src.rows == 6000 {
        src.rows / 50 / 24
    } else {
        src.rows / 40 / 24
    };
    let columns = src.cols / 90 / 24 * 2;
    let (frows, fcols) = (src.rows as i64, src.cols as i64);
    let (r, c) = (rows as i64, columns as i64);
    let row0 = frows - 1 - g.j0 as i64 * 10 * 24 * r - g.j1 as i64 * 24 * r - g.j2 as i64 * r;
    let col0 = g.i0 as i64 * 10 * 24 * c + g.i1 as i64 * 24 * c + g.i2 as i64 * c;
    // `all_void` per tile here (`:450`)
    let (tile, any_data) = build(&index, rows, columns, now, |k, l| {
        let e = read_i16(data, (row0 - l as i64) * fcols + col0 + k as i64, false)?;
        Some(if e <= -500 { ELV_MISSING } else { e })
    })?;
    Ok(any_data.then_some(tile))
}

/// `<out>/<index[0..2]>/<index[2..4]>/<index>.loc` (`:209`, `:447`).
pub fn loc_path(out: &Path, index: &str) -> PathBuf {
    let a = index.get(..2).unwrap_or_default();
    let b = index.get(2..4).unwrap_or_default();
    out.join(a).join(b).join(format!("{index}.loc"))
}

/// The tile shapes the generator writes: SRTM3, SRTM1 and GLOBE.
pub(crate) const SHAPES: [(usize, usize); 3] = [(50, 100), (150, 300), (5, 10)];

/// Write a tile as `.loc` (`:274-292`, `:483-508`): create the dirs, write the served layout to
/// `<file>.part`, set its modification time to `last_updated`, then rename it.
pub fn write_loc(out: &Path, tile: &ElevationTile) -> io::Result<PathBuf> {
    let shape = (tile.rows, tile.columns);
    if !SHAPES.contains(&shape) || tile.elv.len() != tile.rows * tile.columns {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "tile {}: {} values for {} x {} is not a generator shape",
                tile.index,
                tile.elv.len(),
                tile.rows,
                tile.columns
            ),
        ));
    }
    if tile.index.len() != 6 || !maidenhead::check(&tile.index) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("tile index {:?} is not a 6-character locator", tile.index),
        ));
    }
    let path = loc_path(out, &tile.index);
    let dir = path.parent().unwrap_or(out);
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{}.loc.part", tile.index));
    let written = (|| {
        let mut f = std::fs::File::create(&part)?;
        f.write_all(tile.to_loc_json().as_bytes())?;
        f.flush()?;
        f.set_modified(tile.last_updated.into())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&part, &path)
    })();
    if let Err(e) = written {
        // only the `.part` this call created
        let _ = std::fs::remove_file(&part);
        return Err(io::Error::new(e.kind(), format!("{}: {e}", path.display())));
    }
    Ok(path)
}
