//! The served tree: one zip of `.loc` tiles per square (`ElevationTileGenerator/MainDlg.cs:544-589`)
//! and the tile catalogue `files.cat` / `files.zip` (`:677-789`).
//!
//! Not reproduced from the C# tool: it deletes the `.loc` files and the folder of every square
//! once `Save` returns, without checking the archive (`:576-578`), and it takes the catalogue
//! times from file or entry creation times (`:690`, `:724`). Here the `.loc` files are deleted only with
//! `remove_loc`, after the new zip is read back and compared, and the times are each tile's
//! `LastUpdated` (findings 7 and 8).

use as_elevation::{ElevationError, ElevationTile};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The largest `.loc` text read from a file or zip entry (an SRTM1 tile is about 0.3 MB).
const MAX_LOC_BYTES: u64 = 16 << 20;

/// What `zip_squares` did.
#[derive(Debug, Default, PartialEq)]
pub struct ZipReport {
    /// Zips written.
    pub squares: usize,
    /// Entries in them.
    pub tiles: usize,
    /// `.loc` files deleted.
    pub removed: usize,
}

/// Where `scan_catalogue` reads the tiles from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CatFrom {
    /// The square zips (`CATFromZIP`, `:759`).
    Zip,
    /// The `.loc` files (`CAT`, `:729`).
    Loc,
}

pub(crate) fn corrupt(path: &Path, what: impl std::fmt::Display) -> ElevationError {
    ElevationError::Corrupt(format!("{}: {what}", path.display()))
}

/// The tile in a `.loc` text.
pub(crate) fn parse_loc(bytes: &[u8], path: &Path) -> Result<ElevationTile, ElevationError> {
    let text = std::str::from_utf8(bytes).map_err(|e| corrupt(path, e))?;
    ElevationTile::from_loc_json(text).map_err(|e| corrupt(path, e))
}

/// Read at most `MAX_LOC_BYTES` from `r`.
pub(crate) fn read_limited(r: impl Read, path: &Path) -> Result<Vec<u8>, ElevationError> {
    let mut b = Vec::new();
    r.take(MAX_LOC_BYTES + 1).read_to_end(&mut b)?;
    if b.len() as u64 > MAX_LOC_BYTES {
        return Err(corrupt(path, "larger than 16 MB"));
    }
    Ok(b)
}

/// A UTC time as zip date fields (2-second steps). Outside 1980..=2107 it is 1980-01-01.
fn zip_time(t: DateTime<Utc>) -> zip::DateTime {
    let f = |v: u32| u8::try_from(v).unwrap_or(u8::MAX);
    u16::try_from(t.year())
        .ok()
        .and_then(|y| {
            zip::DateTime::from_date_and_time(
                y,
                f(t.month()),
                f(t.day()),
                f(t.hour()),
                f(t.minute()),
                f(t.second()),
            )
            .ok()
        })
        .unwrap_or_default()
}

/// Whether `p` is a real directory (a symlink is not followed).
fn is_real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

/// The regular `*.loc` files directly in `dir`, sorted by name (symlinks are not followed).
pub(crate) fn loc_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, ElevationError> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        if !e.file_type()?.is_file() || path.extension().is_none_or(|x| x != "loc") {
            continue;
        }
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| corrupt(&path, "file name is not UTF-8"))?;
        out.push((name, path));
    }
    out.sort();
    Ok(out)
}

