//! Helpers shared by the `airscout-cli` integration tests.
#![allow(dead_code)]

use as_elevation::{ElevationModel, ElevationStore, ElevationTile};
use std::io::Read;

/// Data dir with the GLOBE fixture squares JO50, JO51, JO60, JO61.
pub fn fixture_data_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let src = as_golden::repo_root().join("tests/fixtures/elevation/GLOBE/JO");
    for e in std::fs::read_dir(src).unwrap() {
        let mut z = zip::ZipArchive::new(std::fs::File::open(e.unwrap().path()).unwrap()).unwrap();
        let mut tiles = Vec::new();
        for i in 0..z.len() {
            let mut s = String::new();
            z.by_index(i).unwrap().read_to_string(&mut s).unwrap();
            tiles.push(ElevationTile::from_loc_json(&s).unwrap());
        }
        store
            .insert_tiles_atomic(ElevationModel::Globe, &tiles)
            .unwrap();
    }
    dir
}
