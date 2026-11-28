#![allow(dead_code)]
//! Shared test helpers: load fixture tiles from `tests/fixtures/elevation`.
use as_elevation::{ElevationModel, ElevationTile};
use std::collections::BTreeMap;
use std::io::Read;

pub fn fixture_dir() -> std::path::PathBuf {
    as_golden::repo_root().join("tests/fixtures/elevation")
}

/// All fixture tiles keyed by (model, tile index).
pub fn fixture_tiles() -> BTreeMap<(ElevationModel, String), ElevationTile> {
    let mut out = BTreeMap::new();
    for (model, name) in [
        (ElevationModel::Globe, "GLOBE"),
        (ElevationModel::Srtm3, "SRTM3"),
    ] {
        let dir = fixture_dir().join(name).join("JO");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_string();
            let mut add = |json: &str| {
                let t = ElevationTile::from_loc_json(json).unwrap();
                out.insert((model, t.index.clone()), t);
            };
            if ext == "zip" {
                let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
                for i in 0..z.len() {
                    let mut s = String::new();
                    z.by_index(i).unwrap().read_to_string(&mut s).unwrap();
                    add(&s);
                }
            } else if ext == "loc" {
                add(&std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    out
}

pub fn model_from(s: &str) -> ElevationModel {
    match s {
        "GLOBE" => ElevationModel::Globe,
        "SRTM3" => ElevationModel::Srtm3,
        "SRTM1" => ElevationModel::Srtm1,
        "ASTER3" => ElevationModel::Aster3,
        "ASTER1" => ElevationModel::Aster1,
        o => panic!("model {o}"),
    }
}

/// Every `.loc` text of the fixture tree, as raw strings: each entry of the GLOBE square
/// zips and each loose `.loc` file. Returns `(name, text)`.
pub fn fixture_texts() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in ["GLOBE", "SRTM3"] {
        let dir = fixture_dir().join(name).join("JO");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            match path.extension().and_then(|e| e.to_str()) {
                Some("zip") => {
                    let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
                    for i in 0..z.len() {
                        let mut entry = z.by_index(i).unwrap();
                        let entry_name = format!("{file}/{}", entry.name());
                        let mut s = String::new();
                        entry.read_to_string(&mut s).unwrap();
                        out.push((entry_name, s));
                    }
                }
                Some("loc") => out.push((file, std::fs::read_to_string(&path).unwrap())),
                _ => {}
            }
        }
    }
    out
}