/// Write `<file>.part` with `write`, then rename it over `file`. On failure only the `.part`
/// is removed.
pub(crate) fn write_via_part(
    file: &Path,
    write: impl FnOnce(&mut File) -> Result<(), ElevationError>,
) -> Result<(), ElevationError> {
    let mut part = file.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let done = (|| {
        let mut f = File::create(&part)?;
        write(&mut f)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&part, file)?;
        Ok(())
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    done
}

/// Where an entry of a square zip comes from.
enum Src {
    /// A `.loc` file in the square folder.
    Disk(PathBuf),
    /// Entry `i` of the old square zip, which has no `.loc` file on disk.
    Old(usize),
}

/// Writes one entry into the new zip. A parameter, so that a test can write a bad zip.
pub(crate) type PutEntry<'a> = dyn FnMut(&mut ZipWriter<&mut File>, &str, &[u8], SimpleFileOptions) -> Result<(), ElevationError>
    + 'a;

/// The `PutEntry` of the tool.
pub(crate) fn put_entry(
    z: &mut ZipWriter<&mut File>,
    name: &str,
    bytes: &[u8],
    opts: SimpleFileOptions,
) -> Result<(), ElevationError> {
    z.start_file(name, opts)
        .map_err(|e| ElevationError::Corrupt(format!("{name}: {e}")))?;
    z.write_all(bytes)?;
    Ok(())
}

/// The tile in entry or file `name` of square `square`: it must parse and be the tile its name
/// says.
fn check_tile(
    bytes: &[u8],
    at: &Path,
    name: &str,
    square: &str,
) -> Result<ElevationTile, ElevationError> {
    let tile = parse_loc(bytes, at)?;
    let stem = name.strip_suffix(".loc").unwrap_or(name);
    if tile.index != stem || !stem.starts_with(square) {
        return Err(corrupt(
            at,
            format!("holds tile {:?}, not {stem:?}", tile.index),
        ));
    }
    Ok(tile)
}

/// The bytes of one source, and the path that names it in messages.
fn src_bytes(
    src: &Src,
    old: &mut Option<ZipArchive<File>>,
    zip_path: &Path,
) -> Result<(Vec<u8>, PathBuf), ElevationError> {
    match (src, old) {
        (Src::Disk(path), _) => Ok((read_limited(File::open(path)?, path)?, path.clone())),
        (Src::Old(i), Some(z)) => {
            let entry = z.by_index(*i).map_err(|e| corrupt(zip_path, e))?;
            let at = PathBuf::from(format!("{}: {}", zip_path.display(), entry.name()));
            Ok((read_limited(entry, &at)?, at))
        }
        (Src::Old(_), None) => Err(corrupt(zip_path, "old zip not open")),
    }
}

/// Read the new zip back: it must hold exactly `entries`, each byte-equal to its source.
fn verify_zip(
    part: &Path,
    entries: &BTreeMap<String, Src>,
    old: &mut Option<ZipArchive<File>>,
    zip_path: &Path,
) -> Result<(), ElevationError> {
    let mut z = ZipArchive::new(File::open(part)?).map_err(|e| corrupt(part, e))?;
    if z.len() != entries.len() {
        return Err(corrupt(
            part,
            format!("{} entries, expected {}", z.len(), entries.len()),
        ));
    }
    for (name, src) in entries {
        let entry = z.by_name(name).map_err(|e| corrupt(part, e))?;
        let in_zip = read_limited(entry, part)?;
        let (want, at) = src_bytes(src, old, zip_path)?;
        if in_zip != want {
            return Err(corrupt(
                &at,
                format!("differs from its entry in {}", part.display()),
            ));
        }
    }
    Ok(())
}

/// Zip square `square` (folder `dira`) as `<dirb>/<square>.zip`: flat names, sorted, Deflated,
/// the entry time from `LastUpdated`. The `.loc` entries of an old zip that have no file in
/// `dira` are carried over; a file replaces the old entry of its name (finding 7: the C# tool
/// rebuilds the zip from the files only, `:566-571`). The new zip is written as `.zip.part`,
/// read back and compared with every source, and only then renamed over the old one. With
/// `remove_loc` the `.loc` files are then deleted, and `dira` too if it is empty. On any
/// error the old zip and every `.loc` file stay. Returns (entries, files deleted).
pub(crate) fn pack_square(
    dirb: &Path,
    dira: &Path,
    square: &str,
    remove_loc: bool,
    put: &mut PutEntry,
) -> Result<(usize, usize), ElevationError> {
    let zip_path = dirb.join(format!("{square}.zip"));
    let mut part = zip_path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);

    let mut entries: BTreeMap<String, Src> = loc_files(dira)?
        .into_iter()
        .map(|(name, path)| (name, Src::Disk(path)))
        .collect();
    let mut old = if std::fs::symlink_metadata(&zip_path).is_ok() {
        Some(ZipArchive::new(File::open(&zip_path)?).map_err(|e| corrupt(&zip_path, e))?)
    } else {
        None
    };
    if let Some(z) = old.as_mut() {
        for i in 0..z.len() {
            let entry = z.by_index(i).map_err(|e| corrupt(&zip_path, e))?;
            let full = entry.name();
            if !full.to_ascii_lowercase().ends_with(".loc") {
                continue;
            }
            let name = full.rsplit(['/', '\\']).next().unwrap_or_default();
            entries.entry(name.to_string()).or_insert(Src::Old(i));
        }
    }

    let written = (|| {
        let mut f = File::create(&part)?;
        let mut z = ZipWriter::new(&mut f);
        for (name, src) in &entries {
            let (bytes, at) = src_bytes(src, &mut old, &zip_path)?;
            let tile = check_tile(&bytes, &at, name, square)?;
            let opts = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .last_modified_time(zip_time(tile.last_updated));
            put(&mut z, name, &bytes, opts)?;
        }
        z.finish().map_err(|e| corrupt(&part, e))?;
        f.sync_all()?;
        drop(f);
        verify_zip(&part, &entries, &mut old, &zip_path)?;
        // closed before the rename (Windows cannot replace an open file)
        drop(old);
        std::fs::rename(&part, &zip_path)?;
        Ok(())
    })();
    if let Err(e) = written {
        // only the `.part` this call created
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }

    let mut removed = 0;
    if remove_loc {
        for src in entries.values() {
            if let Src::Disk(path) = src {
                std::fs::remove_file(path)?;
                removed += 1;
            }
        }
        if std::fs::read_dir(dira)?.next().is_none() {
            std::fs::remove_dir(dira)?;
        }
    }
    Ok((entries.len(), removed))
}

