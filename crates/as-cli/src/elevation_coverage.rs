//! `airscout-cli elevation-coverage`: port of the C# ElevationCoverageMapper
//! (`ElevationCoverageMapper/MainDlg.cs:87-214`). It lists the tiles of a model's tile
//! catalogue (`files.zip` or `files.cat`), filters them by an area and, with `--data`, counts
//! which of them the local store holds. The map of the C# tool becomes a GeoJSON file.
//!
//! The tool layer is a new design (owner decisions Q6, Q7): the C# tool always downloads the
//! catalogue into the working directory (`:89-104`), here the catalogue is a local file or, only
//! with `--download`, one GET into a private temp dir that is removed afterwards.

use crate::{parse_area, parse_model, Res};
use as_elevation::{read_files_cat, Capped, ElevationStore, DEFAULT_BASE_URL, MAX_CATALOGUE_BYTES};
use as_geo::latlon::GRect;
use as_geo::maidenhead;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// Map detail of the GeoJSON output.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// One polygon per 4-character square.
    Square,
    /// One polygon per 6-character tile.
    Tile,
}

#[derive(clap::Args)]
pub struct ElevationCoverageArgs {
    /// Elevation model: globe, srtm3, srtm1, aster3, aster1 (C# fixed GLOBE, `MainDlg.cs:22`).
    #[arg(long, default_value = "globe")]
    pub model: String,
    /// Local tile catalogue: a `files.zip` (by extension, any case) or a `files.cat`.
    #[arg(long)]
    pub catalogue: Option<PathBuf>,
    /// Download `<base-url>/<MODEL>/files.zip` once (the only network request of this command).
    #[arg(long)]
    pub download: bool,
    /// Update server base URL for `--download`.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    pub base_url: String,
    /// Data directory: compare the catalogue with the local elevation store.
    #[arg(long)]
    pub data: Option<PathBuf>,
    /// Area `minlat,minlon,maxlat,maxlon` (default: the world).
    #[arg(long)]
    pub area: Option<String>,
    /// Write a GeoJSON `FeatureCollection` to this file.
    #[arg(long)]
    pub geojson: Option<PathBuf>,
    /// GeoJSON detail.
    #[arg(long, value_enum, default_value = "square")]
    pub level: Level,
}

/// Cell of a 6-character tile: bottom-left corner plus 1/24° × 2/24° (`MainDlg.cs:172-177`),
/// which is `bounds_from_loc` of the tile. `None` unless `index6` is a 6-character locator.
/// The ASCII test keeps non-ASCII digits, which `check` accepts, out, so `index6[..4]` is safe.
pub fn tile_cell(index6: &str) -> Option<GRect> {
    if index6.len() != 6 || !index6.is_ascii() || !maidenhead::check(index6) {
        return None;
    }
    maidenhead::bounds_from_loc(index6).ok()
}

/// Strict overlap on both axes: a cell that only touches the area's edge is outside.
fn overlaps(c: &GRect, a: &GRect) -> bool {
    c.min_lat < a.max_lat && c.max_lat > a.min_lat && c.min_lon < a.max_lon && c.max_lon > a.min_lon
}

fn area_of(s: Option<&str>) -> Res<GRect> {
    let [a, b, c, d] = match s {
        Some(s) => parse_area(s)?,
        None => [-90.0, -180.0, 90.0, 180.0],
    };
    let lat = -90.0..=90.0;
    let lon = -180.0..=180.0;
    if !(a < c
        && b < d
        && lat.contains(&a)
        && lat.contains(&c)
        && lon.contains(&b)
        && lon.contains(&d))
    {
        return Err(format!(
            "bad area {a},{b},{c},{d}: need minlat < maxlat and minlon < maxlon, \
             lat in -90..90, lon in -180..180"
        )
        .into());
    }
    Ok(GRect::new(a, b, c, d))
}

/// Parse a `files.cat` stream with `read_files_cat`, streaming it through [`Capped`]: more
/// than `cap` bytes is an error naming `name`.
fn read_cat_capped(r: impl Read, cap: u64, name: &Path) -> Res<BTreeMap<String, DateTime<Utc>>> {
    Ok(read_files_cat(BufReader::new(Capped::new(r, cap)))
        .map_err(|e| format!("{}: {e}", name.display()))?)
}

/// Tiles of a `files.zip` (its `files.cat` entry) or a `files.cat`, bounded in size.
fn read_catalogue(path: &Path) -> Res<BTreeMap<String, DateTime<Utc>>> {
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if is_zip {
        let bad = |e: zip::result::ZipError| format!("{}: {e}", path.display());
        let mut z = zip::ZipArchive::new(file).map_err(bad)?;
        let entry = z.by_name("files.cat").map_err(bad)?;
        read_cat_capped(entry, MAX_CATALOGUE_BYTES, path)
    } else {
        read_cat_capped(file, MAX_CATALOGUE_BYTES, path)
    }
}

