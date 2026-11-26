//! SRTM file tools: `CompareFilesHGT` and its statistics (`ElevationTileGenerator/MainDlg.cs:1018-1150`)
//! and `ConvertSRTM1ToSRTM3File` (`:1252-1301`).
//!
//! Every function works on bytes in memory and never panics on input of any length.

use crate::pack::write_via_part;
use as_elevation::{ElevationError, ELV_MISSING};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

const SRTM1_SIDE: usize = 3601;
const SRTM3_SIDE: usize = 1201;
/// The largest file `read_file` reads: a GLOBE file of 6000 × 10800 samples.
const MAX_FILE_BYTES: u64 = 6000 * 10800 * 2;

/// An I/O error that names the file.
fn io_at(path: &Path, e: io::Error) -> ElevationError {
    ElevationError::Io(io::Error::new(e.kind(), format!("{}: {e}", path.display())))
}

/// The whole file, at most 6000 × 10800 × 2 bytes (the largest source file).
pub fn read_file(path: &Path) -> Result<Vec<u8>, ElevationError> {
    let too_big = || {
        ElevationError::InvalidArgument(format!(
            "{}: larger than the largest source file ({MAX_FILE_BYTES} bytes)",
            path.display()
        ))
    };
    let f = File::open(path).map_err(|e| io_at(path, e))?;
    // refused before any byte is read; `take` still caps a file that grows meanwhile
    if f.metadata().map_err(|e| io_at(path, e))?.len() > MAX_FILE_BYTES {
        return Err(too_big());
    }
    let mut b = Vec::new();
    f.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut b)
        .map_err(|e| io_at(path, e))?;
    if b.len() as u64 > MAX_FILE_BYTES {
        return Err(too_big());
    }
    Ok(b)
}

/// Write `bytes` to `<path>.part`, then rename it over `path`.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ElevationError> {
    write_via_part(path, |f| Ok(f.write_all(bytes)?)).map_err(|e| match e {
        ElevationError::Io(e) => io_at(path, e),
        e => e,
    })
}

/// The statistics of a difference file. Not the C# statistics (`:1018-1088`, finding 10):
/// voids are `-32768` (C# counts `-512`), and min, max and mean leave voids out.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct DiffStats {
    /// Samples in the file.
    pub samples: usize,
    /// `floor(sqrt(samples))`, the side of the C# bitmap (`:1025`).
    pub width: usize,
    /// Non-void samples other than 0.
    pub differing: usize,
    /// `-32768` samples.
    pub voids: usize,
    /// Smallest non-void sample.
    pub min: Option<i16>,
    /// Largest non-void sample.
    pub max: Option<i16>,
    /// Mean of the non-void samples.
    pub mean: Option<f64>,
}

fn be_samples(b: &[u8]) -> impl Iterator<Item = i16> + '_ {
    b.as_chunks::<2>().0.iter().map(|p| i16::from_be_bytes(*p))
}

/// `CompareFilesHGT` (`:1092-1139`): `a - b` per big-endian sample, `-32768` unless both
/// samples are `>= -500`, written little-endian (`BinaryWriter.Write(short)`, `:1133`).
///
/// C# compares `length / 2` and throws on an odd byte (`:1095-1122`); here an odd length is
/// an error.
pub fn diff_bytes(a: &[u8], b: &[u8]) -> Result<Vec<u8>, ElevationError> {
    if a.len() != b.len() {
        return Err(ElevationError::InvalidArgument(
            "cannot compare files with different length".into(),
        ));
    }
    if !a.len().is_multiple_of(2) {
        return Err(ElevationError::InvalidArgument(
            "cannot compare files of odd length".into(),
        ));
    }
    Ok(be_samples(a)
        .zip(be_samples(b))
        .flat_map(|(e1, e2)| {
            let e = if e1 >= -500 && e2 >= -500 {
                e1.wrapping_sub(e2)
            } else {
                ELV_MISSING
            };
            e.to_le_bytes()
        })
        .collect())
}