/// `ZIP` (`:544-589`): for every square `<out>/AB/CD` (`AA..=RR`, `00..=99`, C# order) that
/// holds `*.loc` files, write `<out>/AB/ABCD.zip` with [`pack_square`]: verified before it
/// replaces the old zip, old tiles kept, the `.loc` files deleted only with `remove_loc`.
pub fn zip_squares(out: &Path, remove_loc: bool) -> Result<ZipReport, ElevationError> {
    let mut report = ZipReport::default();
    for a0 in b'A'..=b'R' {
        for a1 in b'A'..=b'R' {
            let field = format!("{}{}", a0 as char, a1 as char);
            let dirb = out.join(&field);
            if !is_real_dir(&dirb) {
                continue;
            }
            for n in 0..100 {
                let digits = format!("{n:02}");
                let dira = dirb.join(&digits);
                if !is_real_dir(&dira) || loc_files(&dira)?.is_empty() {
                    continue;
                }
                let square = format!("{field}{digits}");
                let (tiles, removed) =
                    pack_square(&dirb, &dira, &square, remove_loc, &mut put_entry)?;
                report.squares += 1;
                report.tiles += tiles;
                report.removed += removed;
            }
        }
    }
    Ok(report)
}

/// The entries and directories of `dir`, sorted by name; symlinks are skipped.
fn sorted_entries(dir: &Path) -> Result<Vec<(PathBuf, bool)>, ElevationError> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let t = e.file_type()?;
        if t.is_dir() || t.is_file() {
            out.push((e.path(), t.is_dir()));
        }
    }
    out.sort();
    Ok(out)
}

fn add(
    files: &mut BTreeMap<String, (DateTime<Utc>, String)>,
    name: String,
    time: DateTime<Utc>,
    from: String,
) -> Result<(), ElevationError> {
    if let Some((_, first)) = files.get(&name) {
        return Err(ElevationError::Corrupt(format!(
            "tile {name} is in {first} and {from}"
        )));
    }
    files.insert(name, (time, from));
    Ok(())
}