/// The temp dir of one download, removed when dropped (also on error). Only a dir this run
/// created is removed.
struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Res<Self> {
        let p = std::env::temp_dir().join(format!("airscout-cli-coverage-{}", std::process::id()));
        std::fs::create_dir(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        Ok(Self(p))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One `download_if_newer` of the catalogue zip into a fresh temp dir, then read it.
fn download(url: &str, model: &str) -> Res<BTreeMap<String, DateTime<Utc>>> {
    eprintln!("{model}: Downloading elevation tile catalogue");
    let client = reqwest::blocking::Client::builder()
        .user_agent(as_net::user_agent())
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let dir = TempDir::create()?;
    let zip = dir.0.join("files.zip");
    as_net::download_if_newer(&client, url, &zip, MAX_CATALOGUE_BYTES)?;
    read_catalogue(&zip)
}

/// Write `v` to `path` through `<path>.part`.
fn write_json(path: &Path, v: &Value) -> Res<()> {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&part, path)?;
    Ok(())
}

fn polygon(r: &GRect, properties: Value) -> Value {
    json!({
        "type": "Feature",
        "properties": properties,
        "geometry": {
            "type": "Polygon",
            "coordinates": [[
                [r.min_lon, r.min_lat],
                [r.max_lon, r.min_lat],
                [r.max_lon, r.max_lat],
                [r.min_lon, r.max_lat],
                [r.min_lon, r.min_lat],
            ]],
        },
    })
}

/// A tile in the area: its index, cell and, with `--data`, whether the store holds it.
struct Tile {
    index: String,
    cell: GRect,
    found: Option<bool>,
}

fn geojson(tiles: &[Tile], level: Level) -> Res<Value> {
    let mut features = Vec::new();
    match level {
        Level::Tile => {
            for t in tiles {
                let state = match t.found {
                    None => "available",
                    Some(true) => "found",
                    Some(false) => "missing",
                };
                features.push(polygon(&t.cell, json!({"tile": t.index, "state": state})));
            }
        }
        Level::Square => {
            // square -> (tiles, found)
            let mut squares: BTreeMap<&str, (u64, Option<u64>)> = BTreeMap::new();
            for t in tiles {
                let e = squares.entry(&t.index[..4]).or_insert((0, None));
                e.0 += 1;
                if let Some(f) = t.found {
                    e.1 = Some(e.1.unwrap_or(0) + u64::from(f));
                }
            }
            for (sq, (n, found)) in squares {
                let r = maidenhead::bounds_from_loc(sq)?;
                let mut p = json!({"square": sq, "tiles": n});
                if let Some(f) = found {
                    p["found"] = json!(f);
                    p["missing"] = json!(n - f);
                }
                features.push(polygon(&r, p));
            }
        }
    }
    Ok(json!({"type": "FeatureCollection", "features": features}))
}

pub fn run(a: &ElevationCoverageArgs) -> Res<Value> {
    if a.catalogue.is_some() == a.download {
        let both = if a.download { ", not both" } else { "" };
        return Err(format!("give --catalogue <files.zip|files.cat> or --download{both}").into());
    }
    let area = area_of(a.area.as_deref())?;
    let model = parse_model(&a.model)?;
    let name = model.dir_name();
    let (files, source) = match &a.catalogue {
        Some(p) => (read_catalogue(p)?, p.display().to_string()),
        None => {
            let url = format!("{}/{name}/files.zip", a.base_url.trim_end_matches('/'));
            (download(&url, name)?, url)
        }
    };

    let mut invalid = 0u64;
    let mut tiles = Vec::new();
    for index in files.into_keys() {
        match tile_cell(&index) {
            None => invalid += 1,
            Some(cell) if overlaps(&cell, &area) => tiles.push(Tile {
                index,
                cell,
                found: None,
            }),
            Some(_) => {}
        }
    }

    let (mut found, mut missing) = (None, None);
    if let Some(data) = &a.data {
        let (mut f, mut m) = (0u64, 0u64);
        // A missing database means every tile is missing; nothing is created then.
        let store = if data.join(model.db_file_name()).is_file() {
            Some(ElevationStore::open(data)?)
        } else {
            None
        };
        for t in &mut tiles {
            let have = match &store {
                Some(s) => s.tile_exists(model, &t.index)?,
                None => false,
            };
            t.found = Some(have);
            if have {
                f += 1;
            } else {
                m += 1;
            }
        }
        eprintln!("{name}: {f} tile(s) found, {m} more tile(s) available and missing.");
        found = Some(f);
        missing = Some(m);
    }

    let squares = tiles
        .iter()
        .map(|t| &t.index[..4])
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if let Some(g) = &a.geojson {
        write_json(g, &geojson(&tiles, a.level)?)?;
    }
    Ok(json!({
        "model": name,
        "catalogue": source,
        "area": [area.min_lat, area.min_lon, area.max_lat, area.max_lon],
        "tiles": tiles.len(),
        "squares": squares,
        "invalid": invalid,
        "found": found,
        "missing": missing,
        "geojson": a.geojson.as_ref().map(|p| p.display().to_string()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_cap_is_inclusive_and_over_is_error() {
        let text = "JO50AA.loc;2018-02-27 08:26:32Z\nJO50AB.loc;2018-02-27 08:26:32Z\n";
        let n = text.len() as u64;
        let name = Path::new("files.cat");
        let at = read_cat_capped(text.as_bytes(), n, name).unwrap();
        assert_eq!(at.len(), 2);
        let over = read_cat_capped(text.as_bytes(), n - 1, name).unwrap_err();
        let msg = over.to_string();
        assert!(msg.contains("files.cat"), "{msg}");
        assert!(
            msg.contains(&format!("larger than {} bytes", n - 1)),
            "{msg}"
        );
    }

    #[test]
    fn tile_cell_is_the_tile_bounds() {
        let c = tile_cell("JO50AA").unwrap();
        assert_eq!((c.min_lat, c.min_lon), (50.0, 10.0));
        assert!((c.max_lat - (50.0 + 1.0 / 24.0)).abs() < 1e-9);
        assert!((c.max_lon - (10.0 + 2.0 / 24.0)).abs() < 1e-9);
        for bad in [
            "JO50",
            "JO50AA00",
            "XX99ZZ",
            "JO50A",
            "JO\u{b2}\u{b2}",
            "\u{c4}\u{d6}50AA",
            "",
        ] {
            assert_eq!(tile_cell(bad), None, "{bad}");
        }
    }
}
