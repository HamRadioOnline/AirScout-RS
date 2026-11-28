//! `import`: tiles from another `.gmdb` or a `<z>/<x>/<y>.png` tree into a `.gmdb` (no C#
//! counterpart; the reverse of `export`). Untrusted input: oversized, non-PNG/JPEG and misnamed
//! files are skipped with a message, symbolic links are not followed, existing keys are kept.

use crate::export::{plural, utc_of};
use crate::{type_name, Error};
use as_maps::{is_png_or_jpeg, MapsError, TileInfo, TileKey, TileStore, DBID_OSM, MAX_TILE_BYTES};
use chrono::{Local, NaiveDateTime};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

/// GMap's deepest zoom level (`1 << z` still fits an `int`).
const MAX_ZOOM: u32 = 30;

#[derive(Default)]
struct Counts {
    imported: u64,
    kept: u64,
    skipped: u64,
}

impl Counts {
    fn skip(&mut self, what: impl std::fmt::Display, why: impl std::fmt::Display) {
        self.skipped += 1;
        println!("skipped {what}: {why}");
    }

    fn insert(
        &mut self,
        store: &TileStore,
        k: TileKey,
        t: Option<NaiveDateTime>,
        data: &[u8],
    ) -> Result<(), MapsError> {
        if store.insert(k, t, data)? {
            self.imported += 1;
        } else {
            self.kept += 1;
        }
        Ok(())
    }
}

pub(crate) fn run(db: &Path, source: &Path, db_id: Option<i32>) -> Result<(), Error> {
    let meta = fs::metadata(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut c = Counts::default();
    if meta.is_dir() {
        let store = TileStore::open(db)?;
        from_tree(&store, source, db_id.unwrap_or(DBID_OSM), &mut c)?;
    } else {
        if let (Ok(a), Ok(b)) = (fs::canonicalize(db), fs::canonicalize(source)) {
            if a == b {
                return Err(format!("{} is the --db file itself", source.display()).into());
            }
        }
        // The source is checked before the target is created.
        let src = TileStore::open_read_only(source)?;
        let store = TileStore::open(db)?;
        from_gmdb(&store, &src, source, db_id, &mut c)?;
    }
    println!(
        "imported {}, kept {}, skipped {}",
        c.imported, c.kept, c.skipped
    );
    Ok(())
}

fn tile_name(t: &TileInfo) -> String {
    let k = t.key;
    format!(
        "tile {}/{}/{} of type {} (id {})",
        k.zoom,
        k.x,
        k.y,
        type_name(k.db_id),
        t.id
    )
}

/// Every tile of `source` (of the type `db_id`, if given) with its `CacheTime`.
fn from_gmdb(
    store: &TileStore,
    src: &TileStore,
    source: &Path,
    db_id: Option<i32>,
    c: &mut Counts,
) -> Result<(), Error> {
    let rows = src.count()?;
    let all = src.list()?;
    let no_key = rows.saturating_sub(all.len() as u64);
    if no_key > 0 {
        c.skipped += no_key;
        println!(
            "skipped {} of {}: X, Y, Zoom or Type out of range",
            plural(no_key, "row"),
            source.display()
        );
    }
    for t in all
        .iter()
        .filter(|t| db_id.is_none_or(|d| t.key.db_id == d))
    {
        // The key checks of a tree import.
        if let Some(why) = key_problem(t.key) {
            c.skip(tile_name(t), why);
            continue;
        }
        match src.get_by_id(t.id) {
            Ok(Some(data)) if is_png_or_jpeg(&data) => {
                c.insert(store, t.key, t.cache_time, &data)?
            }
            Ok(Some(_)) => c.skip(tile_name(t), "not a PNG or JPEG"),
            Ok(None) => c.skip(tile_name(t), "no data"),
            Err(e @ MapsError::TooLarge { .. }) => c.skip(tile_name(t), e),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Why `k` is no tile: zoom above [`MAX_ZOOM`], x or y not below `2^z`.
fn key_problem(k: TileKey) -> Option<String> {
    let z = u32::from(k.zoom);
    if z > MAX_ZOOM {
        return Some(format!("zoom above {MAX_ZOOM}"));
    }
    let side = 1u32 << z;
    if k.x >= side {
        Some(format!("x is not below 2^{z}"))
    } else if k.y >= side {
        Some(format!("y is not below 2^{z}"))
    } else {
        None
    }
}

/// The entries of a subdirectory; an unreadable one is skipped with a message.
fn sub_entries(dir: &Path, c: &mut Counts) -> Vec<(OsString, PathBuf)> {
    entries(dir).unwrap_or_else(|e| {
        c.skip(dir.display(), e);
        Vec::new()
    })
}

/// The entries of `dir`, sorted by name.
fn entries(dir: &Path) -> Result<Vec<(OsString, PathBuf)>, Error> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", dir.display()))?;
        out.push((e.file_name(), e.path()));
    }
    out.sort();
    Ok(out)
}

/// A tile number: decimal digits only.
fn number(name: &str) -> Option<u32> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name.parse().ok()
}

/// The number a `<z>` or `<x>` directory is named by; anything else is skipped.
fn dir_number(path: &Path, name: &OsString, c: &mut Counts) -> Option<u32> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            c.skip(path.display(), "symbolic link, not followed")
        }
        Ok(m) if !m.is_dir() => c.skip(path.display(), "not a directory"),
        Ok(_) => match name.to_str().and_then(number) {
            Some(n) => return Some(n),
            None => c.skip(path.display(), "name is not a number"),
        },
        Err(e) => c.skip(path.display(), e),
    }
    None
}