fn scan_zip(
    path: &Path,
    files: &mut BTreeMap<String, (DateTime<Utc>, String)>,
) -> Result<(), ElevationError> {
    let mut z = ZipArchive::new(File::open(path)?).map_err(|e| corrupt(path, e))?;
    for i in 0..z.len() {
        let entry = z.by_index(i).map_err(|e| corrupt(path, e))?;
        let full = entry.name().to_string();
        if !full.to_ascii_lowercase().ends_with(".loc") {
            continue;
        }
        // `Path.GetFileName` (`:724`)
        let name = full
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_string();
        let at = PathBuf::from(format!("{}: {full}", path.display()));
        let tile = parse_loc(&read_limited(entry, &at)?, &at)?;
        add(files, name, tile.last_updated, path.display().to_string())?;
    }
    Ok(())
}

/// `ScanDir`/`ScanDirZIP` (`:677-726`): every tile under `out` (recursive, symlinks not
/// followed) by file or entry name, with its `LastUpdated`. `Zip` reads the `.loc` entries of
/// every `*.zip` except `<out>/files.zip`; `Loc` reads every `*.loc`. A name seen twice is an
/// error.
pub fn scan_catalogue(
    out: &Path,
    from: CatFrom,
) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError> {
    let mut files = BTreeMap::new();
    let root_zip = out.join("files.zip");
    let mut stack = vec![out.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = sorted_entries(&dir)?;
        // directories after the files of this one, in name order
        for (path, _) in entries.iter().filter(|(_, d)| *d).rev() {
            stack.push(path.clone());
        }
        for (path, _) in entries.iter().filter(|(_, d)| !*d) {
            let Some(ext) = path.extension() else {
                continue;
            };
            match from {
                CatFrom::Zip if ext == "zip" && *path != root_zip => scan_zip(path, &mut files)?,
                CatFrom::Loc if ext == "loc" => {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .ok_or_else(|| corrupt(path, "file name is not UTF-8"))?
                        .to_string();
                    let tile = parse_loc(&read_limited(File::open(path)?, path)?, path)?;
                    add(
                        &mut files,
                        name,
                        tile.last_updated,
                        path.display().to_string(),
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(files.into_iter().map(|(k, (t, _))| (k, t)).collect())
}

/// The `files.cat` text (`:738-748`) with lines ending in `\r\n`. The header names this tool;
/// it claims no copyright and no ASTER void filling.
pub fn catalogue_text(files: &BTreeMap<String, DateTime<Utc>>, created: DateTime<Utc>) -> String {
    let mut s = String::new();
    let mut line = |l: &str| {
        s.push_str(l);
        s.push_str("\r\n");
    };
    line("// (D)igital (E)levation (Model) tile catalogue");
    line(&format!(
        "// created {} with elevation-tile-gen (AirScout-RS)",
        created.format("%Y-%m-%d")
    ));
    line("// GLOBE data homepage: https://www.ngdc.noaa.gov/mgg/topo/globe.html");
    line("// SRTM3 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl3_v003");
    line("// SRTM1 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl1_v003");
    line("");
    for (name, t) in files {
        line(&format!("{name};{}", t.format("%Y-%m-%d %H:%M:%SZ")));
    }
    s
}

/// Write `<out>/files.cat` and `<out>/files.zip` with the single entry `files.cat`
/// (`:751-756`), both via `.part`.
pub fn write_catalogue(out: &Path, text: &str) -> io::Result<()> {
    let to_io = |e: ElevationError| match e {
        ElevationError::Io(e) => e,
        e => io::Error::other(e.to_string()),
    };
    write_via_part(&out.join("files.cat"), |f| {
        f.write_all(text.as_bytes())?;
        Ok(())
    })
    .map_err(to_io)?;
    let zip_path = out.join("files.zip");
    write_via_part(&zip_path, |f| {
        let mut z = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        z.start_file("files.cat", opts)
            .map_err(|e| corrupt(&zip_path, e))?;
        z.write_all(text.as_bytes())?;
        z.finish().map_err(|e| corrupt(&zip_path, e))?;
        Ok(())
    })
    .map_err(to_io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree `<tmp>/JO/50` with the GLOBE-shaped tiles `JO50AA` and `JO50AB`.
    fn square_tree() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<PathBuf>) {
        let tmp = tempfile::tempdir().unwrap();
        let t = DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .unwrap()
            .to_utc();
        let locs = ["JO50AA", "JO50AB"]
            .iter()
            .map(|i| {
                let elv: Vec<i16> = (0..50).collect();
                let tile =
                    ElevationTile::new(i, 0, 50.0, 10.0, 49, 50.0, 10.0, 5, 10, elv, t).unwrap();
                crate::tiles::write_loc(tmp.path(), &tile).unwrap()
            })
            .collect();
        let dirb = tmp.path().join("JO");
        let dira = dirb.join("50");
        (tmp, dirb, dira, locs)
    }

    fn assert_nothing_deleted(dirb: &Path, locs: &[PathBuf], old: Option<&[u8]>) {
        for p in locs {
            assert!(p.exists(), "{}", p.display());
        }
        let zip = dirb.join("JO50.zip");
        match old {
            Some(b) => assert_eq!(std::fs::read(&zip).unwrap(), b),
            None => assert!(!zip.exists()),
        }
        assert!(!dirb.join("JO50.zip.part").exists());
    }

    #[test]
    fn pack_square_deletes_after_verify() {
        let (_tmp, dirb, dira, locs) = square_tree();
        let r = pack_square(&dirb, &dira, "JO50", true, &mut put_entry).unwrap();
        assert_eq!(r, (2, 2));
        assert!(locs.iter().all(|p| !p.exists()));
        assert!(!dira.exists());
    }

    #[test]
    fn pack_square_bad_entry_deletes_nothing() {
        let (_tmp, dirb, dira, locs) = square_tree();
        // no old zip: a changed byte
        let mut flip = |z: &mut ZipWriter<&mut File>, n: &str, b: &[u8], o| {
            let mut b = b.to_vec();
            b[0] ^= 1;
            put_entry(z, n, &b, o)
        };
        let e = pack_square(&dirb, &dira, "JO50", true, &mut flip).unwrap_err();
        assert!(
            e.to_string().contains("JO50AA.loc: differs from its entry"),
            "{e}"
        );
        assert_nothing_deleted(&dirb, &locs, None);

        // an old zip: it stays as it was
        pack_square(&dirb, &dira, "JO50", false, &mut put_entry).unwrap();
        let old = std::fs::read(dirb.join("JO50.zip")).unwrap();
        let e = pack_square(&dirb, &dira, "JO50", true, &mut flip).unwrap_err();
        assert!(e.to_string().contains("differs from its entry"), "{e}");
        assert_nothing_deleted(&dirb, &locs, Some(&old));

        // a dropped entry
        let mut drop_ab = |z: &mut ZipWriter<&mut File>, n: &str, b: &[u8], o| {
            if n == "JO50AB.loc" {
                return Ok(());
            }
            put_entry(z, n, b, o)
        };
        let e = pack_square(&dirb, &dira, "JO50", true, &mut drop_ab).unwrap_err();
        assert!(e.to_string().contains("1 entries, expected 2"), "{e}");
        assert_nothing_deleted(&dirb, &locs, Some(&old));
    }

    #[test]
    fn zip_time_fields() {
        let t = DateTime::parse_from_rfc3339("2026-11-24T12:34:57Z")
            .unwrap()
            .to_utc();
        let z = zip_time(t);
        assert_eq!(
            (
                z.year(),
                z.month(),
                z.day(),
                z.hour(),
                z.minute(),
                z.second()
            ),
            (2026, 11, 24, 12, 34, 56)
        );
        let old = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        assert_eq!(zip_time(old), zip::DateTime::default());
    }
}
