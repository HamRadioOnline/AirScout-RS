//! A picture of one square from its tiles: `PictureFromTiles` (`ElevationTileGenerator/MainDlg.cs:811-844`)
//! with `ElevationTile.ToBitmap` (`ScoutBase/ScoutBase.Elevation/ElevationTile.cs:183-213`),
//! written as PNG instead of `ElevationTiles.tif`.

use crate::pack::{corrupt, loc_files, parse_loc, read_limited, write_via_part};
use crate::tiles::SHAPES;
use as_elevation::{palette_color, ElevationError, ElevationTile, ELV_MISSING};
use as_geo::maidenhead;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Sub-squares of a square per side.
const SUBS: usize = 24;

fn invalid(msg: String) -> ElevationError {
    ElevationError::InvalidArgument(msg)
}

/// Open a file; the error names it.
fn open(path: &Path) -> Result<File, ElevationError> {
    File::open(path).map_err(|e| {
        ElevationError::Io(io::Error::new(e.kind(), format!("{}: {e}", path.display())))
    })
}

/// Only the generator's shapes and a 6-character index with upper-case sub-square letters,
/// so that the image is at most 7200 × 3600 and `index[4]`, `index[5]` are in range.
fn check_tile(t: &ElevationTile) -> Result<(), ElevationError> {
    let b = t.index.as_bytes();
    if b.len() != 6
        || !t.index.is_ascii()
        || !maidenhead::check(&t.index)
        || !b[4..].iter().all(|c| (b'A'..=b'X').contains(c))
    {
        return Err(invalid(format!(
            "tile {:?} is not a 6-character locator such as JO50MB",
            t.index
        )));
    }
    if !SHAPES.contains(&(t.rows, t.columns)) || t.elv.len() != t.rows * t.columns {
        return Err(invalid(format!(
            "tile {}: {} values for {} x {} is not a generator shape",
            t.index,
            t.elv.len(),
            t.rows,
            t.columns
        )));
    }
    Ok(())
}

/// `ToBitmap` (`ElevationTile.cs:194-209`): `e / 3000 * 100` clamped to 0..100, then the
/// palette; a void is opaque black.
fn colour(e: i16) -> [u8; 4] {
    if e == ELV_MISSING {
        return [0, 0, 0, 255];
    }
    let [r, g, b] = palette_color((f64::from(e) / 3000.0 * 100.0).clamp(0.0, 100.0));
    [r, g, b, 255]
}

/// `t` is of the square and shape of `first`.
fn check_same(first: &ElevationTile, t: &ElevationTile) -> Result<(), ElevationError> {
    if t.index[..4] != first.index[..4] || (t.rows, t.columns) != (first.rows, first.columns) {
        return Err(invalid(format!(
            "tile {} ({} x {}) is not of square {} ({} x {})",
            t.index,
            t.rows,
            t.columns,
            &first.index[..4],
            first.rows,
            first.columns
        )));
    }
    Ok(())
}

/// The RGBA image (width, height, 4 bytes per pixel, rows from the top) of one square.
///
/// The tiles must share one square and one shape. The image is `Columns * 24` ×
/// `Rows * 24`. Cell `(k, l)` of a tile goes to `x = (index[4] - 'A') * Columns + k`,
/// `y = height - (index[5] - 'A') * Rows - 1 - l` (`:830-838`). A missing tile is
/// transparent. C# takes the size from the first tile and lets a tile of another size or
/// square throw or draw out of place; here that is an error.
pub fn square_rgba(tiles: &[ElevationTile]) -> Result<(u32, u32, Vec<u8>), ElevationError> {
    let first = tiles
        .first()
        .ok_or_else(|| invalid("no tile to draw".into()))?;
    for t in tiles {
        check_tile(t)?;
        check_same(first, t)?;
    }
    let (rows, cols) = (first.rows, first.columns);
    let (width, height) = (cols * SUBS, rows * SUBS);
    let mut rgba = vec![0u8; width * height * 4];
    for t in tiles {
        let b = t.index.as_bytes();
        let x0 = usize::from(b[4] - b'A') * cols;
        let y0 = usize::from(b[5] - b'A') * rows;
        for k in 0..cols {
            for l in 0..rows {
                let (x, y) = (x0 + k, height - y0 - 1 - l);
                let at = (y * width + x) * 4;
                rgba[at..at + 4].copy_from_slice(&colour(t.elv[k * rows + l]));
            }
        }
    }
    // at most 7200 x 3600 (checked shapes)
    Ok((width as u32, height as u32, rgba))
}

/// The tiles of a square zip (its `.loc` entries) or of a directory (its `*.loc` files,
/// `Directory.GetFiles(dir, "*.loc")`, `:852`). A tile index seen twice is an error.
///
/// Every tile is checked against the first one as soon as it is parsed, so at most one
/// square is held: 576 tiles of at most 300 × 150 values (about 52 MB).
pub fn read_tiles(path: &Path) -> Result<Vec<ElevationTile>, ElevationError> {
    let mut tiles: BTreeMap<String, ElevationTile> = BTreeMap::new();
    let mut add = |t: ElevationTile, at: &Path| -> Result<(), ElevationError> {
        check_tile(&t).map_err(|e| corrupt(at, e))?;
        if let Some(first) = tiles.values().next() {
            check_same(first, &t).map_err(|e| corrupt(at, e))?;
        }
        if tiles.contains_key(&t.index) {
            return Err(corrupt(at, format!("tile {} appears twice", t.index)));
        }
        tiles.insert(t.index.clone(), t);
        Ok(())
    };
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
        for (_, file) in loc_files(path)? {
            let bytes = read_limited(open(&file)?, &file)?;
            add(parse_loc(&bytes, &file)?, &file)?;
        }
    } else {
        let mut z = ZipArchive::new(open(path)?).map_err(|e| corrupt(path, e))?;
        for i in 0..z.len() {
            let entry = z.by_index(i).map_err(|e| corrupt(path, e))?;
            if !entry.name().to_ascii_lowercase().ends_with(".loc") {
                continue;
            }
            let at = PathBuf::from(format!("{}: {}", path.display(), entry.name()));
            let bytes = read_limited(entry, &at)?;
            add(parse_loc(&bytes, &at)?, &at)?;
        }
    }
    Ok(tiles.into_values().collect())
}

/// Write an RGBA image as an 8-bit RGBA PNG via `<file>.part`.
pub fn write_png(file: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), ElevationError> {
    let io = |e: png::EncodingError| ElevationError::Io(io::Error::other(e));
    write_via_part(file, |f| {
        let mut out = BufWriter::new(f);
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(io)?;
        w.write_image_data(rgba).map_err(io)?;
        w.finish().map_err(io)?;
        out.flush()?;
        Ok(())
    })
}