/// `<root>/<z>/<x>/<y>.png`; the cache time is the file's mtime (local time).
fn from_tree(store: &TileStore, root: &Path, db_id: i32, c: &mut Counts) -> Result<(), Error> {
    for (z_name, z_path) in entries(root)? {
        let Some(z) = dir_number(&z_path, &z_name, c) else {
            continue;
        };
        if z > MAX_ZOOM {
            c.skip(z_path.display(), format!("zoom above {MAX_ZOOM}"));
            continue;
        }
        let side = 1u32 << z;
        for (x_name, x_path) in sub_entries(&z_path, c) {
            let Some(x) = dir_number(&x_path, &x_name, c) else {
                continue;
            };
            if x >= side {
                c.skip(x_path.display(), format!("x is not below 2^{z}"));
                continue;
            }
            for (y_name, y_path) in sub_entries(&x_path, c) {
                let y = y_name
                    .to_str()
                    .and_then(|n| n.strip_suffix(".png"))
                    .and_then(number);
                let Some(y) = y else {
                    c.skip(y_path.display(), "not named <number>.png");
                    continue;
                };
                if y >= side {
                    c.skip(y_path.display(), format!("y is not below 2^{z}"));
                    continue;
                }
                let k = TileKey {
                    x,
                    y,
                    zoom: z as u8,
                    db_id,
                };
                from_file(store, k, &y_path, c)?;
            }
        }
    }
    Ok(())
}

fn from_file(store: &TileStore, k: TileKey, path: &Path, c: &mut Counts) -> Result<(), Error> {
    let shown = path.display();
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            c.skip(shown, e);
            return Ok(());
        }
    };
    if meta.file_type().is_symlink() {
        c.skip(shown, "symbolic link, not followed");
        return Ok(());
    }
    if !meta.is_file() {
        c.skip(shown, "not a file");
        return Ok(());
    }
    let too_large = format!("larger than {MAX_TILE_BYTES} bytes");
    if meta.len() > MAX_TILE_BYTES as u64 {
        c.skip(shown, too_large);
        return Ok(());
    }
    // Read under the cap, in case the file grew since `symlink_metadata`.
    let mut data = Vec::new();
    if let Err(e) =
        File::open(path).and_then(|f| f.take(MAX_TILE_BYTES as u64 + 1).read_to_end(&mut data))
    {
        c.skip(shown, e);
        return Ok(());
    }
    if data.len() > MAX_TILE_BYTES {
        c.skip(shown, too_large);
    } else if !is_png_or_jpeg(&data) {
        c.skip(shown, "not a PNG or JPEG");
    } else {
        let t = meta
            .modified()
            .ok()
            .and_then(utc_of)
            .map(|m| m.with_timezone(&Local).naive_local());
        c.insert(store, k, t, &data)?;
    }
    Ok(())
}