/// The statistics and the histogram (value, count) of a little-endian difference file.
/// A trailing odd byte is ignored.
pub fn diff_stats(diff: &[u8]) -> (DiffStats, BTreeMap<i16, u64>) {
    let mut hist = BTreeMap::new();
    let (mut samples, mut voids, mut differing, mut sum) = (0usize, 0usize, 0usize, 0i64);
    for e in diff
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes(*p))
    {
        samples += 1;
        if e == ELV_MISSING {
            voids += 1;
            continue;
        }
        if e != 0 {
            differing += 1;
        }
        sum += i64::from(e);
        *hist.entry(e).or_insert(0u64) += 1;
    }
    let valid = samples - voids;
    let stats = DiffStats {
        samples,
        width: samples.isqrt(),
        differing,
        voids,
        min: hist.keys().next().copied(),
        max: hist.keys().next_back().copied(),
        mean: (valid > 0).then(|| sum as f64 / valid as f64),
    };
    (stats, hist)
}

/// `ElevationDifference.csv` (`:1075-1082`): a header, then every value `min..=max` with its
/// count (0 included).
pub fn histogram_csv(h: &BTreeMap<i16, u64>, min: i16, max: i16) -> String {
    let mut s = String::from("Difference[m];Count\n");
    for v in min..=max {
        s.push_str(&format!("{v};{}\n", h.get(&v).copied().unwrap_or(0)));
    }
    s
}

/// `ConvertSRTM1ToSRTM3File` (`:1263-1301`): an SRTM1 file (3601² big-endian samples) to an
/// SRTM3 file (1201²).
///
/// The sample layout is 1:1: point `(i, j)` for `i, j < 1200` is the mean of the 3 × 3 block
/// at rows `3i..3i+2`, columns `3j..3j+2`; column 1200 repeats the row's last point
/// (`:1288-1290`); row 1200 is SRTM1 row 3600 at columns `3j` (`:1292-1297`).
///
/// Port deviation (owner decision S13): C# sums in a `short`, which wraps above 32 767, and
/// averages voids as numbers. Here the sum is `i32`, and a block with any `-32768` sample
/// gives `-32768`.
pub fn downsample(srtm1: &[u8]) -> Result<Vec<u8>, ElevationError> {
    if srtm1.len() != SRTM1_SIDE * SRTM1_SIDE * 2 {
        return Err(ElevationError::InvalidArgument(format!(
            "an SRTM1 file has {} bytes, not {}",
            SRTM1_SIDE * SRTM1_SIDE * 2,
            srtm1.len()
        )));
    }
    let at = |r: usize, c: usize| {
        let i = (r * SRTM1_SIDE + c) * 2;
        i16::from_be_bytes([srtm1[i], srtm1[i + 1]])
    };
    let mut out = Vec::with_capacity(SRTM3_SIDE * SRTM3_SIDE * 2);
    for i in 0..SRTM3_SIDE - 1 {
        let mut e = 0i16;
        for j in 0..SRTM3_SIDE - 1 {
            let mut sum = 0i32;
            let mut void = false;
            for k in 0..3 {
                for l in 0..3 {
                    let v = at(i * 3 + k, j * 3 + l);
                    void |= v == ELV_MISSING;
                    sum += i32::from(v);
                }
            }
            // the mean of nine `i16` values is an `i16`
            e = if void {
                ELV_MISSING
            } else {
                i16::try_from(sum / 9).unwrap_or(ELV_MISSING)
            };
            out.extend_from_slice(&e.to_be_bytes());
        }
        // point #1201 (`:1288-1290`)
        out.extend_from_slice(&e.to_be_bytes());
    }
    for j in 0..SRTM3_SIDE {
        out.extend_from_slice(&at(SRTM1_SIDE - 1, j * 3).to_be_bytes());
    }
    Ok(out)
}
