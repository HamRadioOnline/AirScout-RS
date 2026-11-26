//! Port of `ElevationCatalogue.cs`: list of available tiles on the update server.

use crate::{ElevationError, ElevationModel};
use as_geo::latlon::GRect;
use as_geo::maidenhead;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Default update server (`Elevation_*_UpdateURL` settings without the model suffix).
pub const DEFAULT_BASE_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/ElevationData";

pub use as_net::user_agent;

/// Largest `files.cat` text read [bytes]. A downloaded zip is untrusted, so its entry is
/// never inflated beyond this. The size of the real catalogues has not been measured; the cap
/// is set far above any plausible one (one line per tile, about 32 bytes a line).
pub const MAX_CATALOGUE_BYTES: u64 = 1024 * 1024 * 1024;

/// A reader that fails once more than `cap` bytes have come through, instead of truncating.
pub struct Capped<R> {
    inner: R,
    cap: u64,
    left: u64,
}

impl<R> Capped<R> {
    /// Wrap `inner`; a stream of exactly `cap` bytes passes.
    pub fn new(inner: R, cap: u64) -> Self {
        Self {
            inner,
            cap,
            left: cap,
        }
    }
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Ask for one byte more than allowed, so a stream of exactly `cap` bytes passes.
        let want = buf
            .len()
            .min(usize::try_from(self.left + 1).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        if n as u64 > self.left {
            return Err(std::io::Error::other(format!(
                "catalogue larger than {} bytes",
                self.cap
            )));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

/// `GetLocsFromRect`: locators covering a rectangle. `precision` 1..=3 (2 = 4-char squares).
pub fn get_locs_from_rect(r: GRect, precision: i32) -> Result<Vec<String>, ElevationError> {
    let mut a = Vec::new();
    if r.max_lat <= r.min_lat || r.max_lon <= r.min_lon {
        return Ok(a);
    }
    let (step_lat, step_lon) = match precision {
        1 => (10.0, 20.0),
        2 => (1.0, 2.0),
        3 => (1.0 / 24.0, 2.0 / 24.0),
        p => {
            return Err(ElevationError::InvalidArgument(format!(
                "illegal precision value (1..3): {p}"
            )))
        }
    };
    let mut lat = r.min_lat;
    while lat <= r.max_lat {
        let mut lon = r.min_lon;
        while lon <= r.max_lon {
            if let Some(l) =
                maidenhead::loc_from_lat_lon(lat + 0.02, lon + 0.04, false, precision, false)?
            {
                a.push(l);
            }
            lon += step_lon;
        }
        lat += step_lat;
    }
    Ok(a)
}

/// Tile catalogue of one elevation model, restricted to a rectangle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalogue {
    pub base_url: String,
    /// Tile index (6 chars) -> last update on the server.
    pub files: BTreeMap<String, DateTime<Utc>>,
    /// Modification time of the downloaded catalogue file.
    pub last_modified: Option<DateTime<Utc>>,
}

impl Catalogue {
    /// Download `{base}/{MODEL}/files.zip` into `cache_dir` (only if newer than the cached copy)
    /// and parse the `files.cat` inside, keeping tiles whose 4-char square lies in `rect`.
    pub fn fetch(
        client: &reqwest::blocking::Client,
        base_url: &str,
        model: ElevationModel,
        rect: GRect,
        cache_dir: &Path,
    ) -> Result<Catalogue, ElevationError> {
        let base = format!("{}/{}", base_url.trim_end_matches('/'), model.dir_name());
        let url = format!("{base}/files.zip");
        std::fs::create_dir_all(cache_dir)?;
        let zip_path = cache_dir.join(format!("{}-files.zip", model.dir_name().to_lowercase()));
        as_net::download_if_newer(client, &url, &zip_path)?;
        let squares: BTreeSet<String> = get_locs_from_rect(rect, 2)?.into_iter().collect();
        // Filter while reading, so the map holds only the area's tiles (the old loop did the
        // same), and read through the cap: the downloaded zip is untrusted.
        let files = read_files_zip_where(&zip_path, MAX_CATALOGUE_BYTES, |index| {
            index.get(..4).is_some_and(|sq| squares.contains(sq))
        })?;
        let last_modified = std::fs::metadata(&zip_path)
            .and_then(|m| m.modified())
            .ok()
            .map(DateTime::<Utc>::from);
        Ok(Catalogue {
            base_url: base,
            files,
            last_modified,
        })
    }
}

/// Read a `files.cat` (`ElevationCatalogue.cs:45`): `<name>.loc;yyyy-MM-dd HH:mm:ssZ` lines.
/// Key is the name without `.loc`, upper-cased. Empty lines, `/` comment lines, lines without
/// `;` and lines with a bad date are skipped, as the original swallows them. The first entry
/// of a name wins.
pub fn read_files_cat(r: impl BufRead) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError> {
    read_files_cat_where(r, |_| true)
}

/// [`read_files_cat`], keeping only the entries whose key `keep` accepts. The filter runs
/// while reading, so a dropped entry never enters the map.
pub fn read_files_cat_where(
    r: impl BufRead,
    mut keep: impl FnMut(&str) -> bool,
) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError> {
    let mut files = BTreeMap::new();
    for line in r.lines() {
        let line = line?;
        if line.is_empty() || line.starts_with('/') {
            continue;
        }
        let Some((name, ts)) = line.split_once(';') else {
            continue;
        };
        let Ok(t) = NaiveDateTime::parse_from_str(ts.trim(), "%Y-%m-%d %H:%M:%SZ") else {
            continue;
        };
        let index = name.trim_end_matches(".loc").to_uppercase();
        if keep(&index) {
            files.entry(index).or_insert(t.and_utc());
        }
    }
    Ok(files)
}

/// Read the `files.cat` entry of a catalogue zip (`files.zip`) with [`read_files_cat`],
/// through a [`Capped`] reader of [`MAX_CATALOGUE_BYTES`].
pub fn read_files_zip(path: &Path) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError> {
    read_files_zip_where(path, MAX_CATALOGUE_BYTES, |_| true)
}

/// [`read_files_zip`] with a filter ([`read_files_cat_where`]) and a byte cap on the entry.
/// A read error of the entry, the cap included, is `Corrupt` naming the zip.
fn read_files_zip_where(
    path: &Path,
    cap: u64,
    keep: impl FnMut(&str) -> bool,
) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError> {
    let corrupt =
        |e: zip::result::ZipError| ElevationError::Corrupt(format!("{}: {e}", path.display()));
    let file = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(file).map_err(corrupt)?;
    let entry = z.by_name("files.cat").map_err(corrupt)?;
    read_files_cat_where(BufReader::new(Capped::new(entry, cap)), keep).map_err(|e| match e {
        ElevationError::Io(e) => ElevationError::Corrupt(format!("{}: {e}", path.display())),
        e => e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const CAT: &str = "// h\nJO50AA.loc;2018-02-27 08:26:32Z\nJN58TD.loc;2018-02-27 08:26:33Z\njo50ab.loc;2018-02-27 08:26:34Z\nJO50AA.loc;2019-01-01 00:00:00Z\n";

    fn cat_zip(dir: &Path, text: &str) -> std::path::PathBuf {
        let path = dir.join("files.zip");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        z.start_file("files.cat", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(text.as_bytes()).unwrap();
        z.finish().unwrap();
        path
    }

    /// I1: entries outside the filter never enter the map; the first kept entry still wins.
    #[test]
    fn filter_applies_while_reading() {
        let mut asked = Vec::new();
        let map = read_files_cat_where(CAT.as_bytes(), |index| {
            asked.push(index.to_string());
            index.starts_with("JO50")
        })
        .unwrap();
        assert_eq!(asked, ["JO50AA", "JN58TD", "JO50AB", "JO50AA"]);
        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys, ["JO50AA", "JO50AB"]);
        assert_eq!(map["JO50AA"].to_rfc3339(), "2018-02-27T08:26:32+00:00");
    }

    /// I1: the zip entry is read through the cap; at the cap passes, one byte under fails.
    #[test]
    fn zip_cap_is_inclusive_and_over_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = cat_zip(dir.path(), CAT);
        let n = CAT.len() as u64;
        let at = read_files_zip_where(&path, n, |_| true).unwrap();
        assert_eq!(at.len(), 3);
        let over = read_files_zip_where(&path, n - 1, |_| true).unwrap_err();
        let msg = over.to_string();
        assert!(matches!(over, ElevationError::Corrupt(_)), "{over:?}");
        assert!(msg.contains("files.zip"), "{msg}");
        assert!(
            msg.contains(&format!("larger than {} bytes", n - 1)),
            "{msg}"
        );
    }

    /// The cap counts every byte, also the bytes of lines the filter drops.
    #[test]
    fn cap_counts_dropped_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = cat_zip(dir.path(), CAT);
        let r = read_files_zip_where(&path, CAT.len() as u64 - 1, |_| false);
        assert!(r.is_err(), "{r:?}");
    }
}
