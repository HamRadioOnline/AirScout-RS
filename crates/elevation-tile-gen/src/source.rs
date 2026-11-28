//! Source file detection: `IsSRTM3File`, `IsSRTM1File`, `IsGLOBEFile`
//! (`ElevationTileGenerator/MainDlg.cs:72-196`) and the source geometry of `ProcessTile`
//! (`:201-221`, `:310-414`).
//!
//! The C# checks index the name before a length check (GLOBE, `:87-90`) or without one (SRTM,
//! `:143-156`, `:177-190`), so a short name throws. Here every length is checked first. The
//! SRTM stem must have exactly 7 characters; C# accepts a longer stem (finding 2).

use as_elevation::ElevationModel;
use std::io;
use std::path::{Path, PathBuf};

/// The kind of a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Srtm3,
    Srtm1,
    Globe,
}

impl SourceKind {
    /// The elevation model whose tiles this source gives.
    pub fn model(self) -> ElevationModel {
        match self {
            SourceKind::Srtm3 => ElevationModel::Srtm3,
            SourceKind::Srtm1 => ElevationModel::Srtm1,
            SourceKind::Globe => ElevationModel::Globe,
        }
    }

    /// The model name (`"SRTM3"`, `"SRTM1"`, `"GLOBE"`).
    pub fn name(self) -> &'static str {
        self.model().dir_name()
    }
}

/// A source file and its geometry. `rows` and `cols` are the samples of the file;
/// `lat_base` and `lon_base` are its south-west corner in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    pub kind: SourceKind,
    pub lat_base: i32,
    pub lon_base: i32,
    pub rows: usize,
    pub cols: usize,
}

const SRTM3_SIDE: usize = 1201;
const SRTM1_SIDE: usize = 3601;
const GLOBE_COLS: usize = 10800;

/// Identify a source file by its name and size. `None` for anything that is not an SRTM3,
/// SRTM1 or GLOBE file, a missing file or a directory.
pub fn identify(path: &Path) -> io::Result<Option<Source>> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let len = meta.len();
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(None);
    };
    let found = srtm(name, len).or_else(|| globe(name, len));
    Ok(found.map(|(kind, lat_base, lon_base, rows, cols)| Source {
        path: path.to_path_buf(),
        kind,
        lat_base,
        lon_base,
        rows,
        cols,
    }))
}

type Geometry = (SourceKind, i32, i32, usize, usize);

fn sample_bytes(rows: usize, cols: usize) -> u64 {
    (rows * cols * 2) as u64
}

/// `IsSRTM3File` / `IsSRTM1File` (`:130-196`) and the base point of `:204-205`.
fn srtm(name: &str, len: u64) -> Option<Geometry> {
    let (stem, ext) = name.rsplit_once('.')?;
    if !ext.eq_ignore_ascii_case("hgt") {
        return None;
    }
    let u = stem.to_ascii_uppercase();
    let b = u.as_bytes();
    if b.len() != 7
        || !matches!(b[0], b'N' | b'S')
        || !matches!(b[3], b'E' | b'W')
        || ![1, 2, 4, 5, 6].iter().all(|&i| b[i].is_ascii_digit())
    {
        return None;
    }
    let side = if len == sample_bytes(SRTM3_SIDE, SRTM3_SIDE) {
        SRTM3_SIDE
    } else if len == sample_bytes(SRTM1_SIDE, SRTM1_SIDE) {
        SRTM1_SIDE
    } else {
        return None;
    };
    let kind = if side == SRTM3_SIDE {
        SourceKind::Srtm3
    } else {
        SourceKind::Srtm1
    };
    let lat: i32 = u[1..3].parse().ok()?;
    let lon: i32 = u[4..7].parse().ok()?;
    let lat = if b[0] == b'S' { -lat } else { lat };
    let lon = if b[3] == b'W' { -lon } else { lon };
    Some((kind, lat, lon, side, side))
}

/// `IsGLOBEFile` (`:72-128`) and the letter table of `:310-414`.
fn globe(name: &str, len: u64) -> Option<Geometry> {
    // no extension: `Path.GetExtension` is empty only without a dot
    if name.contains('.') {
        return None;
    }
    let u = name.to_ascii_uppercase();
    let b = u.as_bytes();
    if b.len() != 4
        || !b[1].is_ascii_digit()
        || !b[2].is_ascii_digit()
        || !matches!(b[3], b'G' | b'B')
    {
        return None;
    }
    let (lat_base, lon_base, rows) = match b[0] {
        b'A' => (50, -180, 4800),
        b'B' => (50, -90, 4800),
        b'C' => (50, 0, 4800),
        b'D' => (50, 90, 4800),
        b'E' => (0, -180, 6000),
        b'F' => (0, -90, 6000),
        b'G' => (0, 0, 6000),
        b'H' => (0, 90, 6000),
        b'I' => (-50, -180, 6000),
        b'J' => (-50, -90, 6000),
        b'K' => (-50, 0, 6000),
        b'L' => (-50, 90, 6000),
        b'M' => (-90, -180, 4800),
        b'N' => (-90, -90, 4800),
        b'O' => (-90, 0, 4800),
        b'P' => (-90, 90, 4800),
        _ => return None,
    };
    if len != sample_bytes(rows, GLOBE_COLS) {
        return None;
    }
    Some((SourceKind::Globe, lat_base, lon_base, rows, GLOBE_COLS))
}
